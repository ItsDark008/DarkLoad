//! Structured error types shared by every backend module.
//!
//! Errors crossing the IPC boundary are serialized as `{ code, message, detail, retryable }`
//! so the frontend can render precise, actionable messages without parsing strings.

use serde::{Deserialize, Serialize, Serializer};

/// Machine-readable error categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidUrl,
    UnsupportedUrl,
    Unavailable,
    PrivateContent,
    LoginRequired,
    AgeRestricted,
    GeoRestricted,
    DrmProtected,
    LiveNotFinished,
    FormatUnavailable,
    RateLimited,
    Network,
    DiskFull,
    PermissionDenied,
    InvalidPath,
    InvalidInput,
    DependencyMissing,
    DependencyFailed,
    ChecksumMismatch,
    ProcessFailed,
    Cancelled,
    Database,
    Io,
    UpdateUnavailable,
    Internal,
}

impl ErrorCode {
    /// Whether an automatic retry has a realistic chance of succeeding.
    pub fn is_transient(self) -> bool {
        matches!(self, ErrorCode::Network | ErrorCode::RateLimited)
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub detail: Option<String>,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        if !detail.trim().is_empty() {
            self.detail = Some(truncate_detail(&detail));
        }
        self
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidInput, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }

    pub fn cancelled() -> Self {
        Self::new(ErrorCode::Cancelled, "The operation was cancelled.")
    }

    pub fn retryable(&self) -> bool {
        self.code.is_transient()
    }
}

/// Keeps error details bounded so a misbehaving child process cannot flood the UI or database.
fn truncate_detail(detail: &str) -> String {
    const MAX: usize = 4000;
    if detail.len() <= MAX {
        return detail.to_string();
    }
    let mut end = MAX;
    while !detail.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &detail[..end])
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 4)?;
        s.serialize_field("code", &self.code)?;
        s.serialize_field("message", &self.message)?;
        s.serialize_field("detail", &self.detail)?;
        s.serialize_field("retryable", &self.retryable())?;
        s.end()
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        use std::io::ErrorKind;
        let code = match err.kind() {
            ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            ErrorKind::NotFound => ErrorCode::InvalidPath,
            ErrorKind::StorageFull => ErrorCode::DiskFull,
            _ if is_disk_full(&err) => ErrorCode::DiskFull,
            _ => ErrorCode::Io,
        };
        let message = match code {
            ErrorCode::PermissionDenied => "Permission denied while accessing a file or folder.",
            ErrorCode::InvalidPath => "A required file or folder does not exist.",
            ErrorCode::DiskFull => "There is not enough free disk space.",
            _ => "A file system operation failed.",
        };
        AppError::new(code, message).with_detail(err.to_string())
    }
}

fn is_disk_full(err: &std::io::Error) -> bool {
    #[cfg(windows)]
    {
        // ERROR_DISK_FULL / ERROR_HANDLE_DISK_FULL.
        matches!(err.raw_os_error(), Some(112) | Some(39))
    }
    #[cfg(not(windows))]
    {
        // ENOSPC.
        err.raw_os_error() == Some(28)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        AppError::new(ErrorCode::Database, "A local database operation failed.")
            .with_detail(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::internal("Failed to process structured data.").with_detail(err.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        AppError::new(
            ErrorCode::Network,
            "A network request failed. Check your connection and try again.",
        )
        .with_detail(err.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(err: tauri::Error) -> Self {
        AppError::internal("A desktop runtime operation failed.").with_detail(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_retryable_flag() {
        let err = AppError::new(ErrorCode::Network, "offline").with_detail("timeout");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "network");
        assert_eq!(json["message"], "offline");
        assert_eq!(json["detail"], "timeout");
        assert_eq!(json["retryable"], true);
    }

    #[test]
    fn non_transient_errors_are_not_retryable() {
        assert!(!AppError::new(ErrorCode::PrivateContent, "x").retryable());
        assert!(!AppError::new(ErrorCode::DiskFull, "x").retryable());
    }

    #[test]
    fn io_errors_map_to_codes() {
        let err: AppError =
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied").into();
        assert_eq!(err.code, ErrorCode::PermissionDenied);
        let raw = if cfg!(windows) { 112 } else { 28 };
        let err: AppError = std::io::Error::from_raw_os_error(raw).into();
        assert_eq!(err.code, ErrorCode::DiskFull);
    }

    #[test]
    fn details_are_truncated_on_char_boundaries() {
        let long = "é".repeat(5000);
        let err = AppError::internal("x").with_detail(long);
        let detail = err.detail.unwrap();
        assert!(detail.len() <= 4004);
        assert!(detail.ends_with('…'));
    }

    #[test]
    fn empty_detail_is_ignored() {
        assert!(AppError::internal("x").with_detail("   ").detail.is_none());
    }
}
