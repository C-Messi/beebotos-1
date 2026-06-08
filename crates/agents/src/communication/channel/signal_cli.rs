//! Signal CLI Manager
//!
//! Manages signal-cli process lifecycle, JSON-RPC client, and SSE event stream
//! parsing. Provides a higher-level interface for signal-cli operations.

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::error::{AgentError, Result};

/// Signal CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalCliConfig {
    /// Path to signal-cli executable
    pub executable_path: String,
    /// Data directory for signal-cli
    pub data_dir: String,
    /// HTTP JSON-RPC host
    pub http_host: String,
    /// HTTP JSON-RPC port
    pub http_port: u16,
    /// Log level
    pub log_level: String,
    /// Whether to use native mode (faster, requires libsignal-client)
    pub native_mode: bool,
}

impl Default for SignalCliConfig {
    fn default() -> Self {
        Self {
            executable_path: "signal-cli".to_string(),
            data_dir: "./data/signal".to_string(),
            http_host: "127.0.0.1".to_string(),
            http_port: 8080,
            log_level: "info".to_string(),
            native_mode: true,
        }
    }
}

impl SignalCliConfig {
    /// Create from environment variables
    pub fn from_env() -> Self {
        Self {
            executable_path: std::env::var("SIGNAL_CLI_PATH")
                .unwrap_or_else(|_| "signal-cli".to_string()),
            data_dir: std::env::var("SIGNAL_DATA_DIR")
                .unwrap_or_else(|_| "./data/signal".to_string()),
            http_host: std::env::var("SIGNAL_HTTP_HOST")
                .unwrap_or_else(|_| "127.0.0.1".to_string()),
            http_port: std::env::var("SIGNAL_HTTP_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8080),
            log_level: std::env::var("SIGNAL_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            native_mode: std::env::var("SIGNAL_NATIVE_MODE")
                .map(|s| s != "false")
                .unwrap_or(true),
        }
    }

    /// Get HTTP base URL
    pub fn http_base_url(&self) -> String {
        format!("http://{}:{}", self.http_host, self.http_port)
    }
}

/// Signal CLI process state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalCliState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error(String),
}

/// Signal CLI Manager
pub struct SignalCliManager {
    config: SignalCliConfig,
    state: Arc<RwLock<SignalCliState>>,
    child: Arc<Mutex<Option<Child>>>,
    http_client: reqwest::Client,
    event_tx: Arc<Mutex<Option<mpsc::UnboundedSender<SignalEvent>>>>,
    event_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<SignalEvent>>>>,
}

/// Signal event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SignalEvent {
    #[serde(rename = "message")]
    Message { envelope: Envelope, account: String },
    #[serde(rename = "receipt")]
    Receipt { data: serde_json::Value },
    #[serde(rename = "typing")]
    Typing { envelope: Envelope, account: String },
    #[serde(rename = "sync")]
    Sync { data: serde_json::Value },
    #[serde(rename = "status")]
    Status {
        connected: bool,
        state: String,
        account: Option<String>,
    },
    #[serde(rename = "error")]
    Error { message: String, code: Option<i32> },
    #[serde(rename = "qr")]
    Qr { qr_code: String },
    #[serde(rename = "linked")]
    Linked {
        device_id: String,
        device_name: String,
    },
}

/// Signal envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub source: String,
    pub source_number: Option<String>,
    pub source_uuid: Option<String>,
    pub source_name: Option<String>,
    pub source_device: i32,
    pub timestamp: i64,
    pub server_timestamp: i64,
    #[serde(rename = "type")]
    pub message_type: String,
    pub data_message: Option<DataMessage>,
    pub sync_message: Option<serde_json::Value>,
    pub call_message: Option<serde_json::Value>,
    pub receipt_message: Option<serde_json::Value>,
    pub typing_message: Option<TypingMessage>,
}

/// Signal data message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataMessage {
    pub timestamp: i64,
    pub message: Option<String>,
    pub expires_in_seconds: Option<i32>,
    pub view_once: Option<bool>,
    pub attachments: Vec<AttachmentInfo>,
    pub quote: Option<QuoteInfo>,
    pub reaction: Option<ReactionInfo>,
    pub remote_delete: Option<RemoteDeleteInfo>,
    pub sticker: Option<StickerInfo>,
    pub group_info: Option<GroupInfo>,
}

/// Attachment info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentInfo {
    pub id: String,
    pub content_type: String,
    pub filename: String,
    pub size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub voice_note: Option<bool>,
}

/// Quote info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteInfo {
    pub id: i64,
    pub author: String,
    pub text: Option<String>,
}

/// Reaction info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionInfo {
    pub emoji: String,
    pub target_author: String,
    pub target_sent_timestamp: i64,
    pub is_remove: bool,
}

