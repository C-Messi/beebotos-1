//! Media Downloader
//!
//! Handles downloading media files (images, files, voice, video) from
//! messaging platforms like Lark/Feishu.
//!
//! 🟡 P1 FIX: Added singleton pattern for shared MediaDownloader instance
//! to prevent connection resource waste.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::{RwLock, Semaphore};
use tracing::{debug, info, warn};

use crate::communication::PlatformType;
use crate::error::{AgentError, Result};

/// Media types supported for download
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaType {
    /// Image files (jpg, png, gif, etc.)
    Image,
    /// Generic files (documents, archives, etc.)
    File,
    /// Voice/audio messages
    Voice,
    /// Video files
    Video,
    /// Sticker/images
    Sticker,
}

impl MediaType {
    /// Get file extension for media type
    pub fn default_extension(&self) -> &'static str {
        match self {
            MediaType::Image => "jpg",
            MediaType::File => "bin",
            MediaType::Voice => "mp3",
            MediaType::Video => "mp4",
            MediaType::Sticker => "webp",
        }
    }

    /// Get MIME type for media type
    pub fn default_mime_type(&self) -> &'static str {
        match self {
            MediaType::Image => "image/jpeg",
            MediaType::File => "application/octet-stream",
            MediaType::Voice => "audio/mpeg",
            MediaType::Video => "video/mp4",
            MediaType::Sticker => "image/webp",
        }
    }

    /// Detect media type from MIME type
    pub fn from_mime_type(mime_type: &str) -> Self {
        if mime_type.starts_with("image/") {
            if mime_type.contains("webp") {
                MediaType::Sticker
            } else {
                MediaType::Image
            }
        } else if mime_type.starts_with("audio/") || mime_type.starts_with("voice/") {
            MediaType::Voice
        } else if mime_type.starts_with("video/") {
            MediaType::Video
        } else {
            MediaType::File
        }
    }
}

impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaType::Image => write!(f, "image"),
            MediaType::File => write!(f, "file"),
            MediaType::Voice => write!(f, "voice"),
            MediaType::Video => write!(f, "video"),
            MediaType::Sticker => write!(f, "sticker"),
        }
    }
}

/// Download configuration with connection pooling and memory limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadConfig {
    /// Maximum file size in bytes (default: 100MB)
    pub max_file_size: usize,
    /// Download timeout in seconds (default: 60)
    pub timeout_secs: u64,
    /// Base directory for downloads
    pub download_dir: PathBuf,
    /// Whether to preserve original filenames
    pub preserve_filenames: bool,
    /// Whether to create subdirectories by platform
    pub organize_by_platform: bool,
    /// Whether to create subdirectories by date
    pub organize_by_date: bool,
    /// Custom headers for HTTP requests
    pub custom_headers: HashMap<String, String>,
    /// Connection pool max idle connections per host (default: 10)
    pub pool_max_idle_per_host: usize,
    /// Connection pool idle timeout in seconds (default: 90)
    pub pool_idle_timeout_secs: u64,
    /// Maximum memory usage for downloads in MB (default: 512MB)
    /// Limits concurrent downloads to prevent OOM
    pub max_memory_mb: usize,
    /// Maximum concurrent downloads (default: 10)
    pub max_concurrent_downloads: usize,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            max_file_size: 100 * 1024 * 1024, // 100MB
            timeout_secs: 60,
            download_dir: PathBuf::from("./downloads"),
            preserve_filenames: true,
            organize_by_platform: true,
            organize_by_date: true,
            custom_headers: HashMap::new(),
            pool_max_idle_per_host: 10,   // Connection pooling
            pool_idle_timeout_secs: 90,   // 90s idle timeout
            max_memory_mb: 512,           // 512MB memory limit
            max_concurrent_downloads: 10, // Max 10 concurrent downloads
        }
    }
}

