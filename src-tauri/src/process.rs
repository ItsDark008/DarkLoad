//! Safe child-process management.
//!
//! Processes are always spawned from an absolute executable path with an explicit argument
//! vector — never through a shell — so user input can never be interpreted as a command.
//! Each child runs in its own process group (Unix) or is terminated with its descendants
//! (Windows) so cancelling a download also stops FFmpeg instances started by yt-dlp.

use std::ffi::OsString;
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::errors::{AppError, AppResult, ErrorCode};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Builds a command with hardened defaults.
pub fn command(program: &Path, args: &[OsString]) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        // Force UTF-8 output from Python-based tools regardless of the system code page.
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONHOME");
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        cmd.process_group(0);
    }
    cmd
}

/// Terminates a child process and all of its descendants.
pub async fn kill_tree(child: &mut Child) {
    let Some(pid) = child.id() else {
        return;
    };
    #[cfg(windows)]
    {
        let system_root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let taskkill = system_root.join("System32").join("taskkill.exe");
        let _ = Command::new(taskkill)
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .await;
    }
    #[cfg(unix)]
    {
        let pgid = pid as libc::pid_t;
        // SAFETY: signalling a process group we created; failure is harmless.
        unsafe {
            libc::kill(-pgid, libc::SIGTERM);
        }
        if tokio::time::timeout(Duration::from_secs(3), child.wait())
            .await
            .is_err()
        {
            unsafe {
                libc::kill(-pgid, libc::SIGKILL);
            }
        }
    }
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug)]
pub struct Line {
    pub stream: Stream,
    pub text: String,
}

fn spawn_reader<R: AsyncRead + Unpin + Send + 'static>(
    reader: R,
    stream: Stream,
    tx: mpsc::Sender<Line>,
) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(reader);
        let mut buf = Vec::with_capacity(512);
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    // Progress output may use carriage returns; treat them as line breaks.
                    let text = String::from_utf8_lossy(&buf);
                    for part in text.split(['\r', '\n']).filter(|p| !p.trim().is_empty()) {
                        if tx
                            .send(Line {
                                stream,
                                text: part.to_string(),
                            })
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
        }
    });
}

/// Outcome of a completed process.
#[derive(Debug)]
pub struct Finished {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr_tail: String,
}

const STDERR_TAIL_LINES: usize = 60;
const MAX_CAPTURED_STDOUT: usize = 64 * 1024 * 1024;

/// Runs a process to completion, delivering each output line to `on_line`.
///
/// When `capture_stdout` is set, stdout is also accumulated (bounded) and returned.
pub async fn run<F>(
    program: &Path,
    args: &[OsString],
    cwd: Option<&Path>,
    cancel: &CancellationToken,
    timeout: Option<Duration>,
    capture_stdout: bool,
    mut on_line: F,
) -> AppResult<Finished>
where
    F: FnMut(&Line),
{
    let mut cmd = command(program, args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let mut child = cmd.spawn().map_err(|e| {
        AppError::new(
            ErrorCode::DependencyFailed,
            format!("Failed to start {}.", program_name(program)),
        )
        .with_detail(e.to_string())
    })?;

    let (tx, mut rx) = mpsc::channel::<Line>(256);
    if let Some(out) = child.stdout.take() {
        spawn_reader(out, Stream::Stdout, tx.clone());
    }
    if let Some(err) = child.stderr.take() {
        spawn_reader(err, Stream::Stderr, tx.clone());
    }
    drop(tx);

    let mut stdout = String::new();
    let mut stderr_tail: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    let deadline = timeout.map(|t| tokio::time::Instant::now() + t);

    loop {
        let sleep = async {
            match deadline {
                Some(d) => tokio::time::sleep_until(d).await,
                None => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                kill_tree(&mut child).await;
                return Err(AppError::cancelled());
            }
            _ = sleep => {
                kill_tree(&mut child).await;
                return Err(AppError::new(ErrorCode::Network, "The operation timed out. Check your connection and try again."));
            }
            line = rx.recv() => match line {
                Some(line) => {
                    on_line(&line);
                    match line.stream {
                        Stream::Stdout if capture_stdout => {
                            if stdout.len() + line.text.len() > MAX_CAPTURED_STDOUT {
                                kill_tree(&mut child).await;
                                return Err(AppError::internal("The extractor produced an unexpectedly large response."));
                            }
                            stdout.push_str(&line.text);
                            stdout.push('\n');
                        }
                        Stream::Stderr => {
                            if stderr_tail.len() == STDERR_TAIL_LINES {
                                stderr_tail.pop_front();
                            }
                            stderr_tail.push_back(line.text);
                        }
                        _ => {}
                    }
                }
                None => break,
            }
        }
    }

    let status = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            kill_tree(&mut child).await;
            return Err(AppError::cancelled());
        }
        status = child.wait() => status?,
    };
    Ok(Finished {
        success: status.success(),
        code: status.code(),
        stdout,
        stderr_tail: stderr_tail.into_iter().collect::<Vec<_>>().join("\n"),
    })
}

