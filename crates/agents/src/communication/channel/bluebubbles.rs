//! BlueBubbles API Client
//!
//! Provides integration with BlueBubbles server for iMessage support.
//! BlueBubbles is a macOS app that provides an API for iMessage.
//!
//! Features:
//! - REST API client for sending messages
//! - WebSocket connection for real-time message receiving
//! - Auto-reconnect with exponential backoff
//! - Media upload and download
//! - Tapback (reaction) support
//! - Typing indicators

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage, WebSocketStream};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::error::{AgentError, Result};

use super::r#trait::BaseChannelConfig;

/// BlueBubbles server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesConfig {
    /// Server host (e.g., "mac-mini.local" or IP address)
    pub host: String,
    /// Server port (default: 12345)
    pub port: u16,
    /// API key for authentication (if required)
    pub api_key: Option<String>,
    /// Password for authentication (if required)
    pub password: Option<String>,
    /// WebSocket path (default: "/ws")
    pub ws_path: String,
    /// REST API path prefix (default: "/api/v1")
    pub api_prefix: String,
    /// Use HTTPS/WSS (default: false for local network)
    pub use_tls: bool,
    /// Reconnect interval in seconds
    pub reconnect_interval_secs: u64,
    /// Request timeout in seconds
    pub request_timeout_secs: u64,
    #[serde(flatten)]
    pub base: super::r#trait::BaseChannelConfig,
}

impl Default for BlueBubblesConfig {
    fn default() -> Self {
        let mut base = super::r#trait::BaseChannelConfig::default();
        // BlueBubbles uses WebSocket mode
        base.connection_mode = super::r#trait::ConnectionMode::WebSocket;
        
        Self {
            host: "mac-mini.local".to_string(),
            port: 12345,
            api_key: None,
            password: None,
            ws_path: "/ws".to_string(),
            api_prefix: "/api/v1".to_string(),
            use_tls: false,
            reconnect_interval_secs: 5,
            request_timeout_secs: 180,
            base,
        }
    }
}

impl BlueBubblesConfig {
    /// Create from environment variables
    pub fn from_env() -> Self {
        let base = super::r#trait::BaseChannelConfig::from_env("BLUEBUBBLES")
            .unwrap_or_default();
        
        Self {
            host: std::env::var("BLUEBUBBLES_HOST").unwrap_or_else(|_| "mac-mini.local".to_string()),
            port: std::env::var("BLUEBUBBLES_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(12345),
            api_key: std::env::var("BLUEBUBBLES_API_KEY").ok(),
            password: std::env::var("BLUEBUBBLES_PASSWORD").ok(),
            ws_path: std::env::var("BLUEBUBBLES_WS_PATH").unwrap_or_else(|_| "/ws".to_string()),
            api_prefix: std::env::var("BLUEBUBBLES_API_PREFIX")
                .unwrap_or_else(|_| "/api/v1".to_string()),
            use_tls: std::env::var("BLUEBUBBLES_USE_TLS")
                .map(|s| s == "true")
                .unwrap_or(false),
            reconnect_interval_secs: std::env::var("BLUEBUBBLES_RECONNECT_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            request_timeout_secs: std::env::var("BLUEBUBBLES_REQUEST_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            base,
        }
    }

    /// Get base URL for REST API
    pub fn base_url(&self) -> String {
        let protocol = if self.use_tls { "https" } else { "http" };
        format!("{}://{}:{}", protocol, self.host, self.port)
    }

    /// Get WebSocket URL
    pub fn ws_url(&self) -> String {
        let protocol = if self.use_tls { "wss" } else { "ws" };
        format!("{}://{}:{}{}", protocol, self.host, self.port, self.ws_path)
    }
}

/// Connection state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// BlueBubbles message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesMessage {
    /// Unique message GUID
    pub guid: String,
    /// Chat GUID
    pub chat_guid: String,
    /// Sender handle (phone number or email)
    pub handle: Option<String>,
    /// Sender display name
    pub display_name: Option<String>,
    /// Message text
    pub text: Option<String>,
    /// Message date (Unix timestamp)
    pub date: i64,
    /// Whether message is from me
    pub is_from_me: bool,
    /// Whether message has been delivered
    pub is_delivered: bool,
    /// Whether message has been read
    pub is_read: bool,
    /// Attachments
    pub attachments: Vec<BlueBubblesAttachment>,
    /// Associated message GUID (for reactions/tapbacks)
    pub associated_message_guid: Option<String>,
    /// Associated message type (for reactions/tapbacks)
    pub associated_message_type: Option<i32>,
    /// Thread originator GUID (for replies)
    pub thread_originator_guid: Option<String>,
    /// Thread originator part (for replies)
    pub thread_originator_part: Option<String>,
    /// Balloon bundle ID (for special message types)
    pub balloon_bundle_id: Option<String>,
    /// Expressive send style ID
    pub expressive_send_style_id: Option<String>,
    /// Tapback (reaction) details
    pub tapback: Option<BlueBubblesTapback>,
}

/// BlueBubbles attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesAttachment {
    /// Attachment GUID
    pub guid: String,
    /// File name
    pub transfer_name: Option<String>,
    /// MIME type
    pub mime_type: Option<String>,
    /// File size in bytes
    pub total_bytes: Option<i64>,
    /// File path on server
    pub file_path: Option<String>,
    /// Width (for images/videos)
    pub width: Option<i32>,
    /// Height (for images/videos)
    pub height: Option<i32>,
    /// Duration (for audio/video)
    pub duration: Option<i32>,
}

