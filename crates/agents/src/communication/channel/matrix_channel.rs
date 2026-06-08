//! Matrix Channel Implementation
//!
//! Unified Channel trait implementation for Matrix protocol.
//! Supports WebSocket sync (default) and HTTP polling fallback.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{error, info};

use super::r#trait::{BaseChannelConfig, ConnectionMode, ContentType};
use super::{Channel, ChannelConfig, ChannelEvent, ChannelInfo, MemberInfo};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Matrix sync timeout in milliseconds
const MATRIX_SYNC_TIMEOUT_MS: u64 = 180000;

/// Matrix Channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixChannelConfig {
    /// Homeserver URL
    pub homeserver: String,
    /// Username
    pub username: String,
    /// Password or access token
    pub credential: MatrixCredential,
    /// Device ID (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

/// Matrix credential type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MatrixCredential {
    #[serde(rename = "password")]
    Password { password: String },
    #[serde(rename = "token")]
    Token { access_token: String },
}

impl Default for MatrixChannelConfig {
    fn default() -> Self {
        Self {
            homeserver: "https://matrix.org".to_string(),
            username: String::new(),
            credential: MatrixCredential::Password {
                password: String::new(),
            },
            device_id: None,
            base: BaseChannelConfig::default(),
        }
    }
}

impl ChannelConfig for MatrixChannelConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        let homeserver =
            std::env::var("MATRIX_HOMESERVER").unwrap_or_else(|_| "https://matrix.org".to_string());
        let username = std::env::var("MATRIX_USERNAME").ok()?;

        let credential = if let Ok(token) = std::env::var("MATRIX_ACCESS_TOKEN") {
            MatrixCredential::Token {
                access_token: token,
            }
        } else {
            let password = std::env::var("MATRIX_PASSWORD").ok()?;
            MatrixCredential::Password { password }
        };

        let base = BaseChannelConfig::from_env("MATRIX")?;
        let device_id = std::env::var("MATRIX_DEVICE_ID").ok();

        Some(Self {
            homeserver,
            username,
            credential,
            device_id,
            base,
        })
    }

    fn is_valid(&self) -> bool {
        !self.username.is_empty()
            && match &self.credential {
                MatrixCredential::Password { password } => !password.is_empty(),
                MatrixCredential::Token { access_token } => !access_token.is_empty(),
            }
    }

    fn allowlist(&self) -> Vec<String> {
        vec![]
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.base.connection_mode
    }

    fn auto_reconnect(&self) -> bool {
        self.base.auto_reconnect
    }

    fn max_reconnect_attempts(&self) -> u32 {
        self.base.max_reconnect_attempts
    }
}

/// Matrix login request
#[derive(Debug, Clone, Serialize)]
pub struct MatrixLoginRequest {
    #[serde(rename = "type")]
    pub login_type: String,
    pub identifier: MatrixIdentifier,
    pub password: Option<String>,
    pub token: Option<String>,
    #[serde(rename = "device_id")]
    pub device_id: Option<String>,
}

/// Matrix identifier
#[derive(Debug, Clone, Serialize)]
pub struct MatrixIdentifier {
    #[serde(rename = "type")]
    pub id_type: String,
    pub user: String,
}

/// Matrix login response
#[derive(Debug, Clone, Deserialize)]
pub struct MatrixLoginResponse {
    #[serde(rename = "access_token")]
    pub access_token: String,
    #[serde(rename = "device_id")]
    pub device_id: String,
    #[serde(rename = "home_server")]
    pub home_server: String,
    #[serde(rename = "user_id")]
    pub user_id: String,
}

/// Matrix event
#[derive(Debug, Clone, Deserialize)]
pub struct MatrixEvent {
    #[serde(rename = "event_id")]
    pub event_id: String,
    pub sender: String,
    #[serde(rename = "origin_server_ts")]
    pub origin_server_ts: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    pub content: serde_json::Value,
}

/// Matrix sync response
#[derive(Debug, Clone, Deserialize)]
pub struct MatrixSyncResponse {
    #[serde(rename = "next_batch")]
    pub next_batch: String,
    pub rooms: Option<MatrixRooms>,
}

/// Matrix rooms
#[derive(Debug, Clone, Deserialize)]
pub struct MatrixRooms {
    pub join: Option<std::collections::HashMap<String, MatrixJoinedRoom>>,
}

/// Matrix joined room
#[derive(Debug, Clone, Deserialize)]
pub struct MatrixJoinedRoom {
    pub timeline: Option<MatrixTimeline>,
}

/// Matrix timeline
#[derive(Debug, Clone, Deserialize)]
pub struct MatrixTimeline {
    pub events: Vec<MatrixEvent>,
}

