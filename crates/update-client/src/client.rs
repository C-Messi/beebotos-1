//! Update client implementation for native platforms

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use crate::config::UpdateConfig;
use crate::error::UpdateError;
use crate::models::{
    PackageInfo, UpdateCheckRequest, UpdateCheckResponse, UpdateReportRequest, UpdateState,
    UpdateStatus, VersionInfo,
};

/// Download progress callback
#[async_trait]
pub trait DownloadProgress: Send + Sync {
    fn on_progress(&self, downloaded: u64, total: u64);
    fn on_complete(&self, path: &Path);
    fn on_error(&self, error: &UpdateError);
}

/// Update client trait
#[async_trait]
pub trait UpdateClient: Send + Sync {
    /// Check for available updates
    async fn check_update(&self) -> Result<Option<VersionInfo>, UpdateError>;

    /// Download update package
    async fn download(
        &self,
        package: &PackageInfo,
        progress: &dyn DownloadProgress,
    ) -> Result<PathBuf, UpdateError>;

    /// Verify package integrity and signature
    async fn verify(&self, package_path: &Path, package: &PackageInfo)
        -> Result<bool, UpdateError>;

    /// Install the update
    async fn install(&self, package_path: &Path, info: &VersionInfo) -> Result<(), UpdateError>;

    /// Rollback to previous version
    async fn rollback(&self) -> Result<(), UpdateError>;

    /// Get current update state
    fn state(&self) -> UpdateState;

    /// Get configuration reference
    fn config(&self) -> &UpdateConfig;
}

/// Native update client implementation
pub struct NativeUpdateClient {
    config: UpdateConfig,
    http_client: reqwest::Client,
    state: std::sync::Arc<tokio::sync::RwLock<UpdateState>>,
    temp_dir: PathBuf,
}

impl NativeUpdateClient {
    pub fn new(config: UpdateConfig) -> Result<Self, UpdateError> {
        let mut client_builder =
            reqwest::Client::builder().timeout(std::time::Duration::from_secs(300));

        if let Some(proxy) = &config.http_proxy {
            if let Ok(proxy_url) = reqwest::Proxy::all(proxy) {
                client_builder = client_builder.proxy(proxy_url);
            }
        }

        let http_client = client_builder
            .build()
            .map_err(|e| UpdateError::Network(e.to_string()))?;

        let temp_dir = std::env::temp_dir().join("beebotos").join("updates");

        let current_version: semver::Version = config
            .current_version
            .parse()
            .map_err(|e: semver::Error| UpdateError::Verification(e.to_string()))?;

        Ok(Self {
            config,
            http_client,
            state: std::sync::Arc::new(tokio::sync::RwLock::new(UpdateState {
                status: UpdateStatus::Idle,
                current_version: current_version.into(),
                target_version: None,
                download_progress: 0,
                error: None,
                retry_count: 0,
            })),
            temp_dir,
        })
    }

    async fn set_state(&self, status: UpdateStatus) {
        let mut state = self.state.write().await;
        state.status = status;
    }

    #[allow(dead_code)]
    async fn set_error(&self, error: Option<String>) {
        let mut state = self.state.write().await;
        state.error = error;
    }

    async fn set_progress(&self, progress: u8) {
        let mut state = self.state.write().await;
        state.download_progress = progress;
    }

    async fn set_target(&self, version: Option<crate::models::SemVer>) {
        let mut state = self.state.write().await;
        state.target_version = version;
    }

    /// Report update status to server
    pub async fn report_status(
        &self,
        target_version: &str,
        status: UpdateStatus,
        duration_secs: u64,
        error: Option<String>,
    ) -> Result<(), UpdateError> {
        let report = UpdateReportRequest {
            app_name: self.config.app_name.clone(),
            device_id: self.config.device_id.clone(),
            current_version: self.config.current_version.clone(),
            target_version: target_version.to_string(),
            status,
            duration_secs,
            error,
        };

        let url = format!("{}/api/v1/updates/report", self.config.server_url);
        let _resp = self.http_client.post(&url).json(&report).send().await?;

        Ok(())
    }
}

#[async_trait]
impl UpdateClient for NativeUpdateClient {
    async fn check_update(&self) -> Result<Option<VersionInfo>, UpdateError> {
        self.set_state(UpdateStatus::Checking).await;

        let req = UpdateCheckRequest {
            app_name: self.config.app_name.clone(),
            current_version: self.config.current_version.clone(),
            platform: self.config.platform.clone(),
            channel: self.config.channel.clone(),
        };

        let url = format!("{}/api/v1/updates/check", self.config.server_url);
        let resp = self.http_client.post(&url).json(&req).send().await?;

        if !resp.status().is_success() {
            return Err(UpdateError::Network(format!("HTTP {}", resp.status())));
        }

        let check_resp: UpdateCheckResponse = resp.json().await?;

        if let Some(info) = &check_resp.version_info {
            // Downgrade protection: check min_supported_version
            if !self.config.allow_downgrade {
                if let Some(min_ver) = &info.min_supported_version {
                    if let (Ok(current), Ok(min)) = (
                        semver::Version::parse(&self.config.current_version),
                        semver::Version::parse(&min_ver.to_string()),
                    ) {
                        if current < min {
                            return Err(UpdateError::VersionNotSupported);
                        }
                    }
                }
            }

            self.set_target(Some(info.version.clone())).await;
            self.set_state(UpdateStatus::Idle).await;
            Ok(Some(info.clone()))
        } else {
            self.set_target(None).await;
            self.set_state(UpdateStatus::Idle).await;
            Ok(None)
        }
    }