/// BlueBubbles tapback (reaction)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesTapback {
    /// Tapback type (0-5)
    pub tapback_type: i32,
    /// Sender handle
    pub handle: String,
    /// Associated message GUID
    pub associated_message_guid: String,
}

/// Tapback types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapbackType {
    Love = 0,
    Like = 1,
    Dislike = 2,
    Laugh = 3,
    Emphasis = 4,
    Question = 5,
}

impl TapbackType {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(TapbackType::Love),
            1 => Some(TapbackType::Like),
            2 => Some(TapbackType::Dislike),
            3 => Some(TapbackType::Laugh),
            4 => Some(TapbackType::Emphasis),
            5 => Some(TapbackType::Question),
            _ => None,
        }
    }

    pub fn to_emoji(&self) -> &'static str {
        match self {
            TapbackType::Love => "❤️",
            TapbackType::Like => "👍",
            TapbackType::Dislike => "👎",
            TapbackType::Laugh => "😂",
            TapbackType::Emphasis => "‼️",
            TapbackType::Question => "❓",
        }
    }
}

/// BlueBubbles chat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesChat {
    /// Chat GUID
    pub guid: String,
    /// Chat display name
    pub display_name: Option<String>,
    /// Participants
    pub participants: Vec<String>,
    /// Whether it's a group chat
    pub is_group: bool,
    /// Last message text
    pub last_message_text: Option<String>,
    /// Last message date
    pub last_message_date: Option<i64>,
}

/// BlueBubbles handle (contact)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesHandle {
    /// Handle address (phone or email)
    pub address: String,
    /// Service (iMessage or SMS)
    pub service: String,
    /// Country code
    pub country: Option<String>,
}

/// WebSocket event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BlueBubblesEvent {
    #[serde(rename = "new-message")]
    NewMessage { message: BlueBubblesMessage },
    #[serde(rename = "message-updated")]
    MessageUpdated { message: BlueBubblesMessage },
    #[serde(rename = "message-deleted")]
    MessageDeleted { guid: String },
    #[serde(rename = "participant-removed")]
    ParticipantRemoved { chat_guid: String, handle: String },
    #[serde(rename = "participant-added")]
    ParticipantAdded { chat_guid: String, handle: String },
    #[serde(rename = "chat-read-status-changed")]
    ChatReadStatusChanged { chat_guid: String, is_read: bool },
    #[serde(rename = "typing-indicator")]
    TypingIndicator { chat_guid: String, display_name: String, is_typing: bool },
    #[serde(rename = "connected")]
    Connected,
    #[serde(rename = "disconnected")]
    Disconnected,
    #[serde(rename = "error")]
    Error { message: String },
}

