//! Input hardening: filename sanitization, output template validation, directory validation,
//! path containment and safe file placement.

use std::path::{Component, Path, PathBuf};

use crate::errors::{AppError, AppResult, ErrorCode};

/// Maximum length, in bytes, of a final file name (most file systems allow 255).
pub const MAX_FILENAME_BYTES: usize = 200;
pub const MAX_TEMPLATE_LENGTH: usize = 240;

const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "COM¹", "COM²", "COM³", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6",
    "LPT7", "LPT8", "LPT9", "LPT¹", "LPT²", "LPT³", "CONIN$", "CONOUT$",
];

fn is_forbidden_char(c: char) -> bool {
    matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control()
}

/// Returns true when `stem` (the part before the first dot) is a reserved Windows device name.
pub fn is_windows_reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").trim_end();
    WINDOWS_RESERVED
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(stem))
}

/// Truncates a string to at most `max_bytes` without splitting a UTF-8 character.
pub fn truncate_utf8(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

/// Produces a file name that is valid on Windows and Linux while preserving Unicode text.
///
/// The extension (if any) is kept intact when the name has to be shortened.
pub fn sanitize_filename(name: &str) -> String {
    let mut cleaned: String = name
        .chars()
        .map(|c| if is_forbidden_char(c) { '_' } else { c })
        .collect();
    // Collapse whitespace runs and trim characters Windows strips silently.
    cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut cleaned = cleaned
        .trim_matches(|c: char| c == ' ' || c == '.')
        .to_string();

    if cleaned.is_empty() || cleaned.chars().all(|c| c == '_') {
        cleaned = "download".to_string();
    }

    if is_windows_reserved(&cleaned) {
        cleaned = format!("_{cleaned}");
    }

    if cleaned.len() > MAX_FILENAME_BYTES {
        let (stem, ext) = split_extension(&cleaned);
        let ext_part = ext.map(|e| format!(".{e}")).unwrap_or_default();
        let budget = MAX_FILENAME_BYTES.saturating_sub(ext_part.len()).max(1);
        let stem = truncate_utf8(stem, budget).trim_end_matches([' ', '.']);
        cleaned = format!("{stem}{ext_part}");
    }
    cleaned
}

/// Splits `name.ext` into `(name, Some(ext))` for short alphanumeric extensions.
pub fn split_extension(name: &str) -> (&str, Option<&str>) {
    match name.rfind('.') {
        Some(idx) if idx > 0 => {
            let ext = &name[idx + 1..];
            if !ext.is_empty() && ext.len() <= 8 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
                (&name[..idx], Some(ext))
            } else {
                (name, None)
            }
        }
        _ => (name, None),
    }
}

/// Validates a yt-dlp output template supplied by the user.
///
/// Templates may only describe a file name: directory separators, parent references, absolute
/// paths and shell-like constructs are rejected. Every template must end with the extension field.
pub fn validate_filename_template(template: &str) -> AppResult<()> {
    let template = template.trim();
    let fail = |msg: &str| Err(AppError::new(ErrorCode::InvalidInput, msg.to_string()));
    if template.is_empty() {
        return fail("The filename template cannot be empty.");
    }
    if template.len() > MAX_TEMPLATE_LENGTH {
        return fail("The filename template is too long.");
    }
    if template.contains('/') || template.contains('\\') {
        return fail("The filename template cannot contain folder separators.");
    }
    if template.contains("..") && !template.contains("%(") {
        return fail("The filename template cannot reference parent folders.");
    }
    if template.chars().any(|c| c.is_control()) {
        return fail("The filename template contains invalid characters.");
    }
    if template
        .chars()
        .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return fail(
            "The filename template contains characters that are not allowed in file names.",
        );
    }
    if !template.ends_with(".%(ext)s") {
        return fail("The filename template must end with .%(ext)s");
    }
    // Every field reference must be a simple, known-safe field expression.
    let mut rest = template;
    while let Some(start) = rest.find("%(") {
        let after = &rest[start + 2..];
        let end = after
            .find(")")
            .ok_or_else(|| AppError::invalid_input("A template field is not closed."))?;
        let field = &after[..end];
        if field.is_empty()
            || !field.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(c, '_' | '.' | ',' | '&' | '|' | '-' | '+' | ' ')
            })
        {
            return fail("The filename template contains an unsupported field.");
        }
        let spec = &after[end + 1..];
        let conversion = spec.chars().find(|c| c.is_ascii_alphabetic());
        if !matches!(conversion, Some('s' | 'd' | 'f' | 'S' | 'U' | 'D' | 'B')) {
            return fail("Template fields must use a supported conversion such as %(title)s.");
        }
        rest = &after[end + 1..];
    }
    Ok(())
}