/// Media download result
#[derive(Debug, Clone)]
pub struct MediaDownloadResult {
    /// Local file path
    pub file_path: PathBuf,
    /// Original URL or file key
    pub source: String,
    /// Media type
    pub media_type: MediaType,
    /// File size in bytes
    pub file_size: usize,
    /// MIME type
    pub mime_type: String,
    /// File name
    pub file_name: String,
    /// Download timestamp
    pub downloaded_at: chrono::DateTime<chrono::Utc>,
    /// Platform source
    pub platform: PlatformType,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Media file information
#[derive(Debug, Clone)]
pub struct MediaInfo {
    /// File key or ID (platform-specific)
    pub file_key: String,
    /// Media type
    pub media_type: MediaType,
    /// Original file name (if available)
    pub file_name: Option<String>,
    /// File size (if known)
    pub file_size: Option<usize>,
    /// MIME type (if known)
    pub mime_type: Option<String>,
    /// Platform-specific metadata
    pub metadata: HashMap<String, String>,
}

/// Lark-specific media downloader configuration
#[derive(Debug, Clone)]
pub struct LarkMediaConfig {
    /// App ID
    pub app_id: String,
    /// App secret
    pub app_secret: String,
    /// Access token (will be refreshed if expired)
    pub access_token: Option<String>,
    /// API base URL
    pub api_base: String,
}

impl LarkMediaConfig {
    /// Create new Lark media config
    pub fn new(app_id: String, app_secret: String) -> Self {
        Self {
            app_id,
            app_secret,
            access_token: None,
            api_base: "https://open.larksuite.com/open-apis".to_string(),
        }
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self> {
        let app_id = std::env::var("LARK_APP_ID")
            .map_err(|_| AgentError::configuration("LARK_APP_ID not set"))?;
        let app_secret = std::env::var("LARK_APP_SECRET")
            .map_err(|_| AgentError::configuration("LARK_APP_SECRET not set"))?;
        Ok(Self::new(app_id, app_secret))
    }
}

/// 🟡 P1 FIX: Global singleton instance holder for MediaDownloader
///
/// Uses std::sync::OnceLock for thread-safe lazy initialization
use std::sync::OnceLock;

/// 🟡 P1 FIX: Global singleton instance for shared MediaDownloader
pub static GLOBAL_DOWNLOADER: OnceLock<Arc<MediaDownloader>> = OnceLock::new();

/// Media downloader with connection pooling, memory limits and optional caching
pub struct MediaDownloader {
    config: DownloadConfig,
    http_client: reqwest::Client,
    lark_config: Option<LarkMediaConfig>,
    /// Semaphore for limiting concurrent downloads (memory safety)
    download_semaphore: Arc<Semaphore>,
    /// Optional in-memory cache (merged from downloader_v2)
    cache: Option<Arc<MediaCache>>,
}

impl MediaDownloader {
    /// Create a new media downloader with connection pooling and memory limits
    ///
    /// # Connection Pool
    /// - Max idle per host: 10 connections (configurable)
    /// - Idle timeout: 90 seconds (configurable)
    ///
    /// # Memory Safety
    /// - Max memory: 512MB default (configurable)
    /// - Max concurrent downloads: 10 (configurable)
    /// - Uses semaphore to prevent OOM under high load
    pub fn new(config: DownloadConfig) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            // Connection pool settings for efficient media downloads
            .pool_max_idle_per_host(config.pool_max_idle_per_host)
            .pool_idle_timeout(Duration::from_secs(config.pool_idle_timeout_secs))
            .connect_timeout(Duration::from_secs(180))
            .build()
            .map_err(|e| AgentError::Platform(format!("Failed to build HTTP client: {}", e)))?;

        // Semaphore limits concurrent downloads to control memory usage
        let download_semaphore = Arc::new(Semaphore::new(config.max_concurrent_downloads));

        Ok(Self {
            config,
            http_client,
            lark_config: None,
            download_semaphore,
            cache: None,
        })
    }

    /// Create with default configuration
    pub fn default() -> Result<Self> {
        Self::new(DownloadConfig::default())
    }

    /// 🟡 P1 FIX: Get or initialize global singleton instance
    ///
    /// This prevents multiple processors from creating separate downloader
    /// instances, saving connection resources and enabling connection pool
    /// sharing.
    pub fn global() -> Result<Arc<Self>> {
        match GLOBAL_DOWNLOADER.get() {
            Some(downloader) => Ok(downloader.clone()),
            None => {
                let downloader = Arc::new(Self::new(DownloadConfig::default())?);
                // Try to set the global instance, if another thread already set it, use that
                // one
                match GLOBAL_DOWNLOADER.set(downloader.clone()) {
                    Ok(()) => Ok(downloader),
                    Err(_) => Ok(GLOBAL_DOWNLOADER.get().unwrap().clone()),
                }
            }
        }
    }