/// Send message request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    /// Chat GUID or array of handles for new chat
    pub chat_guid: String,
    /// Message text
    pub message: String,
    /// Temp GUID for deduplication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_guid: Option<String>,
    /// Method (apple-script or private-api)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// Subject (for MMS)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Effect (bubble or screen)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect_id: Option<String>,
    /// Selected message GUID (for replies)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_message_guid: Option<String>,
    /// Part index (for replies)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_index: Option<i32>,
}

/// Send tapback request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTapbackRequest {
    /// Chat GUID
    pub chat_guid: String,
    /// Message GUID to react to
    pub message_guid: String,
    /// Tapback type (0-5)
    pub tapback: i32,
}

/// Typing indicator request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypingIndicatorRequest {
    /// Chat GUID
    pub chat_guid: String,
    /// Whether typing is active
    pub typing: bool,
}

/// API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub status: String,
    pub message: Option<String>,
    pub data: Option<T>,
}

/// BlueBubbles client
pub struct BlueBubblesClient {
    config: BlueBubblesConfig,
    http_client: reqwest::Client,
    state: Arc<RwLock<ConnectionState>>,
    ws_stream: Arc<Mutex<Option<WebSocketStream<TcpStream>>>>,
    event_tx: Arc<Mutex<Option<mpsc::UnboundedSender<BlueBubblesEvent>>>>,
    reconnect_attempts: Arc<RwLock<u32>>,
    shutdown_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
}

impl BlueBubblesClient {
    /// Create a new BlueBubbles client
    pub fn new(config: BlueBubblesConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .build()
            .unwrap_or_default();

        Self {
            config,
            http_client,
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            ws_stream: Arc::new(Mutex::new(None)),
            event_tx: Arc::new(Mutex::new(None)),
            reconnect_attempts: Arc::new(RwLock::new(0)),
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// Create from environment variables
    pub fn from_env() -> Self {
        Self::new(BlueBubblesConfig::from_env())
    }

    /// Get current connection state
    pub async fn connection_state(&self) -> ConnectionState {
        self.state.read().await.clone()
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        *self.state.read().await == ConnectionState::Connected
    }

    /// Connect to BlueBubbles server (WebSocket)
    pub async fn connect(&self) -> Result<mpsc::UnboundedReceiver<BlueBubblesEvent>> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        *self.event_tx.lock().await = Some(event_tx);

        // Start WebSocket connection in background
        self.start_websocket_loop().await?;

        Ok(event_rx)
    }

    /// Disconnect from server
    pub async fn disconnect(&self) -> Result<()> {
        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(()).await;
        }

        // Close WebSocket
        let mut ws = self.ws_stream.lock().await;
        if let Some(ref mut stream) = *ws {
            let _ = stream.close(Some(CloseFrame {
                code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal,
                reason: std::borrow::Cow::Borrowed("Client disconnect"),
            })).await;
        }
        *ws = None;

        *self.state.write().await = ConnectionState::Disconnected;
        info!("BlueBubbles client disconnected");

        Ok(())
    }