/// Validates that `dir` is an existing, writable directory and returns its canonical form.
pub fn validate_output_dir(dir: &str) -> AppResult<PathBuf> {
    let trimmed = dir.trim();
    if trimmed.is_empty() {
        return Err(AppError::new(
            ErrorCode::InvalidPath,
            "Choose a download folder.",
        ));
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err(AppError::new(
            ErrorCode::InvalidPath,
            "The download folder must be an absolute path.",
        ));
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(AppError::new(
            ErrorCode::InvalidPath,
            "The download folder path cannot contain '..'.",
        ));
    }
    let meta = std::fs::metadata(&path).map_err(|e| {
        AppError::new(
            ErrorCode::InvalidPath,
            "The download folder does not exist or cannot be accessed.",
        )
        .with_detail(e.to_string())
    })?;
    if !meta.is_dir() {
        return Err(AppError::new(
            ErrorCode::InvalidPath,
            "The selected download location is not a folder.",
        ));
    }
    ensure_writable(&path)?;
    Ok(dunce_canonicalize(&path))
}

/// Confirms a directory accepts new files by creating and removing a probe file.
pub fn ensure_writable(dir: &Path) -> AppResult<()> {
    let probe = dir.join(format!(
        ".darkload-write-test-{}",
        uuid::Uuid::new_v4().simple()
    ));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(e) => Err(AppError::new(
            ErrorCode::PermissionDenied,
            "DarkLoad cannot write to the selected download folder. Choose a different folder.",
        )
        .with_detail(e.to_string())),
    }
}

/// Canonicalizes a path without the `\\?\` verbatim prefix on Windows.
pub fn dunce_canonicalize(path: &Path) -> PathBuf {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    #[cfg(windows)]
    {
        let text = canonical.to_string_lossy();
        if let Some(stripped) = text.strip_prefix(r"\\?\") {
            if !stripped.starts_with("UNC\\") {
                return PathBuf::from(stripped);
            }
            return PathBuf::from(format!(r"\\{}", &stripped[4..]));
        }
    }
    canonical
}

/// Returns true when `candidate` resolves to a location inside `root`.
pub fn is_within(root: &Path, candidate: &Path) -> bool {
    let root = dunce_canonicalize(root);
    let candidate = dunce_canonicalize(candidate);
    candidate.starts_with(&root)
}

/// How to resolve a name collision in the output folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DuplicatePolicy {
    Rename,
    Overwrite,
    Skip,
}

/// Result of placing a finished file into the output folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    Placed(PathBuf),
    SkippedExisting(PathBuf),
}

/// Chooses the destination path for `file_name` inside `dir` according to `policy`.
pub fn resolve_destination(dir: &Path, file_name: &str, policy: DuplicatePolicy) -> Placement {
    let safe = sanitize_filename(file_name);
    let target = dir.join(&safe);
    if !target.exists() {
        return Placement::Placed(target);
    }
    match policy {
        DuplicatePolicy::Overwrite => Placement::Placed(target),
        DuplicatePolicy::Skip => Placement::SkippedExisting(target),
        DuplicatePolicy::Rename => {
            let (stem, ext) = split_extension(&safe);
            let ext_part = ext.map(|e| format!(".{e}")).unwrap_or_default();
            for n in 1..10_000 {
                let suffix = format!(" ({n})");
                let budget = MAX_FILENAME_BYTES.saturating_sub(ext_part.len() + suffix.len());
                let candidate =
                    dir.join(format!("{}{suffix}{ext_part}", truncate_utf8(stem, budget)));
                if !candidate.exists() {
                    return Placement::Placed(candidate);
                }
            }
            let unique = uuid::Uuid::new_v4().simple().to_string();
            Placement::Placed(dir.join(format!(
                "{}-{}{ext_part}",
                truncate_utf8(stem, 120),
                &unique[..8]
            )))
        }
    }
}