    /// 🟡 P1 FIX: Initialize global singleton with custom config
    ///
    /// Must be called before any call to `global()` if custom configuration is
    /// needed.
    pub fn init_global(config: DownloadConfig) -> Result<Arc<Self>> {
        let downloader = Arc::new(Self::new(config)?);
        match GLOBAL_DOWNLOADER.set(downloader.clone()) {
            Ok(()) => Ok(downloader),
            Err(_) => Err(AgentError::configuration(
                "Global downloader already initialized",
            )),
        }
    }

    /// 🟡 P1 FIX: Initialize global singleton with Lark config
    pub fn init_global_with_lark(
        config: DownloadConfig,
        lark_config: LarkMediaConfig,
    ) -> Result<Arc<Self>> {
        let mut downloader = Self::new(config)?;
        downloader.lark_config = Some(lark_config);
        let downloader = Arc::new(downloader);
        match GLOBAL_DOWNLOADER.set(downloader.clone()) {
            Ok(()) => Ok(downloader),
            Err(_) => Err(AgentError::configuration(
                "Global downloader already initialized",
            )),
        }
    }

    /// Configure Lark-specific settings
    pub fn with_lark_config(mut self, config: LarkMediaConfig) -> Self {
        self.lark_config = Some(config);
        self
    }

    /// Enable in-memory cache with size limit
    pub fn with_cache(mut self, max_size_mb: usize) -> Self {
        self.cache = Some(Arc::new(MediaCache::new(max_size_mb)));
        self
    }

    /// Get current memory usage estimate in MB
    ///
    /// # Estimate Logic
    /// - Each concurrent download ≈ max_file_size memory
    /// - Returns: concurrent_downloads * (max_file_size / 1024 / 1024)
    pub fn estimate_memory_usage_mb(&self) -> usize {
        // Conservative estimate: assume each download uses max_file_size
        let concurrent_downloads = self.config.max_concurrent_downloads;
        let file_size_mb = self.config.max_file_size / 1024 / 1024;
        concurrent_downloads * file_size_mb
    }

    /// Check if memory limit would be exceeded with additional downloads
    pub fn would_exceed_memory_limit(&self, additional_downloads: usize) -> bool {
        let current_estimate = self.estimate_memory_usage_mb();
        let additional_mb = additional_downloads * (self.config.max_file_size / 1024 / 1024);
        (current_estimate + additional_mb) > self.config.max_memory_mb
    }

    /// Download media from URL
    ///
    /// # Arguments
    /// * `url` - URL to download from
    /// * `media_type` - Type of media
    /// * `platform` - Source platform
    ///
    /// # Returns
    /// Download result with file path and metadata
    pub async fn download_from_url(
        &self,
        url: &str,
        media_type: MediaType,
        platform: PlatformType,
    ) -> Result<MediaDownloadResult> {
        info!("Downloading {} from URL: {}", media_type, url);

        // Acquire semaphore permit to limit concurrent downloads (memory safety)
        let _permit = self
            .download_semaphore
            .acquire()
            .await
            .map_err(|_| AgentError::platform("Failed to acquire download permit"))?;

        // Build request
        let mut request = self.http_client.get(url);

        // Add custom headers
        for (key, value) in &self.config.custom_headers {
            request = request.header(key, value);
        }

        // Send request
        let response = request
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to download file: {}", e)))?;

        // Check status
        if !response.status().is_success() {
            return Err(AgentError::platform(format!(
                "Download failed with status: {}",
                response.status()
            )));
        }

        // Get content length
        let content_length = response.content_length().map(|l| l as usize).unwrap_or(0);

        if content_length > self.config.max_file_size {
            return Err(AgentError::platform(format!(
                "File too large: {} bytes (max: {})",
                content_length, self.config.max_file_size
            )));
        }

        // Get content type
        let mime_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| media_type.default_mime_type().to_string());

        // Get filename from Content-Disposition header or URL
        let file_name = self.extract_filename(&response, url, &media_type);

        // Download content
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to read response body: {}", e)))?;

        if bytes.len() > self.config.max_file_size {
            return Err(AgentError::platform(format!(
                "File too large: {} bytes (max: {})",
                bytes.len(),
                self.config.max_file_size
            )));
        }

        // Save file
        let file_path = self
            .save_file(&bytes, &file_name, platform, &media_type)
            .await?;

        info!("Downloaded file to: {:?}", file_path);