    async fn download(
        &self,
        package: &PackageInfo,
        progress: &dyn DownloadProgress,
    ) -> Result<PathBuf, UpdateError> {
        self.set_state(UpdateStatus::Downloading).await;
        self.set_progress(0).await;

        // Sanitize package.id to prevent directory traversal
        let safe_id = package
            .id
            .replace('/', "_")
            .replace('\\', "_")
            .replace("..", "_");
        let temp_path = self.temp_dir.join(&safe_id);
        tokio::fs::create_dir_all(&self.temp_dir).await?;

        let url = if package.download_url.starts_with("http") {
            package.download_url.clone()
        } else {
            format!("{}{}", self.config.server_url, package.download_url)
        };

        // Check for partial download (resume)
        let mut start_byte = if temp_path.exists() {
            let meta = tokio::fs::metadata(&temp_path).await?;
            meta.len()
        } else {
            0
        };

        let mut request = self.http_client.get(&url);
        let has_resume = start_byte > 0 && start_byte < package.size;
        if has_resume {
            request = request.header("Range", format!("bytes={}-", start_byte));
        }

        let mut resp = request.send().await?;
        let status = resp.status();
        if status == reqwest::StatusCode::PARTIAL_CONTENT && has_resume {
            // Server supports resume, continue appending
        } else if status.is_success() && !has_resume {
            // Full download, start from beginning
        } else if status.is_success() && has_resume {
            // Server ignored Range, restart from beginning
            start_byte = 0;
        } else {
            return Err(UpdateError::Download(format!("HTTP {}", status)));
        }

        let mut file = if start_byte > 0 {
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(&temp_path)
                .await?
        } else {
            // Truncate existing file if restarting
            tokio::fs::File::create(&temp_path).await?
        };

        let mut downloaded = start_byte;
        while let Some(chunk) = resp.chunk().await? {
            use tokio::io::AsyncWriteExt;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            let pct = ((downloaded as f64 / package.size as f64) * 100.0) as u8;
            self.set_progress(pct.min(100)).await;
            progress.on_progress(downloaded, package.size);
        }

        file.flush().await?;
        file.sync_all().await?;
        drop(file);

        // Verify downloaded file size
        let final_size = tokio::fs::metadata(&temp_path).await?.len();
        if final_size != package.size {
            return Err(UpdateError::Download(format!(
                "Size mismatch: expected {} bytes, got {}",
                package.size, final_size
            )));
        }

        self.set_progress(100).await;
        progress.on_complete(&temp_path);
        Ok(temp_path)
    }

    async fn verify(
        &self,
        package_path: &Path,
        package: &PackageInfo,
    ) -> Result<bool, UpdateError> {
        self.set_state(UpdateStatus::Verifying).await;

        // SHA-256 hash verification
        let file_hash = crate::verify::sha256_file(package_path).await?;
        let expected_hash = hex::decode(&package.hash)
            .map_err(|e| UpdateError::Verification(format!("Invalid hash: {}", e)))?;

        if file_hash != expected_hash {
            return Err(UpdateError::Verification(
                "SHA-256 hash mismatch".to_string(),
            ));
        }

        // Ed25519 signature verification (if public key configured)
        if let Some(pub_key_b64) = &self.config.public_key_b64 {
            let verifier = crate::verify::SignatureVerifier::with_public_key_b64(pub_key_b64)?;
            if !verifier.verify_bytes(&file_hash, &package.signature)? {
                return Err(UpdateError::Verification(
                    "Signature verification failed".to_string(),
                ));
            }
        }

        Ok(true)
    }

    async fn install(&self, _package_path: &Path, _info: &VersionInfo) -> Result<(), UpdateError> {
        self.set_state(UpdateStatus::Installing).await;
        // Platform-specific installation is handled by the application
        // This is a placeholder - gateway/cli should implement their own install logic
        Ok(())
    }

    async fn rollback(&self) -> Result<(), UpdateError> {
        self.set_state(UpdateStatus::RolledBack).await;
        // Platform-specific rollback is handled by the application
        Ok(())
    }

    fn state(&self) -> UpdateState {
        // Blocking read is acceptable here since it's just returning a snapshot
        if let Ok(state) = self.state.try_read() {
            state.clone()
        } else {
            UpdateState::default()
        }
    }

    fn config(&self) -> &UpdateConfig {
        &self.config
    }
}

/// Simple console progress handler
pub struct ConsoleProgress;

#[async_trait]
impl DownloadProgress for ConsoleProgress {
    fn on_progress(&self, downloaded: u64, total: u64) {
        let pct = if total > 0 {
            (downloaded as f64 / total as f64 * 100.0) as u8
        } else {
            0
        };
        tracing::info!(
            "Download progress: {}% ({}/{} bytes)",
            pct,
            downloaded,
            total
        );
    }

    fn on_complete(&self, path: &Path) {
        tracing::info!("Download complete: {}", path.display());
    }

    fn on_error(&self, error: &UpdateError) {
        tracing::error!("Download error: {}", error);
    }
}