/// Moves a file, falling back to copy + delete when crossing file systems.
pub fn move_file(from: &Path, to: &Path, overwrite: bool) -> AppResult<()> {
    if to.exists() {
        if !overwrite {
            return Err(AppError::new(
                ErrorCode::Io,
                "The destination file already exists.",
            ));
        }
        std::fs::remove_file(to)?;
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(from, to)?;
            std::fs::remove_file(from)?;
            Ok(())
        }
    }
}

/// Free space, in bytes, on the volume containing `path`.
pub fn available_space(path: &Path) -> Option<u64> {
    fs4::available_space(path).ok()
}

/// Validates a custom yt-dlp format expression (for example `137+140` or `bv*[height<=1080]+ba`).
pub fn validate_format_expression(expr: &str) -> AppResult<()> {
    let expr = expr.trim();
    if expr.is_empty() || expr.len() > 200 {
        return Err(AppError::invalid_input(
            "Enter a custom format expression of up to 200 characters.",
        ));
    }
    if expr.starts_with('-') {
        return Err(AppError::invalid_input(
            "A format expression cannot start with '-'.",
        ));
    }
    let allowed = |c: char| c.is_ascii_alphanumeric() || "+-/*[]=<>!^$~.,:_?()' ".contains(c);
    if !expr.chars().all(allowed) {
        return Err(AppError::invalid_input(
            "The custom format expression contains unsupported characters.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_forbidden_characters() {
        assert_eq!(
            sanitize_filename(r#"a<b>c:d"e/f\g|h?i*j.mp4"#),
            "a_b_c_d_e_f_g_h_i_j.mp4"
        );
        assert_eq!(
            sanitize_filename("line\nbreak\ttab.mp3"),
            "line_break_tab.mp3"
        );
    }

    #[test]
    fn trims_dots_and_spaces() {
        assert_eq!(sanitize_filename("  name.mp4. . "), "name.mp4");
        assert_eq!(sanitize_filename("..."), "download");
        assert_eq!(sanitize_filename(""), "download");
        assert_eq!(sanitize_filename("///"), "download");
    }

    #[test]
    fn handles_reserved_windows_names() {
        assert_eq!(sanitize_filename("CON.mp4"), "_CON.mp4");
        assert_eq!(sanitize_filename("nul"), "_nul");
        assert_eq!(sanitize_filename("Lpt1.txt"), "_Lpt1.txt");
        assert_eq!(sanitize_filename("console.mp4"), "console.mp4");
        assert!(is_windows_reserved("COM3.tar.gz"));
        assert!(!is_windows_reserved("COMPANY.mp4"));
    }

    #[test]
    fn preserves_unicode() {
        assert_eq!(
            sanitize_filename("日本語のタイトル 🎵.m4a"),
            "日本語のタイトル 🎵.m4a"
        );
        assert_eq!(
            sanitize_filename("Ünïcödé — Ελληνικά.mkv"),
            "Ünïcödé — Ελληνικά.mkv"
        );
    }

    #[test]
    fn truncates_long_names_keeping_extension() {
        let long = format!("{}.webm", "あ".repeat(200));
        let out = sanitize_filename(&long);
        assert!(out.len() <= MAX_FILENAME_BYTES);
        assert!(out.ends_with(".webm"));
        assert!(out.starts_with('あ'));
    }

    #[test]
    fn splits_extensions() {
        assert_eq!(split_extension("video.mp4"), ("video", Some("mp4")));
        assert_eq!(split_extension(".hidden"), (".hidden", None));
        assert_eq!(split_extension("no_ext"), ("no_ext", None));
        assert_eq!(
            split_extension("weird.ext with space"),
            ("weird.ext with space", None)
        );
    }

    #[test]
    fn validates_templates() {
        assert!(validate_filename_template("%(title)s [%(id)s].%(ext)s").is_ok());
        assert!(validate_filename_template("%(uploader)s - %(title).150B.%(ext)s").is_ok());
        assert!(validate_filename_template("%(upload_date>%Y-%m-%d)s %(title)s.%(ext)s").is_err());
        assert!(validate_filename_template("../%(title)s.%(ext)s").is_err());
        assert!(validate_filename_template("/etc/%(title)s.%(ext)s").is_err());
        assert!(validate_filename_template(r"C:\x\%(title)s.%(ext)s").is_err());
        assert!(validate_filename_template("%(title)s").is_err());
        assert!(validate_filename_template("").is_err());
        assert!(validate_filename_template("%(title)s|calc.%(ext)s").is_err());
        assert!(validate_filename_template("%(title)x.%(ext)s").is_err());
    }

    #[test]
    fn validates_format_expressions() {
        assert!(validate_format_expression("137+140").is_ok());
        assert!(validate_format_expression("bv*[height<=1080][vcodec^=avc1]+ba/b").is_ok());
        assert!(validate_format_expression("--exec calc").is_err());
        assert!(validate_format_expression("137;rm -rf").is_err());
        assert!(validate_format_expression("").is_err());
    }

    #[test]
    fn validates_output_directories() {
        let dir = tempfile::tempdir().unwrap();
        assert!(validate_output_dir(dir.path().to_str().unwrap()).is_ok());
        assert_eq!(
            validate_output_dir("").unwrap_err().code,
            ErrorCode::InvalidPath
        );
        assert_eq!(
            validate_output_dir("relative/path").unwrap_err().code,
            ErrorCode::InvalidPath
        );
        let missing = dir.path().join("missing");
        assert_eq!(
            validate_output_dir(missing.to_str().unwrap())
                .unwrap_err()
                .code,
            ErrorCode::InvalidPath
        );
        let file = dir.path().join("file.txt");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(
            validate_output_dir(file.to_str().unwrap())
                .unwrap_err()
                .code,
            ErrorCode::InvalidPath
        );
        let traversal = format!("{}{}..", dir.path().display(), std::path::MAIN_SEPARATOR);
        assert_eq!(
            validate_output_dir(&traversal).unwrap_err().code,
            ErrorCode::InvalidPath
        );
    }

    #[test]
    fn supports_unicode_directories() {
        let dir = tempfile::tempdir().unwrap();
        let unicode = dir.path().join("Видео ダウンロード 🎬");
        std::fs::create_dir(&unicode).unwrap();
        let validated = validate_output_dir(unicode.to_str().unwrap()).unwrap();
        assert!(validated.ends_with("Видео ダウンロード 🎬"));
    }

    #[test]
    fn resolves_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let first = resolve_destination(dir.path(), "clip.mp4", DuplicatePolicy::Rename);
        assert_eq!(first, Placement::Placed(dir.path().join("clip.mp4")));
        std::fs::write(dir.path().join("clip.mp4"), b"1").unwrap();
        assert_eq!(
            resolve_destination(dir.path(), "clip.mp4", DuplicatePolicy::Rename),
            Placement::Placed(dir.path().join("clip (1).mp4"))
        );
        std::fs::write(dir.path().join("clip (1).mp4"), b"2").unwrap();
        assert_eq!(
            resolve_destination(dir.path(), "clip.mp4", DuplicatePolicy::Rename),
            Placement::Placed(dir.path().join("clip (2).mp4"))
        );
        assert_eq!(
            resolve_destination(dir.path(), "clip.mp4", DuplicatePolicy::Skip),
            Placement::SkippedExisting(dir.path().join("clip.mp4"))
        );
        assert_eq!(
            resolve_destination(dir.path(), "clip.mp4", DuplicatePolicy::Overwrite),
            Placement::Placed(dir.path().join("clip.mp4"))
        );
    }

    #[test]
    fn moves_files_without_silent_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        assert!(move_file(&a, &b, false).is_err());
        move_file(&a, &b, true).unwrap();
        assert_eq!(std::fs::read(&b).unwrap(), b"a");
        assert!(!a.exists());
    }

    #[test]
    fn containment_checks() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("inner");
        std::fs::create_dir(&inner).unwrap();
        assert!(is_within(dir.path(), &inner));
        assert!(!is_within(&inner, dir.path()));
    }
}