/// Remote delete info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteDeleteInfo {
    pub target_sent_timestamp: i64,
}

/// Sticker info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickerInfo {
    pub sticker_id: String,
    pub pack_id: String,
    pub emoji: Option<String>,
}

/// Group info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    pub group_id: String,
    pub group_name: Option<String>,
}

/// Typing message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypingMessage {
    pub action: String,
    pub timestamp: i64,
    pub group_id: Option<String>,
}

impl SignalCliManager {
    /// Create a new Signal CLI manager
    pub fn new(config: SignalCliConfig) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Self {
            config,
            state: Arc::new(RwLock::new(SignalCliState::Stopped)),
            child: Arc::new(Mutex::new(None)),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(180))
                .build()
                .unwrap_or_default(),
            event_tx: Arc::new(Mutex::new(Some(event_tx))),
            event_rx: Arc::new(Mutex::new(Some(event_rx))),
        }
    }

    /// Create with default configuration
    pub fn default() -> Self {
        Self::new(SignalCliConfig::default())
    }

    /// Create from environment variables
    pub fn from_env() -> Self {
        Self::new(SignalCliConfig::from_env())
    }

    /// Get current state
    pub async fn state(&self) -> SignalCliState {
        self.state.read().await.clone()
    }

    /// Check if running
    pub async fn is_running(&self) -> bool {
        matches!(self.state().await, SignalCliState::Running)
    }

    /// Start the signal-cli daemon
    pub async fn start(&self) -> Result<()> {
        let mut state = self.state.write().await;
        if matches!(*state, SignalCliState::Running | SignalCliState::Starting) {
            return Ok(());
        }
        *state = SignalCliState::Starting;
        drop(state);

        // Ensure data directory exists
        tokio::fs::create_dir_all(&self.config.data_dir)
            .await
            .map_err(|e| AgentError::platform(format!("Failed to create data directory: {}", e)))?;

        // Build command
        let mut cmd = Command::new(&self.config.executable_path);
        cmd.arg("--config")
            .arg(&self.config.data_dir)
            .arg("daemon")
            .arg("--http")
            .arg("--http-host")
            .arg(&self.config.http_host)
            .arg("--http-port")
            .arg(self.config.http_port.to_string());

        if self.config.native_mode {
            // Native mode is default in newer signal-cli versions
            debug!("Using native mode for signal-cli");
        }

        // Spawn process
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AgentError::platform(format!("Failed to start signal-cli: {}", e)))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AgentError::platform("Failed to get stdout"))?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AgentError::platform("Failed to get stderr"))?;

        // Start event processing
        let state_arc = self.state.clone();
        let event_tx = self.event_tx.lock().await.clone();

        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                debug!("signal-cli: {}", line);

                // Try to parse as event
                if let Ok(event) = serde_json::from_str::<SignalEvent>(&line) {
                    // Update state based on event first (before sending)
                    match &event {
                        SignalEvent::Status { connected, .. } => {
                            let mut state = state_arc.write().await;
                            if *connected {
                                *state = SignalCliState::Running;
                            } else {
                                *state = SignalCliState::Stopped;
                            }
                        }
                        SignalEvent::Error { message, .. } => {
                            warn!("signal-cli error: {}", message);
                            let mut state = state_arc.write().await;
                            *state = SignalCliState::Error(message.clone());
                        }
                        _ => {}
                    }

                    // Send event to handler
                    if let Some(ref tx) = event_tx {
                        let _ = tx.send(event);
                    }
                }
            }

            debug!("signal-cli stdout reader ended");
        });

        // Log stderr
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                debug!("signal-cli stderr: {}", line);
            }
        });

        *self.child.lock().await = Some(child);

        // Wait for HTTP interface to be ready
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Verify HTTP interface is accessible
        match self.check_health().await {
            Ok(_) => {
                info!(
                    "signal-cli daemon started on port {}",
                    self.config.http_port
                );
                Ok(())
            }
            Err(e) => {
                let mut state = self.state.write().await;
                *state = SignalCliState::Error(e.to_string());
                Err(e)
            }
        }
    }

    /// Stop the daemon
    pub async fn stop(&self) -> Result<()> {
        let mut state = self.state.write().await;
        if matches!(*state, SignalCliState::Stopped | SignalCliState::Stopping) {
            return Ok(());
        }
        *state = SignalCliState::Stopping;
        drop(state);

        let mut child_lock = self.child.lock().await;
        if let Some(mut child) = child_lock.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }

        *self.state.write().await = SignalCliState::Stopped;
        info!("signal-cli daemon stopped");

        Ok(())
    }

    /// Restart the daemon
    pub async fn restart(&self) -> Result<()> {
        self.stop().await?;
        tokio::time::sleep(Duration::from_secs(2)).await;
        self.start().await
    }

    /// Check if HTTP interface is healthy
    pub async fn check_health(&self) -> Result<()> {
        let url = format!("{}/v1/accounts", self.config.http_base_url());

        let response = timeout(Duration::from_secs(5), self.http_client.get(&url).send())
            .await
            .map_err(|_| AgentError::platform("Health check timeout"))?
            .map_err(|e| AgentError::platform(format!("Health check failed: {}", e)))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(AgentError::platform(format!(
                "Health check failed: {}",
                response.status()
            )))
        }
    }

    /// Get event receiver
    pub async fn take_event_receiver(&self) -> Option<mpsc::UnboundedReceiver<SignalEvent>> {
        self.event_rx.lock().await.take()
    }

    /// Get accounts
    pub async fn get_accounts(&self) -> Result<Vec<String>> {
        let url = format!("{}/v1/accounts", self.config.http_base_url());

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get accounts: {}", e)))?;

        if !response.status().is_success() {
            return Err(AgentError::platform(format!(
                "Failed to get accounts: {}",
                response.status()
            )));
        }

        let accounts: Vec<String> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse accounts: {}", e)))?;

        Ok(accounts)
    }

    /// Check if account is registered
    pub async fn is_registered(&self, phone_number: &str) -> Result<bool> {
        let accounts = self.get_accounts().await?;
        Ok(accounts.contains(&phone_number.to_string()))
    }

    /// Register a new account
    pub async fn register(
        &self,
        phone_number: &str,
        voice: bool,
        captcha: Option<&str>,
    ) -> Result<()> {
        let url = format!(
            "{}/v1/register/{}",
            self.config.http_base_url(),
            phone_number
        );

        let body = serde_json::json!({
            "voice": voice,
            "captcha": captcha,
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to register: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::platform(format!(
                "Registration failed: {}",
                error_text
            )));
        }

        Ok(())
    }

    /// Verify registration code
    pub async fn verify(&self, phone_number: &str, code: &str, pin: Option<&str>) -> Result<()> {
        let url = format!(
            "{}/v1/register/{}/verify",
            self.config.http_base_url(),
            phone_number
        );

        let body = serde_json::json!({
            "token": code,
            "pin": pin,
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to verify: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::platform(format!(
                "Verification failed: {}",
                error_text
            )));
        }

        Ok(())
    }

    /// Link a device
    pub async fn link_device(&self, device_name: &str) -> Result<String> {
        let url = format!("{}/v1/devices/{}", self.config.http_base_url(), device_name);

        let response = self
            .http_client
            .post(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to link device: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::platform(format!("Link failed: {}", error_text)));
        }

        // The QR code will be received via event
        Ok("Link initiated. Check events for QR code.".to_string())
    }

    /// Get linked devices
    pub async fn get_linked_devices(&self, phone_number: &str) -> Result<Vec<serde_json::Value>> {
        let url = format!(
            "{}/v1/devices/{}",
            self.config.http_base_url(),
            phone_number
        );

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get devices: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::platform(format!(
                "Get devices failed: {}",
                error_text
            )));
        }

        let devices: Vec<serde_json::Value> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse devices: {}", e)))?;

        Ok(devices)
    }

    /// Remove linked device
    pub async fn remove_device(&self, phone_number: &str, device_id: i64) -> Result<()> {
        let url = format!(
            "{}/v1/devices/{}/{}",
            self.config.http_base_url(),
            phone_number,
            device_id
        );

        let response = self
            .http_client
            .delete(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to remove device: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::platform(format!(
                "Remove device failed: {}",
                error_text
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_cli_config_default() {
        let config = SignalCliConfig::default();
        assert_eq!(config.executable_path, "signal-cli");
        assert_eq!(config.http_port, 8080);
        assert!(config.native_mode);
    }

    #[test]
    fn test_signal_cli_state() {
        let state = SignalCliState::Running;
        assert_eq!(state, SignalCliState::Running);
        assert_ne!(state, SignalCliState::Stopped);
    }

    #[test]
    fn test_signal_event_deserialization() {
        let json = r#"{"type": "message", "envelope": {"source": "+1234567890", "source_device": 1, "timestamp": 1234567890, "server_timestamp": 1234567890, "type": "CIPHERTEXT"}, "account": "+0987654321"}"#;
        let event: SignalEvent = serde_json::from_str(json).unwrap();
        match event {
            SignalEvent::Message { envelope, account } => {
                assert_eq!(envelope.source, "+1234567890");
                assert_eq!(account, "+0987654321");
            }
            _ => panic!("Expected Message event"),
        }
    }
}
