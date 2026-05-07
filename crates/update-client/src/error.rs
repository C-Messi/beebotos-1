//! Error types for update operations

/// Update error enum
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Verification failed: {0}")]
    Verification(String),
    #[error("Installation failed: {0}")]
    Installation(String),
    #[error("Rollback failed: {0}")]
    Rollback(String),
    #[error("Version not supported")]
    VersionNotSupported,
    #[error("Insufficient space")]
    InsufficientSpace,
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Timeout")]
    Timeout,
    #[error("Package not found: {0}")]
    PackageNotFound(String),
    #[error("Version not found: {0}")]
    VersionNotFound(String),
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Download failed: {0}")]
    Download(String),
    #[error("No suitable package found for platform")]
    NoSuitablePackage,
    #[error("Update cancelled by user")]
    Cancelled,
}

impl From<serde_json::Error> for UpdateError {
    fn from(e: serde_json::Error) -> Self {
        UpdateError::Serialization(e.to_string())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<reqwest::Error> for UpdateError {
    fn from(e: reqwest::Error) -> Self {
        UpdateError::Network(e.to_string())
    }
}

impl From<std::io::Error> for UpdateError {
    fn from(e: std::io::Error) -> Self {
        UpdateError::Storage(e.to_string())
    }
}