        Ok(MediaDownloadResult {
            file_path,
            source: url.to_string(),
            media_type,
            file_size: bytes.len(),
            mime_type,
            file_name,
            downloaded_at: chrono::Utc::now(),
            platform,
            metadata: HashMap::new(),
        })
    }

    /// Download media from Lark/Feishu
    ///
    /// # Arguments
    /// * `file_key` - Lark file key
    /// * `media_type` - Type of media
    ///
    /// # Returns
    /// Download result with file path and metadata
    pub async fn download_from_lark(
        &self,
        file_key: &str,
        media_type: MediaType,
    ) -> Result<MediaDownloadResult> {
        let lark_config = self
            .lark_config
            .as_ref()
            .ok_or_else(|| AgentError::configuration("Lark config not set"))?;

        info!(
            "Downloading {} from Lark with key: {}",
            media_type, file_key
        );

        // Get access token
        let token = self.get_lark_token().await?;

        // Determine API endpoint based on media type
        let endpoint = match media_type {
            MediaType::Image => format!("{}/im/v1/images/{}", lark_config.api_base, file_key),
            MediaType::File => format!("{}/im/v1/files/{}", lark_config.api_base, file_key),
            MediaType::Voice => format!("{}/im/v1/audios/{}", lark_config.api_base, file_key),
            MediaType::Video => format!("{}/im/v1/videos/{}", lark_config.api_base, file_key),
            MediaType::Sticker => format!("{}/im/v1/images/{}", lark_config.api_base, file_key),
        };

        // Build request
        let response = self
            .http_client
            .get(&endpoint)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to download from Lark: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "Lark download failed: {} - {}",
                status, body
            )));
        }

        // Get content type
        let mime_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| media_type.default_mime_type().to_string());

        // Generate filename
        let extension = self.mime_to_extension(&mime_type);
        let file_name = format!("{}.{}", file_key, extension);

        // Download content
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to read response body: {}", e)))?;

        // Save file
        let file_path = self
            .save_file(&bytes, &file_name, PlatformType::Lark, &media_type)
            .await?;

        info!("Downloaded Lark file to: {:?}", file_path);

        let mut metadata = HashMap::new();
        metadata.insert("file_key".to_string(), file_key.to_string());

        Ok(MediaDownloadResult {
            file_path,
            source: file_key.to_string(),
            media_type,
            file_size: bytes.len(),
            mime_type,
            file_name,
            downloaded_at: chrono::Utc::now(),
            platform: PlatformType::Lark,
            metadata,
        })
    }

    /// Download media from generic platform
    ///
    /// # Arguments
    /// * `media_info` - Media information
    /// * `platform` - Source platform
    ///
    /// # Returns
    /// Download result with file path and metadata
    pub async fn download(
        &self,
        media_info: &MediaInfo,
        platform: PlatformType,
    ) -> Result<MediaDownloadResult> {
        match platform {
            PlatformType::Lark => {
                self.download_from_lark(&media_info.file_key, media_info.media_type)
                    .await
            }
            _ => {
                // For other platforms, construct URL from file_key if needed
                // This is a simplified implementation
                Err(AgentError::platform(format!(
                    "Download not implemented for platform: {:?}",
                    platform
                )))
            }
        }
    }

    /// Batch download multiple files
    ///
    /// # Arguments
    /// * `media_infos` - List of media information
    /// * `platform` - Source platform
    ///
    /// # Returns
    /// List of download results (some may be errors)
    pub async fn batch_download(
        &self,
        media_infos: Vec<MediaInfo>,
        platform: PlatformType,
    ) -> Vec<Result<MediaDownloadResult>> {
        let mut results = Vec::with_capacity(media_infos.len());

        for media_info in media_infos {
            let result = self.download(&media_info, platform.clone()).await;
            results.push(result);
        }

        results
    }

    /// Get file info without downloading
    pub async fn get_file_info(&self, url: &str) -> Result<(Option<usize>, Option<String>)> {
        let response = self
            .http_client
            .head(url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get file info: {}", e)))?;

        let content_length = response.content_length().map(|l| l as usize);
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        Ok((content_length, content_type))
    }

    /// Delete downloaded file
    pub async fn delete_file(&self, file_path: &Path) -> Result<()> {
        fs::remove_file(file_path)
            .await
            .map_err(|e| AgentError::platform(format!("Failed to delete file: {}", e)))?;
        Ok(())
    }

    /// Get Lark tenant access token
    async fn get_lark_token(&self) -> Result<String> {
        let config = self
            .lark_config
            .as_ref()
            .ok_or_else(|| AgentError::configuration("Lark config not set"))?;

        // If we already have a token, return it
        if let Some(token) = &config.access_token {
            return Ok(token.clone());
        }

        // Otherwise, fetch a new token
        let url = format!("{}/auth/v3/tenant_access_token/internal", config.api_base);

        let body = serde_json::json!({
            "app_id": config.app_id,
            "app_secret": config.app_secret,
        });

        let response = self
            .http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get Lark token: {}", e)))?;

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse token response: {}", e)))?;

        let token = result
            .get("tenant_access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::authentication("No access token in response"))?;

        Ok(token.to_string())
    }

    /// Extract filename from response or URL
    fn extract_filename(
        &self,
        response: &reqwest::Response,
        url: &str,
        media_type: &MediaType,
    ) -> String {
        // Try Content-Disposition header first
        if let Some(content_disposition) =
            response.headers().get(reqwest::header::CONTENT_DISPOSITION)
        {
            if let Ok(value) = content_disposition.to_str() {
                if let Some(filename) = value.split("filename=").nth(1) {
                    let filename = filename.trim_matches('"').trim_matches('\'');
                    if !filename.is_empty() {
                        return filename.to_string();
                    }
                }
            }
        }

        // Try to extract from URL path
        if let Ok(parsed_url) = url::Url::parse(url) {
            if let Some(segments) = parsed_url.path_segments() {
                if let Some(last_segment) = segments.last() {
                    if !last_segment.is_empty() && last_segment.contains('.') {
                        return last_segment.to_string();
                    }
                }
            }
        }

        // Generate filename with timestamp
        let timestamp = chrono::Utc::now().timestamp_millis();
        let extension = media_type.default_extension();
        format!("media_{}.{}", timestamp, extension)
    }

    /// Save file to disk
    async fn save_file(
        &self,
        bytes: &Bytes,
        file_name: &str,
        platform: PlatformType,
        _media_type: &MediaType,
    ) -> Result<PathBuf> {
        // Build directory path
        let mut dir_path = self.config.download_dir.clone();

        if self.config.organize_by_platform {
            let platform_str = format!("{:?}", platform).to_lowercase();
            dir_path = dir_path.join(platform_str);
        }

        if self.config.organize_by_date {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            dir_path = dir_path.join(today);
        }

        // Create directory if it doesn't exist
        fs::create_dir_all(&dir_path)
            .await
            .map_err(|e| AgentError::platform(format!("Failed to create directory: {}", e)))?;

        // Build file path
        let file_path = dir_path.join(file_name);

        // Write file
        let mut file = fs::File::create(&file_path)
            .await
            .map_err(|e| AgentError::platform(format!("Failed to create file: {}", e)))?;

        file.write_all(bytes)
            .await
            .map_err(|e| AgentError::platform(format!("Failed to write file: {}", e)))?;

        Ok(file_path)
    }

    /// Convert MIME type to file extension
    fn mime_to_extension(&self, mime_type: &str) -> String {
        match mime_type {
            "image/jpeg" | "image/jpg" => "jpg",
            "image/png" => "png",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "audio/mpeg" | "audio/mp3" => "mp3",
            "audio/wav" => "wav",
            "audio/ogg" => "ogg",
            "video/mp4" => "mp4",
            "video/webm" => "webm",
            "video/ogg" => "ogv",
            "application/pdf" => "pdf",
            "application/zip" => "zip",
            "text/plain" => "txt",
            _ => "bin",
        }
        .to_string()
    }

    /// Update configuration
    pub fn update_config(&mut self, config: DownloadConfig) {
        self.config = config;
    }

    /// Get current configuration
    pub fn config(&self) -> &DownloadConfig {
        &self.config
    }

    /// Download raw bytes from URL, using cache if available
    pub async fn download_bytes_from_url(&self, url: &str) -> Result<Vec<u8>> {
        // Check cache first
        if let Some(ref cache) = self.cache {
            if let Some(cached) = cache.get(url).await {
                info!("Media cache hit: {}", url);
                return Ok(cached);
            }
        }

        let response = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to download media: {}", e)))?;

        if !response.status().is_success() {
            return Err(AgentError::platform(format!(
                "Media download failed: HTTP {}",
                response.status()
            )));
        }

        let data = response
            .bytes()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to read media: {}", e)))?
            .to_vec();

        if let Some(ref cache) = self.cache {
            cache.put(url.to_string(), data.clone()).await;
        }

        info!("Media downloaded: {} ({} bytes)", url, data.len());
        Ok(data)
    }

    /// Download media to base64 string with MIME type detection
    pub async fn download_to_base64(&self, url: &str) -> Result<(String, String)> {
        let data = self.download_bytes_from_url(url).await?;
        let mime_type = Self::detect_mime_type(&data);
        let base64 = base64::engine::general_purpose::STANDARD.encode(&data);
        Ok((base64, mime_type))
    }

    /// Create a lazy download URL
    pub fn create_lazy_url(
        &self,
        platform: PlatformType,
        message_id: &str,
        file_key: &str,
        file_type: &str,
    ) -> String {
        LazyUrl::build(platform, message_id, file_key, file_type)
    }

    /// Check if URL uses lazy scheme
    pub fn is_lazy_url(&self, url: &str) -> bool {
        url.starts_with(&format!("{}://", LAZY_SCHEME))
    }

    /// Resolve lazy download URL using a platform-specific downloader
    pub async fn resolve_lazy_url(
        &self,
        url: &str,
        downloader: impl FnOnce(
            String,
            String,
            String,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Vec<u8>>> + Send>,
        >,
    ) -> Result<Vec<u8>> {
        if !self.is_lazy_url(url) {
            return self.download_bytes_from_url(url).await;
        }

        let (_, message_id, file_key, file_type) = LazyUrl::parse(url)
            .ok_or_else(|| AgentError::platform(format!("Invalid lazy URL: {}", url)))?;

        if let Some(ref cache) = self.cache {
            if let Some(cached) = cache.get(url).await {
                info!("Lazy download cache hit: {}", url);
                return Ok(cached);
            }
        }

        info!("Resolving lazy download: {} (type: {})", url, file_type);
        let data = downloader(message_id, file_key, file_type).await?;

        if let Some(ref cache) = self.cache {
            cache.put(url.to_string(), data.clone()).await;
        }

        Ok(data)
    }

    /// Detect MIME type from file header magic numbers
    fn detect_mime_type(data: &[u8]) -> String {
        if data.len() < 4 {
            return "application/octet-stream".to_string();
        }
        match &data[0..4] {
            [0xFF, 0xD8, 0xFF, _] => "image/jpeg".to_string(),
            [0x89, 0x50, 0x4E, 0x47] => "image/png".to_string(),
            [0x47, 0x49, 0x46, 0x38] => "image/gif".to_string(),
            [0x52, 0x49, 0x46, 0x46] => "image/webp".to_string(),
            [0x25, 0x50, 0x44, 0x46] => "application/pdf".to_string(),
            _ => "application/octet-stream".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_type_default_extension() {
        assert_eq!(MediaType::Image.default_extension(), "jpg");
        assert_eq!(MediaType::File.default_extension(), "bin");
        assert_eq!(MediaType::Voice.default_extension(), "mp3");
        assert_eq!(MediaType::Video.default_extension(), "mp4");
        assert_eq!(MediaType::Sticker.default_extension(), "webp");
    }

    #[test]
    fn test_media_type_from_mime_type() {
        assert_eq!(MediaType::from_mime_type("image/jpeg"), MediaType::Image);
        assert_eq!(MediaType::from_mime_type("image/webp"), MediaType::Sticker);
        assert_eq!(MediaType::from_mime_type("audio/mpeg"), MediaType::Voice);
        assert_eq!(MediaType::from_mime_type("video/mp4"), MediaType::Video);
        assert_eq!(
            MediaType::from_mime_type("application/pdf"),
            MediaType::File
        );
    }

    #[test]
    fn test_media_type_display() {
        assert_eq!(format!("{}", MediaType::Image), "image");
        assert_eq!(format!("{}", MediaType::File), "file");
        assert_eq!(format!("{}", MediaType::Voice), "voice");
        assert_eq!(format!("{}", MediaType::Video), "video");
        assert_eq!(format!("{}", MediaType::Sticker), "sticker");
    }

    #[test]
    fn test_download_config_default() {
        let config = DownloadConfig::default();
        assert_eq!(config.max_file_size, 100 * 1024 * 1024);
        assert_eq!(config.timeout_secs, 60);
        assert!(config.preserve_filenames);
        assert!(config.organize_by_platform);
        assert!(config.organize_by_date);
    }

    #[test]
    fn test_global_downloader() {
        // Test that global() returns the same instance
        let downloader1 = MediaDownloader::global().expect("Failed to get global downloader");
        let downloader2 = MediaDownloader::global().expect("Failed to get global downloader");

        // Both should point to the same instance
        assert!(Arc::ptr_eq(&downloader1, &downloader2));
    }

    #[test]
    fn test_lazy_url_build() {
        let url = LazyUrl::build(PlatformType::Lark, "om_123", "img_456", "image");
        assert_eq!(url, "lazy://lark/om_123/img_456/image");
    }

    #[test]
    fn test_lazy_url_parse() {
        let url = "lazy://lark/om_123/img_456/image";
        let (platform, msg_id, file_key, file_type) = LazyUrl::parse(url).unwrap();

        assert!(matches!(platform, PlatformType::Lark));
        assert_eq!(msg_id, "om_123");
        assert_eq!(file_key, "img_456");
        assert_eq!(file_type, "image");
    }

    #[test]
    fn test_detect_mime_type() {
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(MediaDownloader::detect_mime_type(&jpeg), "image/jpeg");

        let png = vec![0x89, 0x50, 0x4E, 0x47];
        assert_eq!(MediaDownloader::detect_mime_type(&png), "image/png");

        let pdf = vec![0x25, 0x50, 0x44, 0x46];
        assert_eq!(MediaDownloader::detect_mime_type(&pdf), "application/pdf");
    }
}

/// Lazy download URL scheme
pub const LAZY_SCHEME: &str = "lazy";

/// Media download cache
pub struct MediaCache {
    cache: RwLock<HashMap<String, Vec<u8>>>,
    max_size: usize,
    current_size: RwLock<usize>,
}

impl MediaCache {
    pub fn new(max_size_mb: usize) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            max_size: max_size_mb * 1024 * 1024,
            current_size: RwLock::new(0),
        }
    }

    pub async fn get(&self, key: &str) -> Option<Vec<u8>> {
        let cache = self.cache.read().await;
        cache.get(key).cloned()
    }

    pub async fn put(&self, key: String, data: Vec<u8>) {
        let size = data.len();
        {
            let current = *self.current_size.read().await;
            if current + size > self.max_size {
                warn!("Media cache full, skipping cache for {}", key);
                return;
            }
        }
        let mut cache = self.cache.write().await;
        let mut current = self.current_size.write().await;
        *current += size;
        cache.insert(key.clone(), data);
        debug!(
            "Media cached: {} (size: {} bytes, total: {} bytes)",
            key, size, *current
        );
    }

    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        let mut current = self.current_size.write().await;
        cache.clear();
        *current = 0;
        info!("Media cache cleared");
    }
}

/// Lazy download URL builder/parser
pub struct LazyUrl;

impl LazyUrl {
    pub fn build(
        platform: PlatformType,
        message_id: &str,
        file_key: &str,
        file_type: &str,
    ) -> String {
        format!(
            "{}://{}/{}/{}/{}",
            LAZY_SCHEME, platform, message_id, file_key, file_type
        )
    }

    pub fn parse(url: &str) -> Option<(PlatformType, String, String, String)> {
        if !url.starts_with(&format!("{}://", LAZY_SCHEME)) {
            return None;
        }
        let parts: Vec<&str> = url.split('/').collect();
        // URL format: lazy://<platform>/<msg_id>/<file_key>/<file_type>
        // parts: ["lazy:", "", "<platform>", "<msg_id>", "<file_key>", "<file_type>"]
        if parts.len() < 6 {
            return None;
        }
        let platform = match parts[2] {
            "lark" => PlatformType::Lark,
            "dingtalk" => PlatformType::DingTalk,
            "telegram" => PlatformType::Telegram,
            "discord" => PlatformType::Discord,
            "slack" => PlatformType::Slack,
            _ => return None,
        };
        Some((
            platform,
            parts[3].to_string(),
            parts[4].to_string(),
            parts[5].to_string(),
        ))
    }
}

/// Platform-specific media downloader trait
#[async_trait::async_trait]
pub trait PlatformMediaDownloader: Send + Sync {
    async fn download_message_resource(
        &self,
        message_id: &str,
        file_key: &str,
        resource_type: &str,
    ) -> Result<Vec<u8>>;
}