/// Runs a short command and returns its trimmed stdout (used for version checks).
pub async fn output(program: &Path, args: &[&str], timeout: Duration) -> AppResult<String> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let cancel = CancellationToken::new();
    let finished = run(program, &args, None, &cancel, Some(timeout), true, |_| {}).await?;
    if !finished.success {
        return Err(AppError::new(
            ErrorCode::DependencyFailed,
            format!("{} exited with an error.", program_name(program)),
        )
        .with_detail(finished.stderr_tail));
    }
    Ok(finished.stdout.trim().to_string())
}

pub fn program_name(program: &Path) -> String {
    program
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "process".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn shell() -> (PathBuf, Vec<OsString>) {
        #[cfg(windows)]
        {
            let root = std::env::var_os("SystemRoot")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
            (root.join("System32").join("cmd.exe"), vec!["/C".into()])
        }
        #[cfg(unix)]
        {
            (PathBuf::from("/bin/sh"), vec!["-c".into()])
        }
    }

    #[tokio::test]
    async fn captures_output_lines() {
        let (prog, mut args) = shell();
        args.push("echo hello".into());
        let cancel = CancellationToken::new();
        let mut seen = Vec::new();
        let done = run(
            &prog,
            &args,
            None,
            &cancel,
            Some(Duration::from_secs(20)),
            true,
            |l| seen.push(l.text.clone()),
        )
        .await
        .unwrap();
        assert!(done.success);
        assert!(done.stdout.contains("hello"));
        assert!(seen.iter().any(|l| l.contains("hello")));
    }

    #[tokio::test]
    async fn cancellation_terminates_child() {
        let (prog, mut args) = shell();
        #[cfg(windows)]
        args.push("ping -n 30 127.0.0.1 > NUL".into());
        #[cfg(unix)]
        args.push("sleep 30".into());
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            trigger.cancel();
        });
        let started = std::time::Instant::now();
        let err = run(&prog, &args, None, &cancel, None, false, |_| {})
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(15));
    }

    #[tokio::test]
    async fn timeouts_are_reported() {
        let (prog, mut args) = shell();
        #[cfg(windows)]
        args.push("ping -n 30 127.0.0.1 > NUL".into());
        #[cfg(unix)]
        args.push("sleep 30".into());
        let cancel = CancellationToken::new();
        let err = run(
            &prog,
            &args,
            None,
            &cancel,
            Some(Duration::from_millis(300)),
            false,
            |_| {},
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::Network);
    }

    #[tokio::test]
    async fn missing_program_is_a_dependency_error() {
        let cancel = CancellationToken::new();
        let err = run(
            Path::new("/definitely/not/here/yt-dlp"),
            &[],
            None,
            &cancel,
            None,
            false,
            |_| {},
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::DependencyFailed);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn arguments_are_not_shell_interpreted() {
        // An argument containing shell metacharacters is delivered verbatim as $0.
        let (prog, mut args) = shell();
        args.push("printf '%s' \"$0\"".into());
        args.push("a;b&&c|d".into());
        let cancel = CancellationToken::new();
        let done = run(
            &prog,
            &args,
            None,
            &cancel,
            Some(Duration::from_secs(20)),
            true,
            |_| {},
        )
        .await
        .unwrap();
        assert!(done.stdout.contains("a;b"));
    }
}
