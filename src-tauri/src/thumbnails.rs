//! Local thumbnail cache.
//!
//! Remote thumbnails are fetched once by the backend and served to the interface from the local
//! cache through Tauri's asset protocol. This avoids hot-linking third-party CDNs from the UI and
//! keeps the Content Security Policy strict.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};

const MAX_THUMBNAIL_BYTES: usize = 8 * 1024 * 1024;

fn cache_key(url: &str) -> String {
    let digest = Sha256::digest(url.as_bytes());
    hex::encode(&digest[..16])
}

fn extension_for(content_type: &str) -> Option<&'static str> {
    let ct = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match ct.as_str() {
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        "image/avif" => Some("avif"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

/// Returns a cached thumbnail path for `url`, downloading it when necessary.
pub async fn cache(http: &reqwest::Client, dir: &Path, url: &str) -> Option<PathBuf> {
    if !url.starts_with("https://") || url.len() > 4096 {
        return None;
    }
    let key = cache_key(url);
    for ext in ["jpg", "png", "webp", "avif", "gif"] {
        let candidate = dir.join(format!("{key}.{ext}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let response = tokio::time::timeout(Duration::from_secs(15), http.get(url).send())
        .await
        .ok()?
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let ext = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(extension_for)?;
    if response
        .content_length()
        .is_some_and(|l| l as usize > MAX_THUMBNAIL_BYTES)
    {
        return None;
    }
    let mut data = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = tokio::time::timeout(Duration::from_secs(15), stream.next())
        .await
        .ok()?
    {
        let chunk = chunk.ok()?;
        if data.len() + chunk.len() > MAX_THUMBNAIL_BYTES {
            return None;
        }
        data.extend_from_slice(&chunk);
    }
    std::fs::create_dir_all(dir).ok()?;
    let target = dir.join(format!("{key}.{ext}"));
    let tmp = dir.join(format!("{key}.{ext}.tmp"));
    std::fs::write(&tmp, &data).ok()?;
    std::fs::rename(&tmp, &target).ok()?;
    Some(target)
}

/// Returns the folder where files created in `dir` are physically stored, when it differs from
/// where `dir` itself resolves.
///
/// Windows can redirect new files written under `%LOCALAPPDATA%` to another folder. This happens
/// with MSIX file-system virtualization, for example when DarkLoad is started by a packaged app:
/// the files land in `%LOCALAPPDATA%\Packages\<package>\LocalCache\Local\...` while the folder
/// still appears at its usual path. The asset protocol canonicalizes every requested file before
/// checking its scope, so thumbnails stored in the redirected folder are rejected unless that
/// folder is allowed as well.
pub fn redirected_dir(dir: &Path) -> Option<PathBuf> {
    let probe = dir.join(format!(".location-probe-{}", std::process::id()));
    std::fs::write(&probe, b"").ok()?;
    let stored = std::fs::canonicalize(&probe)
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let _ = std::fs::remove_file(&probe);
    let stored = stored?;
    let resolved = std::fs::canonicalize(dir).ok()?;
    (stored != resolved).then_some(stored)
}

/// Deletes every cached thumbnail.
pub fn clear(dir: &Path) -> std::io::Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::create_dir_all(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_stable_and_safe() {
        let a = cache_key("https://i.ytimg.com/vi/x/hqdefault.jpg");
        assert_eq!(a, cache_key("https://i.ytimg.com/vi/x/hqdefault.jpg"));
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn only_images_are_accepted() {
        assert_eq!(extension_for("image/jpeg"), Some("jpg"));
        assert_eq!(extension_for("image/webp; charset=binary"), Some("webp"));
        assert_eq!(extension_for("text/html"), None);
        assert_eq!(
            extension_for("image/svg+xml"),
            None,
            "SVG can carry scripts and is rejected"
        );
    }

    #[tokio::test]
    async fn rejects_non_https_urls() {
        let client = reqwest::Client::new();
        let dir = tempfile::tempdir().unwrap();
        assert!(cache(&client, dir.path(), "http://example.com/a.jpg")
            .await
            .is_none());
        assert!(cache(&client, dir.path(), "file:///etc/passwd")
            .await
            .is_none());
    }

    #[test]
    fn plain_folders_are_not_redirected_and_the_probe_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(redirected_dir(dir.path()), None);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn missing_folders_are_not_redirected() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(redirected_dir(&dir.path().join("missing")), None);
    }
}