    /// Start WebSocket connection loop with auto-reconnect
    async fn start_websocket_loop(&self) -> Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        let config = self.config.clone();
        let state = self.state.clone();
        let ws_stream = self.ws_stream.clone();
        let event_tx = self.event_tx.clone();
        let reconnect_attempts = self.reconnect_attempts.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        debug!("WebSocket loop received shutdown signal");
                        break;
                    }
                    result = Self::websocket_connect_loop(
                        &config,
                        &state,
                        &ws_stream,
                        &event_tx,
                        &reconnect_attempts,
                    ) => {
                        if result.is_err() {
                            error!("WebSocket connection loop error");
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// WebSocket connection loop
    async fn websocket_connect_loop(
        config: &BlueBubblesConfig,
        state: &Arc<RwLock<ConnectionState>>,
        ws_stream: &Arc<Mutex<Option<WebSocketStream<TcpStream>>>>,
        event_tx: &Arc<Mutex<Option<mpsc::UnboundedSender<BlueBubblesEvent>>>>,
        reconnect_attempts: &Arc<RwLock<u32>>,
    ) -> Result<()> {
        loop {
            *state.write().await = ConnectionState::Connecting;
            info!("Connecting to BlueBubbles WebSocket: {}", config.ws_url());

            match connect_async(&config.ws_url()).await {
                Ok((stream, _)) => {
                    info!("BlueBubbles WebSocket connected");
                    *state.write().await = ConnectionState::Connected;
                    *reconnect_attempts.write().await = 0;

                    // Store stream
                    *ws_stream.lock().await = Some(stream);

                    // Send connected event
                    if let Some(ref tx) = *event_tx.lock().await {
                        let _ = tx.send(BlueBubblesEvent::Connected);
                    }

                    // Handle messages
                    Self::handle_websocket_messages(ws_stream, event_tx, state).await;
                }
                Err(e) => {
                    error!("Failed to connect to BlueBubbles WebSocket: {}", e);
                    *state.write().await = ConnectionState::Reconnecting;
                }
            }

            // Check max reconnect attempts
            let attempts = *reconnect_attempts.read().await;
            if config.base.max_reconnect_attempts > 0 && attempts >= config.base.max_reconnect_attempts {
                error!("Max reconnect attempts reached");
                *state.write().await = ConnectionState::Disconnected;
                break;
            }

            *reconnect_attempts.write().await += 1;

            // Wait before reconnecting
            warn!(
                "Reconnecting to BlueBubbles in {} seconds (attempt {})",
                config.reconnect_interval_secs,
                attempts + 1
            );
            sleep(Duration::from_secs(config.reconnect_interval_secs)).await;
        }

        Ok(())
    }

    /// Handle WebSocket messages
    async fn handle_websocket_messages(
        ws_stream: &Arc<Mutex<Option<WebSocketStream<TcpStream>>>>,
        event_tx: &Arc<Mutex<Option<mpsc::UnboundedSender<BlueBubblesEvent>>>>,
        state: &Arc<RwLock<ConnectionState>>,
    ) {
        let mut ws = ws_stream.lock().await;
        if let Some(ref mut stream) = *ws {
            while let Some(msg) = stream.next().await {
                match msg {
                    Ok(WsMessage::Text(text)) => {
                        debug!("Received WebSocket message: {}", text);
                        match serde_json::from_str::<BlueBubblesEvent>(&text) {
                            Ok(event) => {
                                if let Some(ref tx) = *event_tx.lock().await {
                                    if let Err(e) = tx.send(event) {
                                        error!("Failed to send event: {}", e);
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                debug!("Failed to parse event: {}", e);
                            }
                        }
                    }
                    Ok(WsMessage::Close(_)) => {
                        info!("WebSocket connection closed by server");
                        break;
                    }
                    Ok(WsMessage::Ping(data)) => {
                        // Send pong
                        let _ = stream.send(WsMessage::Pong(data)).await;
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        }

        *state.write().await = ConnectionState::Disconnected;
    }

    /// Send text message
    pub async fn send_message(&self, chat_guid: &str, message: &str) -> Result<BlueBubblesMessage> {
        let url = format!("{}/message", self.config.base_url());

        let request = SendMessageRequest {
            chat_guid: chat_guid.to_string(),
            message: message.to_string(),
            temp_guid: Some(Uuid::new_v4().to_string()),
            method: Some("apple-script".to_string()),
            subject: None,
            effect_id: None,
            selected_message_guid: None,
            part_index: None,
        };

        let response = self
            .http_client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send message: {}", e)))?;

        let api_response: ApiResponse<BlueBubblesMessage> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        match api_response.data {
            Some(message) => Ok(message),
            None => Err(AgentError::platform(
                api_response.message.unwrap_or_else(|| "Unknown error".to_string()),
            )
            .into()),
        }
    }

    /// Send message with attachment
    pub async fn send_message_with_attachment(
        &self,
        chat_guid: &str,
        message: &str,
        attachment_path: &str,
    ) -> Result<BlueBubblesMessage> {
        let url = format!("{}/message/attachment", self.config.base_url());

        // Read file
        let file_data = tokio::fs::read(attachment_path).await.map_err(|e| {
            AgentError::platform(format!("Failed to read attachment: {}", e))
        })?;

        let filename = std::path::Path::new(attachment_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("attachment");

        // Build multipart form
        let form = reqwest::multipart::Form::new()
            .text("chatGuid", chat_guid.to_string())
            .text("message", message.to_string())
            .part(
                "attachment",
                reqwest::multipart::Part::bytes(file_data)
                    .file_name(filename.to_string()),
            );

        let response = self
            .http_client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send attachment: {}", e)))?;

        let api_response: ApiResponse<BlueBubblesMessage> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        match api_response.data {
            Some(message) => Ok(message),
            None => Err(AgentError::platform(
                api_response.message.unwrap_or_else(|| "Unknown error".to_string()),
            )
            .into()),
        }
    }

    /// Send tapback (reaction)
    pub async fn send_tapback(
        &self,
        chat_guid: &str,
        message_guid: &str,
        tapback_type: TapbackType,
    ) -> Result<()> {
        let url = format!("{}/message/{}/tapback", self.config.base_url(), message_guid);

        let request = SendTapbackRequest {
            chat_guid: chat_guid.to_string(),
            message_guid: message_guid.to_string(),
            tapback: tapback_type as i32,
        };

        let response = self
            .http_client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send tapback: {}", e)))?;

        if response.status().is_success() {
            Ok(())
        } else {
            let text = response.text().await.unwrap_or_default();
            Err(AgentError::platform(format!("Tapback failed: {}", text)).into())
        }
    }

    /// Send typing indicator
    pub async fn send_typing_indicator(&self, chat_guid: &str, typing: bool) -> Result<()> {
        let url = format!("{}/chat/{}/typing", self.config.base_url(), chat_guid);

        let request = TypingIndicatorRequest {
            chat_guid: chat_guid.to_string(),
            typing,
        };

        let response = self
            .http_client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send typing indicator: {}", e)))?;

        if response.status().is_success() {
            Ok(())
        } else {
            let text = response.text().await.unwrap_or_default();
            Err(AgentError::platform(format!("Typing indicator failed: {}", text)).into())
        }
    }

    /// Get chats
    pub async fn get_chats(&self, limit: Option<i32>, offset: Option<i32>) -> Result<Vec<BlueBubblesChat>> {
        let mut url = format!("{}/chat", self.config.base_url());
        
        let mut params = Vec::new();
        if let Some(limit) = limit {
            params.push(format!("limit={}", limit));
        }
        if let Some(offset) = offset {
            params.push(format!("offset={}", offset));
        }
        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get chats: {}", e)))?;

        let api_response: ApiResponse<Vec<BlueBubblesChat>> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        Ok(api_response.data.unwrap_or_default())
    }

    /// Get messages for a chat
    pub async fn get_messages(
        &self,
        chat_guid: &str,
        limit: Option<i32>,
        offset: Option<i32>,
        after: Option<i64>,
        before: Option<i64>,
    ) -> Result<Vec<BlueBubblesMessage>> {
        let mut url = format!("{}/chat/{}/message", self.config.base_url(), chat_guid);
        
        let mut params = Vec::new();
        if let Some(limit) = limit {
            params.push(format!("limit={}", limit));
        }
        if let Some(offset) = offset {
            params.push(format!("offset={}", offset));
        }
        if let Some(after) = after {
            params.push(format!("after={}", after));
        }
        if let Some(before) = before {
            params.push(format!("before={}", before));
        }
        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get messages: {}", e)))?;

        let api_response: ApiResponse<Vec<BlueBubblesMessage>> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        Ok(api_response.data.unwrap_or_default())
    }

    /// Download attachment
    pub async fn download_attachment(&self, attachment: &BlueBubblesAttachment, dest_path: &str) -> Result<()> {
        let file_path = attachment
            .file_path
            .as_ref()
            .ok_or_else(|| AgentError::platform("Attachment has no file path"))?;

        let url = format!("{}/attachment/{}", self.config.base_url(), file_path);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to download attachment: {}", e)))?;

        if response.status().is_success() {
            let bytes = response.bytes().await.map_err(|e| {
                AgentError::platform(format!("Failed to read attachment bytes: {}", e))
            })?;

            tokio::fs::write(dest_path, bytes).await.map_err(|e| {
                AgentError::platform(format!("Failed to write attachment: {}", e))
            })?;

            Ok(())
        } else {
            Err(AgentError::platform(format!(
                "Download failed with status: {}",
                response.status()
            ))
            .into())
        }
    }

    /// Get server info
    pub async fn get_server_info(&self) -> Result<serde_json::Value> {
        let url = format!("{}/server/info", self.config.base_url());

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get server info: {}", e)))?;

        let api_response: ApiResponse<serde_json::Value> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        Ok(api_response.data.unwrap_or_default())
    }

    /// Check server health
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.config.base_url());

        match self.http_client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}

impl Clone for BlueBubblesClient {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            http_client: self.http_client.clone(),
            state: self.state.clone(),
            ws_stream: Arc::new(Mutex::new(None)),
            event_tx: Arc::new(Mutex::new(None)),
            reconnect_attempts: Arc::new(RwLock::new(0)),
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bluebubbles_config_default() {
        let config = BlueBubblesConfig::default();
        assert_eq!(config.host, "mac-mini.local");
        assert_eq!(config.port, 12345);
        assert_eq!(config.ws_path, "/ws");
        assert_eq!(config.api_prefix, "/api/v1");
        assert!(!config.use_tls);
        assert_eq!(config.base.max_reconnect_attempts, 10);
    }

    #[test]
    fn test_bluebubbles_config_urls() {
        let config = BlueBubblesConfig::default();
        assert_eq!(config.base_url(), "http://mac-mini.local:12345");
        assert_eq!(config.ws_url(), "ws://mac-mini.local:12345/ws");

        let config_tls = BlueBubblesConfig {
            use_tls: true,
            ..Default::default()
        };
        assert_eq!(config_tls.base_url(), "https://mac-mini.local:12345");
        assert_eq!(config_tls.ws_url(), "wss://mac-mini.local:12345/ws");
    }

    #[test]
    fn test_tapback_type() {
        assert_eq!(TapbackType::Love.to_emoji(), "❤️");
        assert_eq!(TapbackType::Like.to_emoji(), "👍");
        assert_eq!(TapbackType::Dislike.to_emoji(), "👎");
        assert_eq!(TapbackType::Laugh.to_emoji(), "😂");
        assert_eq!(TapbackType::Emphasis.to_emoji(), "‼️");
        assert_eq!(TapbackType::Question.to_emoji(), "❓");

        assert_eq!(TapbackType::from_i32(0), Some(TapbackType::Love));
        assert_eq!(TapbackType::from_i32(1), Some(TapbackType::Like));
        assert_eq!(TapbackType::from_i32(5), Some(TapbackType::Question));
        assert_eq!(TapbackType::from_i32(99), None);
    }

    #[test]
    fn test_connection_state() {
        let state = ConnectionState::Disconnected;
        assert_ne!(state, ConnectionState::Connected);
    }
}
