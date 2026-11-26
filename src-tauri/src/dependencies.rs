//! Managed external tools: yt-dlp, FFmpeg/FFprobe and the Deno JavaScript runtime.
//!
//! Tools are provisioned into DarkLoad's own data directory from their official GitHub release
//! channels and verified against the SHA-256 checksums published alongside each release.
//! System-wide installations are only used when the user explicitly allows it.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};

use crate::errors::{AppError, AppResult, ErrorCode};
use crate::extractor::ToolPaths;
use crate::process;

const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
const VERSION_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DepId {
    #[serde(rename = "yt-dlp")]
    YtDlp,
    Ffmpeg,
    Ffprobe,
    Deno,
}

impl DepId {
    pub const ALL: [DepId; 4] = [DepId::YtDlp, DepId::Ffmpeg, DepId::Ffprobe, DepId::Deno];

    pub fn key(self) -> &'static str {
        match self {
            DepId::YtDlp => "yt-dlp",
            DepId::Ffmpeg => "ffmpeg",
            DepId::Ffprobe => "ffprobe",
            DepId::Deno => "deno",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            DepId::YtDlp => "yt-dlp",
            DepId::Ffmpeg => "FFmpeg",
            DepId::Ffprobe => "FFprobe",
            DepId::Deno => "Deno",
        }
    }

    fn description(self) -> &'static str {
        match self {
            DepId::YtDlp => "Media extraction and downloading engine.",
            DepId::Ffmpeg => "Merges streams, converts audio and embeds metadata.",
            DepId::Ffprobe => "Inspects media files during processing.",
            DepId::Deno => "JavaScript runtime yt-dlp uses to unlock YouTube formats.",
        }
    }

    fn required(self) -> bool {
        !matches!(self, DepId::Deno)
    }

    /// The package that provides this tool (FFprobe ships with FFmpeg).
    pub fn package(self) -> DepId {
        match self {
            DepId::Ffprobe => DepId::Ffmpeg,
            other => other,
        }
    }

    pub fn exe_name(self) -> String {
        format!("{}{}", self.key(), std::env::consts::EXE_SUFFIX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DepSource {
    Managed,
    System,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyStatus {
    pub id: DepId,
    pub name: String,
    pub description: String,
    pub required: bool,
    pub available: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub source: DepSource,
    pub installed_at: Option<i64>,
    pub error: Option<String>,
    pub latest_version: Option<String>,
    pub update_available: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub id: DepId,
    pub stage: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ManifestEntry {
    version: Option<String>,
    sha256: String,
    asset: String,
    source: String,
    installed_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Raw,
    Zip,
    TarXz,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Asset {
    repo: &'static str,
    file_name: String,
    checksum_file: String,
    kind: ArchiveKind,
}

/// Selects the official release asset for a package on the given OS and architecture.
fn asset_for(id: DepId, os: &str, arch: &str) -> Option<Asset> {
    let id = id.package();
    let (repo, file_name, checksum_file, kind) = match (id, os, arch) {
        (DepId::YtDlp, "windows", "x86_64") => (
            "yt-dlp/yt-dlp",
            "yt-dlp.exe",
            "SHA2-256SUMS",
            ArchiveKind::Raw,
        ),
        (DepId::YtDlp, "windows", "aarch64") => (
            "yt-dlp/yt-dlp",
            "yt-dlp_arm64.exe",
            "SHA2-256SUMS",
            ArchiveKind::Raw,
        ),
        (DepId::YtDlp, "linux", "x86_64") => (
            "yt-dlp/yt-dlp",
            "yt-dlp_linux",
            "SHA2-256SUMS",
            ArchiveKind::Raw,
        ),
        (DepId::YtDlp, "linux", "aarch64") => (
            "yt-dlp/yt-dlp",
            "yt-dlp_linux_aarch64",
            "SHA2-256SUMS",
            ArchiveKind::Raw,
        ),
        (DepId::Ffmpeg, "windows", "x86_64") => (
            "yt-dlp/FFmpeg-Builds",
            "ffmpeg-master-latest-win64-gpl-shared.zip",
            "checksums.sha256",
            ArchiveKind::Zip,
        ),
        (DepId::Ffmpeg, "windows", "aarch64") => (
            "yt-dlp/FFmpeg-Builds",
            "ffmpeg-master-latest-winarm64-gpl-shared.zip",
            "checksums.sha256",
            ArchiveKind::Zip,
        ),
        (DepId::Ffmpeg, "linux", "x86_64") => (
            "yt-dlp/FFmpeg-Builds",
            "ffmpeg-master-latest-linux64-gpl.tar.xz",
            "checksums.sha256",
            ArchiveKind::TarXz,
        ),
        (DepId::Ffmpeg, "linux", "aarch64") => (
            "yt-dlp/FFmpeg-Builds",
            "ffmpeg-master-latest-linuxarm64-gpl.tar.xz",
            "checksums.sha256",
            ArchiveKind::TarXz,
        ),
        (DepId::Deno, "windows", "x86_64") => (
            "denoland/deno",
            "deno-x86_64-pc-windows-msvc.zip",
            "",
            ArchiveKind::Zip,
        ),
        (DepId::Deno, "windows", "aarch64") => (
            "denoland/deno",
            "deno-aarch64-pc-windows-msvc.zip",
            "",
            ArchiveKind::Zip,
        ),
        (DepId::Deno, "linux", "x86_64") => (
            "denoland/deno",
            "deno-x86_64-unknown-linux-gnu.zip",
            "",
            ArchiveKind::Zip,
        ),
        (DepId::Deno, "linux", "aarch64") => (
            "denoland/deno",
            "deno-aarch64-unknown-linux-gnu.zip",
            "",
            ArchiveKind::Zip,
        ),
        _ => return None,
    };
    let checksum_file = if checksum_file.is_empty() {
        format!("{file_name}.sha256sum")
    } else {
        checksum_file.to_string()
    };
    Some(Asset {
        repo,
        file_name: file_name.to_string(),
        checksum_file,
        kind,
    })
}

/// Extracts the SHA-256 for `file_name` from a checksum document.
///
/// Supports `sha256sum` output (`<hash>  <name>` / `<hash> *<name>`) and single-hash documents
/// such as PowerShell `Get-FileHash` output.
pub fn find_checksum(text: &str, file_name: &str) -> Option<String> {
    let is_hash = |s: &str| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit());
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(hash), Some(name)) = (parts.next(), parts.next()) {
            if is_hash(hash)
                && name.trim_start_matches('*').rsplit(['/', '\\']).next() == Some(file_name)
            {
                return Some(hash.to_ascii_lowercase());
            }
        }
    }
    let hashes: Vec<&str> = text
        .split(|c: char| !c.is_ascii_hexdigit())
        .filter(|t| is_hash(t))
        .collect();
    match hashes.as_slice() {
        [only] => Some(only.to_ascii_lowercase()),
        _ => None,
    }
}

/// Parses a version string from a tool's `--version` / `-version` output.
pub fn parse_version(id: DepId, output: &str) -> Option<String> {
    let first = output.lines().find(|l| !l.trim().is_empty())?.trim();
    match id {
        DepId::YtDlp => Some(first.to_string()),
        DepId::Ffmpeg | DepId::Ffprobe => {
            let mut tokens = first.split_whitespace();
            while let Some(t) = tokens.next() {
                if t == "version" {
                    return tokens.next().map(str::to_string);
                }
            }
            None
        }
        DepId::Deno => first.split_whitespace().nth(1).map(str::to_string),
    }
}

/// Which archive entries to install, keyed by their file name.
fn wanted_entry(package: DepId, entry_path: &str) -> Option<String> {
    let normalized = entry_path.replace('\\', "/");
    let name = normalized.rsplit('/').next()?.to_string();
    if name.is_empty() || name.starts_with('.') {
        return None;
    }
    let parent_is_bin = normalized
        .trim_end_matches(&name)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        == Some("bin");
    match package {
        DepId::Ffmpeg => {
            let exe = std::env::consts::EXE_SUFFIX;
            let wanted = [format!("ffmpeg{exe}"), format!("ffprobe{exe}")];
            let is_dll = name.to_ascii_lowercase().ends_with(".dll");
            (parent_is_bin && (wanted.contains(&name) || (cfg!(windows) && is_dll))).then_some(name)
        }
        DepId::Deno => (name == DepId::Deno.exe_name()).then_some(name),
        _ => None,
    }
}

pub struct DependencyManager {
    bin_dir: PathBuf,
    http: reqwest::Client,
    allow_system: AtomicBool,
    install_lock: Mutex<()>,
    statuses: RwLock<Vec<DependencyStatus>>,
    latest: RwLock<HashMap<DepId, (String, Option<String>)>>,
}

impl DependencyManager {
    pub fn new(bin_dir: PathBuf, allow_system: bool) -> AppResult<Self> {
        std::fs::create_dir_all(&bin_dir)?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("DarkLoad/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(60))
            .https_only(true)
            .build()
            .map_err(|e| {
                AppError::internal("Failed to initialize the network client.")
                    .with_detail(e.to_string())
            })?;
        Ok(Self {
            bin_dir,
            http,
            allow_system: AtomicBool::new(allow_system),
            install_lock: Mutex::new(()),
            statuses: RwLock::new(Vec::new()),
            latest: RwLock::new(HashMap::new()),
        })
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub fn bin_dir(&self) -> &Path {
        &self.bin_dir
    }

    pub fn set_allow_system(&self, allow: bool) {
        self.allow_system.store(allow, Ordering::Relaxed);
    }

    fn managed_path(&self, id: DepId) -> PathBuf {
        self.bin_dir.join(id.exe_name())
    }

    fn manifest_path(&self) -> PathBuf {
        self.bin_dir.join("manifest.json")
    }

    fn read_manifest(&self) -> HashMap<String, ManifestEntry> {
        std::fs::read_to_string(self.manifest_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn write_manifest(&self, manifest: &HashMap<String, ManifestEntry>) -> AppResult<()> {
        let tmp = self.bin_dir.join("manifest.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(manifest)?)?;
        crate::security::move_file(&tmp, &self.manifest_path(), true)
    }

    /// Locates an executable: managed copy first, then PATH when allowed.
    pub fn locate(&self, id: DepId) -> Option<(PathBuf, DepSource)> {
        let managed = self.managed_path(id);
        if managed.is_file() {
            return Some((managed, DepSource::Managed));
        }
        if self.allow_system.load(Ordering::Relaxed) {
            if let Some(found) = find_on_path(&id.exe_name()) {
                return Some((found, DepSource::System));
            }
        }
        None
    }

    async fn probe(&self, id: DepId) -> DependencyStatus {
        let manifest = self.read_manifest();
        let mut status = DependencyStatus {
            id,
            name: id.display_name().into(),
            description: id.description().into(),
            required: id.required(),
            available: false,
            version: None,
            path: None,
            source: DepSource::Missing,
            installed_at: None,
            error: None,
            latest_version: None,
            update_available: None,
        };
        let Some((path, source)) = self.locate(id) else {
            return status;
        };
        status.path = Some(path.to_string_lossy().into_owned());
        status.source = source;
        if source == DepSource::Managed {
            status.installed_at = manifest.get(id.package().key()).map(|m| m.installed_at);
        }
        let flag = match id {
            DepId::Ffmpeg | DepId::Ffprobe => "-version",
            _ => "--version",
        };
        match process::output(&path, &[flag], VERSION_TIMEOUT).await {
            Ok(out) => {
                status.version = parse_version(id, &out);
                status.available = true;
            }
            Err(e) => status.error = Some(e.detail.clone().unwrap_or(e.message)),
        }
        if let Some((latest, manifest_hash)) = self.latest.read().await.get(&id.package()).cloned()
        {
            status.latest_version = Some(latest.clone());
            if source == DepSource::Managed && status.available {
                status.update_available = Some(match id.package() {
                    DepId::Ffmpeg => {
                        let installed = manifest.get("ffmpeg").map(|m| m.sha256.clone());
                        manifest_hash.is_some() && installed != manifest_hash
                    }
                    _ => {
                        status.version.as_deref().map(|v| v.trim_start_matches('v'))
                            != Some(latest.trim_start_matches('v'))
                    }
                });
            }
        }
        status
    }

    /// Re-detects every tool and caches the result.
    pub async fn refresh(&self) -> Vec<DependencyStatus> {
        let results =
            futures_util::future::join_all(DepId::ALL.iter().map(|id| self.probe(*id))).await;
        *self.statuses.write().await = results.clone();
        results
    }

    pub async fn statuses(&self) -> Vec<DependencyStatus> {
        let cached = self.statuses.read().await.clone();
        if cached.is_empty() {
            self.refresh().await
        } else {
            cached
        }
    }

    /// Paths for extractor invocations. Fails when yt-dlp is unavailable.
    pub async fn tool_paths(&self, cache_dir: PathBuf) -> AppResult<ToolPaths> {
        let statuses = self.statuses().await;
        let get = |id: DepId| {
            statuses
                .iter()
                .find(|s| s.id == id && s.available)
                .and_then(|s| s.path.clone())
                .map(PathBuf::from)
        };
        let yt_dlp = get(DepId::YtDlp).ok_or_else(|| {
            AppError::new(
                ErrorCode::DependencyMissing,
                "yt-dlp is not installed. Install the required components to continue.",
            )
        })?;
        let ffmpeg = get(DepId::Ffmpeg).map(|p| p.parent().map(Path::to_path_buf).unwrap_or(p));
        Ok(ToolPaths {
            yt_dlp,
            ffmpeg,
            deno: get(DepId::Deno),
            cache_dir,
        })
    }

    pub async fn ffmpeg_available(&self) -> bool {
        self.statuses()
            .await
            .iter()
            .any(|s| s.id == DepId::Ffmpeg && s.available)
    }

    async fn latest_release(&self, repo: &str) -> AppResult<(String, String)> {
        #[derive(Deserialize)]
        struct Release {
            tag_name: String,
            name: Option<String>,
        }
        let release: Release = self
            .http
            .get(format!(
                "https://api.github.com/repos/{repo}/releases/latest"
            ))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await?
            .error_for_status()
            .map_err(|e| {
                AppError::new(ErrorCode::Network, "Could not reach the release server.")
                    .with_detail(e.to_string())
            })?
            .json()
            .await?;
        let name = release.name.unwrap_or_else(|| release.tag_name.clone());
        Ok((release.tag_name, name))
    }

    async fn fetch_text(&self, url: &str) -> AppResult<String> {
        let resp = self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()
            .map_err(|e| {
                AppError::new(ErrorCode::Network, "Could not download the checksum file.")
                    .with_detail(e.to_string())
            })?;
        let bytes = resp.bytes().await?;
        if bytes.len() > 1024 * 1024 {
            return Err(AppError::new(
                ErrorCode::ChecksumMismatch,
                "The checksum file is unexpectedly large.",
            ));
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Checks the official release channels for newer versions.
    pub async fn check_updates(&self) -> AppResult<Vec<DependencyStatus>> {
        let mut latest = HashMap::new();
        for package in [DepId::YtDlp, DepId::Ffmpeg, DepId::Deno] {
            let Some(asset) = asset_for(package, std::env::consts::OS, std::env::consts::ARCH)
            else {
                continue;
            };
            let (tag, name) = self.latest_release(asset.repo).await?;
            if package == DepId::Ffmpeg {
                let url = format!(
                    "https://github.com/{}/releases/download/{tag}/{}",
                    asset.repo, asset.checksum_file
                );
                let hash = find_checksum(&self.fetch_text(&url).await?, &asset.file_name);
                let label = name
                    .trim_start_matches("Latest Auto-Build")
                    .trim()
                    .trim_matches(['(', ')'])
                    .to_string();
                latest.insert(
                    package,
                    (
                        if label.is_empty() {
                            tag
                        } else {
                            format!("Build {label}")
                        },
                        hash,
                    ),
                );
            } else {
                latest.insert(package, (tag, None));
            }
        }
        *self.latest.write().await = latest;
        Ok(self.refresh().await)
    }

    /// Downloads, verifies and installs packages. Existing copies are replaced only after the
    /// new files are fully verified.
    pub async fn install<F>(
        &self,
        ids: &[DepId],
        on_progress: F,
    ) -> AppResult<Vec<DependencyStatus>>
    where
        F: Fn(InstallProgress) + Send + Sync,
    {
        let _guard = self
            .install_lock
            .try_lock()
            .map_err(|_| AppError::invalid_input("Components are already being installed."))?;
        let mut packages: Vec<DepId> = ids.iter().map(|id| id.package()).collect();
        packages.dedup();
        packages.sort_by_key(|p| *p as u8);
        packages.dedup();
        for package in packages {
            if let Err(e) = self.install_one(package, &on_progress).await {
                on_progress(InstallProgress {
                    id: package,
                    stage: "failed".into(),
                    downloaded: 0,
                    total: None,
                    message: Some(e.message.clone()),
                });
                self.refresh().await;
                return Err(e);
            }
        }
        Ok(self.refresh().await)
    }

    async fn install_one<F>(&self, package: DepId, on_progress: &F) -> AppResult<()>
    where
        F: Fn(InstallProgress) + Send + Sync,
    {
        let report = |stage: &str, downloaded: u64, total: Option<u64>| {
            on_progress(InstallProgress {
                id: package,
                stage: stage.into(),
                downloaded,
                total,
                message: None,
            });
        };
        let asset =
            asset_for(package, std::env::consts::OS, std::env::consts::ARCH).ok_or_else(|| {
                AppError::new(
                    ErrorCode::DependencyMissing,
                    format!(
                        "No official {} build is available for this system ({} {}).",
                        package.display_name(),
                        std::env::consts::OS,
                        std::env::consts::ARCH
                    ),
                )
            })?;
        report("resolving", 0, None);
        let tag = match self.latest_release(asset.repo).await {
            Ok((tag, _)) => tag,
            Err(e) => {
                tracing::warn!("release lookup failed for {}: {}", asset.repo, e.message);
                return Err(e);
            }
        };
        let base = format!("https://github.com/{}/releases/download/{tag}", asset.repo);
        let checksums = self
            .fetch_text(&format!("{base}/{}", asset.checksum_file))
            .await?;
        let expected = find_checksum(&checksums, &asset.file_name).ok_or_else(|| {
            AppError::new(
                ErrorCode::ChecksumMismatch,
                format!("No published checksum was found for {}.", asset.file_name),
            )
        })?;

        let staging = self.bin_dir.join(".staging").join(package.key());
        if staging.exists() {
            std::fs::remove_dir_all(&staging)?;
        }
        std::fs::create_dir_all(&staging)?;
        let download_path = staging.join(format!("{}.download", asset.file_name));

        let url = format!("{base}/{}", asset.file_name);
        let resp = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Network,
                    format!("Could not download {}.", package.display_name()),
                )
                .with_detail(e.to_string())
            })?;
        let total = resp.content_length();
        if total.is_some_and(|t| t > MAX_DOWNLOAD_BYTES) {
            return Err(AppError::new(
                ErrorCode::DependencyFailed,
                "The component download is unexpectedly large.",
            ));
        }
        if let (Some(t), Some(free)) = (total, crate::security::available_space(&self.bin_dir)) {
            if free < t.saturating_mul(3) {
                return Err(AppError::new(
                    ErrorCode::DiskFull,
                    "There is not enough free disk space to install this component.",
                ));
            }
        }
        let mut file = std::fs::File::create(&download_path)?;
        let mut hasher = Sha256::new();
        let mut downloaded = 0u64;
        let mut stream = resp.bytes_stream();
        let mut last_report = std::time::Instant::now();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            downloaded += chunk.len() as u64;
            if downloaded > MAX_DOWNLOAD_BYTES {
                return Err(AppError::new(
                    ErrorCode::DependencyFailed,
                    "The component download is unexpectedly large.",
                ));
            }
            hasher.update(&chunk);
            file.write_all(&chunk)?;
            if last_report.elapsed() >= Duration::from_millis(200) {
                report("downloading", downloaded, total);
                last_report = std::time::Instant::now();
            }
        }
        file.flush()?;
        drop(file);
        report("verifying", downloaded, total);
        let actual = hex::encode(hasher.finalize());
        if actual != expected {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(AppError::new(
                ErrorCode::ChecksumMismatch,
                format!(
                    "{} failed integrity verification and was not installed.",
                    package.display_name()
                ),
            )
            .with_detail(format!("expected {expected}, got {actual}")));
        }

        report("extracting", downloaded, total);
        let out_dir = staging.join("out");
        std::fs::create_dir_all(&out_dir)?;
        let archive = download_path.clone();
        let kind = asset.kind;
        let out = out_dir.clone();
        tokio::task::spawn_blocking(move || extract(package, kind, &archive, &out))
            .await
            .map_err(|e| {
                AppError::internal("Extraction was interrupted.").with_detail(e.to_string())
            })??;

        // Validate the staged executables before replacing anything.
        report("validating", downloaded, total);
        let produced: Vec<DepId> = match package {
            DepId::Ffmpeg => vec![DepId::Ffmpeg, DepId::Ffprobe],
            other => vec![other],
        };
        for id in &produced {
            let staged = out_dir.join(id.exe_name());
            if !staged.is_file() {
                return Err(AppError::new(
                    ErrorCode::DependencyFailed,
                    format!(
                        "The {} package did not contain {}.",
                        package.display_name(),
                        id.exe_name()
                    ),
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
            }
        }
        let version_flag = if package == DepId::Ffmpeg {
            "-version"
        } else {
            "--version"
        };
        let staged_main = out_dir.join(package.exe_name());
        let version_out = process::output(&staged_main, &[version_flag], VERSION_TIMEOUT)
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::DependencyFailed,
                    format!(
                        "The downloaded {} could not be started on this system.",
                        package.display_name()
                    ),
                )
                .with_detail(e.detail.unwrap_or(e.message))
            })?;

        for entry in std::fs::read_dir(&out_dir)? {
            let entry = entry?;
            let target = self.bin_dir.join(entry.file_name());
            crate::security::move_file(&entry.path(), &target, true).map_err(|e| {
                AppError::new(
                    ErrorCode::DependencyFailed,
                    format!(
                        "Could not replace {}. Make sure no download is running and try again.",
                        package.display_name()
                    ),
                )
                .with_detail(e.detail.unwrap_or(e.message))
            })?;
        }
        let _ = std::fs::remove_dir_all(&staging);

        let mut manifest = self.read_manifest();
        manifest.insert(
            package.key().to_string(),
            ManifestEntry {
                version: parse_version(package, &version_out),
                sha256: actual,
                asset: asset.file_name.clone(),
                source: url,
                installed_at: chrono::Utc::now().timestamp_millis(),
            },
        );
        self.write_manifest(&manifest)?;
        report("done", downloaded, total);
        Ok(())
    }

    /// Removes managed copies (used by repair before reinstalling).
    pub fn remove_managed(&self, package: DepId) -> AppResult<()> {
        let ids: Vec<DepId> = match package.package() {
            DepId::Ffmpeg => vec![DepId::Ffmpeg, DepId::Ffprobe],
            other => vec![other],
        };
        for id in ids {
            let path = self.managed_path(id);
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
        Ok(())
    }

    /// A plain-text diagnostics report for troubleshooting.
    pub async fn diagnostics(&self) -> String {
        let statuses = self.refresh().await;
        let manifest = self.read_manifest();
        let mut out = String::new();
        out.push_str(&format!("DarkLoad {}\n", env!("CARGO_PKG_VERSION")));
        out.push_str(&format!(
            "OS: {} ({})\n",
            std::env::consts::OS,
            std::env::consts::ARCH
        ));
        out.push_str(&format!(
            "Managed tools folder: {}\n",
            self.bin_dir.display()
        ));
        out.push_str(&format!(
            "System tools allowed: {}\n\n",
            self.allow_system.load(Ordering::Relaxed)
        ));
        for s in &statuses {
            out.push_str(&format!(
                "{}\n  available: {}\n  source: {:?}\n  version: {}\n  path: {}\n",
                s.name,
                s.available,
                s.source,
                s.version.as_deref().unwrap_or("-"),
                s.path.as_deref().unwrap_or("-")
            ));
            if let Some(err) = &s.error {
                out.push_str(&format!("  error: {err}\n"));
            }
            if let Some(m) = manifest.get(s.id.package().key()) {
                out.push_str(&format!("  sha256: {}\n  asset: {}\n", m.sha256, m.asset));
            }
            out.push('\n');
        }
        out
    }
}

fn find_on_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(exe))
        .find(|candidate| candidate.is_file())
}

fn extract(package: DepId, kind: ArchiveKind, archive: &Path, out_dir: &Path) -> AppResult<()> {
    let write_entry = |name: &str, reader: &mut dyn Read| -> AppResult<()> {
        // Only the bare file name is used, so archive paths can never escape `out_dir`.
        let target = out_dir.join(crate::security::sanitize_filename(name));
        let mut file = std::fs::File::create(&target)?;
        let copied = std::io::copy(&mut Read::take(reader, MAX_DOWNLOAD_BYTES), &mut file)?;
        if copied >= MAX_DOWNLOAD_BYTES {
            return Err(AppError::new(
                ErrorCode::DependencyFailed,
                "An archive entry is unexpectedly large.",
            ));
        }
        Ok(())
    };
    match kind {
        ArchiveKind::Raw => {
            std::fs::copy(archive, out_dir.join(package.exe_name()))?;
        }
        ArchiveKind::Zip => {
            let file = std::fs::File::open(archive)?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| {
                AppError::new(
                    ErrorCode::DependencyFailed,
                    "The downloaded archive is damaged.",
                )
                .with_detail(e.to_string())
            })?;
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i).map_err(|e| {
                    AppError::new(
                        ErrorCode::DependencyFailed,
                        "The downloaded archive is damaged.",
                    )
                    .with_detail(e.to_string())
                })?;
                if !entry.is_file() {
                    continue;
                }
                if let Some(name) = wanted_entry(package, entry.name()) {
                    write_entry(&name, &mut entry)?;
                }
            }
        }
        ArchiveKind::TarXz => {
            let file = std::fs::File::open(archive)?;
            let mut tar = tar::Archive::new(xz2::read::XzDecoder::new(file));
            for entry in tar.entries()? {
                let mut entry = entry?;
                if !entry.header().entry_type().is_file() {
                    continue;
                }
                let path = entry.path()?.to_string_lossy().into_owned();
                if let Some(name) = wanted_entry(package, &path) {
                    write_entry(&name, &mut entry)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_official_assets() {
        assert_eq!(
            asset_for(DepId::YtDlp, "windows", "x86_64")
                .unwrap()
                .file_name,
            "yt-dlp.exe"
        );
        assert_eq!(
            asset_for(DepId::YtDlp, "windows", "aarch64")
                .unwrap()
                .file_name,
            "yt-dlp_arm64.exe"
        );
        assert_eq!(
            asset_for(DepId::YtDlp, "linux", "aarch64")
                .unwrap()
                .file_name,
            "yt-dlp_linux_aarch64"
        );
        let ff = asset_for(DepId::Ffprobe, "linux", "x86_64").unwrap();
        assert_eq!(ff.file_name, "ffmpeg-master-latest-linux64-gpl.tar.xz");
        assert_eq!(ff.kind, ArchiveKind::TarXz);
        assert_eq!(ff.checksum_file, "checksums.sha256");
        let deno = asset_for(DepId::Deno, "windows", "x86_64").unwrap();
        assert_eq!(
            deno.checksum_file,
            "deno-x86_64-pc-windows-msvc.zip.sha256sum"
        );
        assert!(asset_for(DepId::YtDlp, "freebsd", "x86_64").is_none());
        assert!(asset_for(DepId::Ffmpeg, "windows", "x86").is_none());
    }

    #[test]
    fn parses_checksum_documents() {
        let sums = "e8665ed64d6b9172773e812f891fb707a0cb1ee8d9ef4fc1acb00cabee4c5256  yt-dlp\n\
                    622ED48023CAC1DAA52A3BF3A6AC38F03514DE214B5F0FA069389FD0F6D17E5C  yt-dlp.exe\n\
                    249ddb058234276bc449b9cd9492aad9f133bda5923c61a9e39983ae114ef17d *dist/yt-dlp_linux\n";
        assert_eq!(
            find_checksum(sums, "yt-dlp.exe").unwrap(),
            "622ed48023cac1daa52a3bf3a6ac38f03514de214b5f0fa069389fd0f6d17e5c"
        );
        assert_eq!(
            find_checksum(sums, "yt-dlp_linux").unwrap(),
            "249ddb058234276bc449b9cd9492aad9f133bda5923c61a9e39983ae114ef17d"
        );
        assert!(find_checksum(sums, "yt-dlp_macos").is_none());
        let powershell = "\nAlgorithm : SHA256\nHash      : CBA84B26E0DF5AE7F74FC66B86A95B866872A86292EBD84FA1E959927EDD93BF\nPath      : C:\\a\\deno.zip\n";
        assert_eq!(
            find_checksum(powershell, "deno-x86_64-pc-windows-msvc.zip").unwrap(),
            "cba84b26e0df5ae7f74fc66b86a95b866872a86292ebd84fa1e959927edd93bf"
        );
        assert!(find_checksum("garbage", "x").is_none());
    }

    #[test]
    fn parses_versions() {
        assert_eq!(
            parse_version(DepId::YtDlp, "2026.11.23\n").as_deref(),
            Some("2026.11.23")
        );
        assert_eq!(
            parse_version(
                DepId::Ffmpeg,
                "ffmpeg version N-121234-gabc-20261125 Copyright (c) 2000-2026\nbuilt with gcc"
            )
            .as_deref(),
            Some("N-121234-gabc-20261125")
        );
        assert_eq!(
            parse_version(DepId::Ffprobe, "ffprobe version 7.1 Copyright").as_deref(),
            Some("7.1")
        );
        assert_eq!(
            parse_version(DepId::Deno, "deno 2.9.8 (stable, release, x86_64)\nv8 14").as_deref(),
            Some("2.9.8")
        );
        assert!(parse_version(DepId::Ffmpeg, "").is_none());
    }

    #[test]
    fn filters_archive_entries_safely() {
        let exe = std::env::consts::EXE_SUFFIX;
        assert_eq!(
            wanted_entry(DepId::Ffmpeg, &format!("ffmpeg-master/bin/ffmpeg{exe}")),
            Some(format!("ffmpeg{exe}"))
        );
        assert_eq!(
            wanted_entry(DepId::Ffmpeg, &format!("ffmpeg-master/bin/ffprobe{exe}")),
            Some(format!("ffprobe{exe}"))
        );
        assert_eq!(
            wanted_entry(DepId::Ffmpeg, &format!("ffmpeg-master/bin/ffplay{exe}")),
            None
        );
        assert_eq!(
            wanted_entry(DepId::Ffmpeg, &format!("ffmpeg-master/doc/ffmpeg{exe}")),
            None
        );
        assert_eq!(
            wanted_entry(DepId::Ffmpeg, "../../bin/../evil/bin/x.sh"),
            None
        );
        assert_eq!(
            wanted_entry(DepId::Deno, &DepId::Deno.exe_name()),
            Some(DepId::Deno.exe_name())
        );
        if cfg!(windows) {
            assert_eq!(
                wanted_entry(DepId::Ffmpeg, "ffmpeg/bin/avcodec-62.dll"),
                Some("avcodec-62.dll".into())
            );
        }
    }

    #[test]
    fn extracts_zip_without_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("deno.zip");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file(DepId::Deno.exe_name(), options).unwrap();
            zip.write_all(b"binary").unwrap();
            zip.start_file("../escape.txt", options).unwrap();
            zip.write_all(b"nope").unwrap();
            zip.finish().unwrap();
        }
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        extract(DepId::Deno, ArchiveKind::Zip, &archive, &out).unwrap();
        assert_eq!(
            std::fs::read(out.join(DepId::Deno.exe_name())).unwrap(),
            b"binary"
        );
        assert!(!dir.path().join("escape.txt").exists());
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1);
    }

    #[test]
    fn extracts_tar_xz() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("ffmpeg.tar.xz");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let enc = xz2::write::XzEncoder::new(file, 6);
            let mut builder = tar::Builder::new(enc);
            for name in [
                format!("ffmpeg{}", std::env::consts::EXE_SUFFIX),
                format!("ffprobe{}", std::env::consts::EXE_SUFFIX),
                "README.txt".into(),
            ] {
                let data = b"data";
                let mut header = tar::Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(0o755);
                header.set_cksum();
                let path = if name == "README.txt" {
                    name.clone()
                } else {
                    format!("ffmpeg-master-latest/bin/{name}")
                };
                builder.append_data(&mut header, path, &data[..]).unwrap();
            }
            builder.into_inner().unwrap().finish().unwrap();
        }
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        extract(DepId::Ffmpeg, ArchiveKind::TarXz, &archive, &out).unwrap();
        assert!(out.join(DepId::Ffmpeg.exe_name()).is_file());
        assert!(out.join(DepId::Ffprobe.exe_name()).is_file());
        assert!(!out.join("README.txt").exists());
    }

    #[tokio::test]
    async fn missing_tools_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let manager = DependencyManager::new(dir.path().join("bin"), false).unwrap();
        let statuses = manager.refresh().await;
        assert_eq!(statuses.len(), 4);
        assert!(statuses
            .iter()
            .all(|s| !s.available && s.source == DepSource::Missing));
        let err = manager
            .tool_paths(dir.path().join("cache"))
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::DependencyMissing);
    }

    #[tokio::test]
    async fn broken_managed_binary_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        let manager = DependencyManager::new(bin.clone(), false).unwrap();
        std::fs::write(bin.join(DepId::YtDlp.exe_name()), b"not an executable").unwrap();
        let statuses = manager.refresh().await;
        let yt = statuses.iter().find(|s| s.id == DepId::YtDlp).unwrap();
        assert_eq!(yt.source, DepSource::Managed);
        assert!(!yt.available);
        assert!(yt.error.is_some());
    }
}