/// Matrix send response
#[derive(Debug, Clone, Deserialize)]
pub struct MatrixSendResponse {
    #[serde(rename = "event_id")]
    pub event_id: String,
}

/// Matrix Channel implementation
pub struct MatrixChannel {
    config: MatrixChannelConfig,
    http_client: reqwest::Client,
    access_token: Arc<RwLock<Option<String>>>,
    user_id: Arc<RwLock<Option<String>>>,
    device_id: Arc<RwLock<Option<String>>>,
    connected: Arc<RwLock<bool>>,
    next_batch: Arc<RwLock<Option<String>>>,
    listener_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl MatrixChannel {
    /// Create a new Matrix channel
    pub fn new(config: MatrixChannelConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
            access_token: Arc::new(RwLock::new(None)),
            user_id: Arc::new(RwLock::new(None)),
            device_id: Arc::new(RwLock::new(None)),
            connected: Arc::new(RwLock::new(false)),
            next_batch: Arc::new(RwLock::new(None)),
            listener_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self> {
        let config = MatrixChannelConfig::from_env().ok_or_else(|| {
            AgentError::configuration("MATRIX_USERNAME or MATRIX_PASSWORD not set")
        })?;
        Ok(Self::new(config))
    }

    /// Login to Matrix
    async fn login(&self) -> Result<()> {
        match &self.config.credential {
            MatrixCredential::Token { access_token } => {
                *self.access_token.write().await = Some(access_token.clone());
                // Get user ID from whoami
                let url = format!(
                    "{}/_matrix/client/r0/account/whoami",
                    self.config.homeserver
                );
                let response = self
                    .http_client
                    .get(&url)
                    .header("Authorization", format!("Bearer {}", access_token))
                    .send()
                    .await
                    .map_err(|e| AgentError::platform(format!("Failed to verify token: {}", e)))?;

                let data: serde_json::Value = response.json().await.map_err(|e| {
                    AgentError::platform(format!("Failed to parse response: {}", e))
                })?;

                let user_id = data
                    .get("user_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AgentError::authentication("Invalid token"))?;

                *self.user_id.write().await = Some(user_id.to_string());
                info!("Matrix logged in as {} (via token)", user_id);
            }
            MatrixCredential::Password { password } => {
                let url = format!("{}/_matrix/client/r0/login", self.config.homeserver);

                let login_request = MatrixLoginRequest {
                    login_type: "m.login.password".to_string(),
                    identifier: MatrixIdentifier {
                        id_type: "m.id.user".to_string(),
                        user: self.config.username.clone(),
                    },
                    password: Some(password.clone()),
                    token: None,
                    device_id: self.config.device_id.clone(),
                };

                let response = self
                    .http_client
                    .post(&url)
                    .json(&login_request)
                    .send()
                    .await
                    .map_err(|e| AgentError::platform(format!("Failed to login: {}", e)))?;

                let login_response: MatrixLoginResponse = response.json().await.map_err(|e| {
                    AgentError::authentication(format!("Failed to parse login response: {}", e))
                })?;

                *self.access_token.write().await = Some(login_response.access_token);
                *self.user_id.write().await = Some(login_response.user_id.clone());
                *self.device_id.write().await = Some(login_response.device_id);

                info!("Matrix logged in as {}", login_response.user_id);
            }
        }

        Ok(())
    }

    /// Get access token
    async fn get_access_token(&self) -> Result<String> {
        self.access_token
            .read()
            .await
            .clone()
            .ok_or_else(|| AgentError::authentication("Not logged in").into())
    }

    /// Send text message to room
    pub async fn send_text_message(&self, room_id: &str, body: &str) -> Result<String> {
        let token = self.get_access_token().await?;
        let url = format!(
            "{}/_matrix/client/r0/rooms/{}/send/m.room.message",
            self.config.homeserver,
            urlencoding::encode(room_id)
        );

        let content = serde_json::json!({
            "msgtype": "m.text",
            "body": body,
        });

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&content)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send message: {}", e)))?;

        let send_response: MatrixSendResponse = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse send response: {}", e)))?;

        Ok(send_response.event_id)
    }

    /// Sync with homeserver
    async fn sync(&self) -> Result<MatrixSyncResponse> {
        let token = self.get_access_token().await?;

        let mut url = format!(
            "{}/_matrix/client/r0/sync?timeout={}",
            self.config.homeserver, MATRIX_SYNC_TIMEOUT_MS
        );

        if let Some(next_batch) = self.next_batch.read().await.clone() {
            url.push_str(&format!("&since={}", next_batch));
        }

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to sync: {}", e)))?;

        let sync_response: MatrixSyncResponse = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse sync response: {}", e)))?;

        *self.next_batch.write().await = Some(sync_response.next_batch.clone());

        Ok(sync_response)
    }

    /// Convert Matrix event to internal Message
    fn convert_event(&self, room_id: &str, event: &MatrixEvent) -> Option<Message> {
        if event.event_type != "m.room.message" {
            return None;
        }

        let body = event
            .content
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let msgtype = event
            .content
            .get("msgtype")
            .and_then(|v| v.as_str())
            .unwrap_or("m.text");

        let message_type = match msgtype {
            "m.text" => MessageType::Text,
            "m.image" => MessageType::Image,
            "m.audio" => MessageType::Voice,
            "m.video" => MessageType::Video,
            "m.file" => MessageType::File,
            _ => MessageType::System,
        };

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("room_id".to_string(), room_id.to_string());
        metadata.insert("event_id".to_string(), event.event_id.clone());
        metadata.insert("sender".to_string(), event.sender.clone());

        let timestamp = chrono::DateTime::from_timestamp_millis(event.origin_server_ts)?;

        Some(Message {
            id: uuid::Uuid::new_v4(),
            thread_id: uuid::Uuid::new_v4(),
            platform: PlatformType::Matrix,
            message_type,
            content: body,
            metadata,
            timestamp,
        })
    }

    /// Run polling listener
    async fn run_polling_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        let mut interval = interval(Duration::from_secs(5));

        loop {
            interval.tick().await;

            match self.sync().await {
                Ok(sync_response) => {
                    if let Some(rooms) = sync_response.rooms {
                        if let Some(joined) = rooms.join {
                            for (room_id, room_data) in joined {
                                if let Some(timeline) = room_data.timeline {
                                    for event in timeline.events {
                                        if let Some(message) = self.convert_event(&room_id, &event)
                                        {
                                            let channel_event = ChannelEvent::MessageReceived {
                                                platform: PlatformType::Matrix,
                                                channel_id: room_id.clone(),
                                                message,
                                            };
                                            let _ = event_bus.send(channel_event).await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Sync error: {}", e);
                    if !self.config.base.auto_reconnect {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Run WebSocket listener (Matrix doesn't have native WebSocket, use
    /// polling)
    #[allow(dead_code)]
    async fn run_websocket_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        // Matrix doesn't have a native WebSocket API for sync
        // Use polling instead
        self.run_polling_listener(event_bus).await
    }
}

#[async_trait]
impl Channel for MatrixChannel {
    fn name(&self) -> &str {
        "matrix"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::Matrix
    }

    fn is_connected(&self) -> bool {
        if let Ok(connected) = self.connected.try_read() {
            *connected
        } else {
            false
        }
    }

    async fn connect(&mut self) -> Result<()> {
        self.login().await?;
        *self.connected.write().await = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.stop_listener().await?;
        *self.access_token.write().await = None;
        *self.user_id.write().await = None;
        *self.connected.write().await = false;
        info!("Disconnected from Matrix");
        Ok(())
    }

    async fn send(&self, channel_id: &str, message: &Message) -> Result<()> {
        match message.message_type {
            MessageType::Text => {
                self.send_text_message(channel_id, &message.content).await?;
            }
            _ => {
                self.send_text_message(channel_id, &message.content).await?;
            }
        }
        Ok(())
    }

    async fn start_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        self.stop_listener().await?;

        let channel = self.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = channel.run_polling_listener(event_bus).await {
                error!("Matrix listener error: {}", e);
            }
        });
        *self.listener_handle.write().await = Some(handle);

        Ok(())
    }

    async fn stop_listener(&self) -> Result<()> {
        if let Some(handle) = self.listener_handle.write().await.take() {
            handle.abort();
        }
        Ok(())
    }

    fn supported_content_types(&self) -> Vec<ContentType> {
        vec![
            ContentType::Text,
            ContentType::Image,
            ContentType::File,
            ContentType::Audio,
            ContentType::Video,
            ContentType::Rich,
        ]
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        // Matrix has joined_rooms API
        Ok(vec![])
    }

    async fn list_members(&self, _channel_id: &str) -> Result<Vec<MemberInfo>> {
        // Matrix has joined_members API
        Ok(vec![])
    }

    fn connection_mode(&self) -> ConnectionMode {
        // Matrix always uses polling for sync
        ConnectionMode::Polling
    }
}

impl Clone for MatrixChannel {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            http_client: self.http_client.clone(),
            access_token: self.access_token.clone(),
            user_id: self.user_id.clone(),
            device_id: self.device_id.clone(),
            connected: self.connected.clone(),
            next_batch: self.next_batch.clone(),
            listener_handle: Arc::new(RwLock::new(None)),
        }
    }
}
