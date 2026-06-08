//! WebSocket Session Management
//!
//! Manages WebSocket connections for real-time agent communication.
//! Provides connection management, session timeout handling, and message
//! broadcasting.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, info, warn};

use crate::error::{AgentError, Result};
use crate::session::key::{SessionKey, SessionType};

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WsProtocolMessage {
    /// Client authentication
    Auth { token: String },
    /// Authentication response
    AuthResponse { success: bool, message: String },
    /// Heartbeat/ping
    Ping,
    /// Heartbeat/pong response
    Pong,
    /// Text message
    Text { content: String },
    /// Binary data
    Binary { data: Vec<u8> },
    /// Session event
    SessionEvent {
        event: String,
        payload: serde_json::Value,
    },
    /// Error message
    Error { code: i32, message: String },
    /// Close connection
    Close { reason: Option<String> },
}

/// WebSocket connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Connection is being established
    Connecting,
    /// Connection is active and authenticated
    Connected,
    /// Connection is active but not yet authenticated
    Authenticating,
    /// Connection is closing
    Closing,
    /// Connection is closed
    Closed,
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionState::Connecting => write!(f, "connecting"),
            ConnectionState::Connected => write!(f, "connected"),
            ConnectionState::Authenticating => write!(f, "authenticating"),
            ConnectionState::Closing => write!(f, "closing"),
            ConnectionState::Closed => write!(f, "closed"),
        }
    }
}

/// WebSocket session information
#[derive(Debug, Clone)]
pub struct WebSocketSession {
    /// Session ID
    pub id: String,
    /// Session key
    pub session_key: Option<SessionKey>,
    /// Connection state
    pub state: ConnectionState,
    /// Client address
    pub client_addr: SocketAddr,
    /// Connection established time
    pub connected_at: Instant,
    /// Last activity time
    pub last_activity: Instant,
    /// Message channel sender
    pub message_tx: mpsc::UnboundedSender<WsProtocolMessage>,
    /// User/agent ID (set after authentication)
    pub user_id: Option<String>,
    /// Session metadata
    pub metadata: HashMap<String, String>,
}

impl WebSocketSession {
    /// Create a new WebSocket session
    pub fn new(
        id: String,
        client_addr: SocketAddr,
        message_tx: mpsc::UnboundedSender<WsProtocolMessage>,
    ) -> Self {
        let now = Instant::now();
        Self {
            id,
            session_key: None,
            state: ConnectionState::Connecting,
            client_addr,
            connected_at: now,
            last_activity: now,
            message_tx,
            user_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Check if session is active
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            ConnectionState::Connected | ConnectionState::Authenticating
        )
    }

    /// Update last activity timestamp
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Check if session has timed out
    pub fn is_timed_out(&self, timeout_duration: Duration) -> bool {
        self.last_activity.elapsed() > timeout_duration
    }

    /// Send message to the session
    pub fn send(&self, message: WsProtocolMessage) -> Result<()> {
        self.message_tx
            .send(message)
            .map_err(|_| AgentError::platform("Failed to send message to session"))
    }

    /// Set authenticated user
    pub fn set_authenticated(&mut self, user_id: String, session_key: SessionKey) {
        self.user_id = Some(user_id);
        self.session_key = Some(session_key);
        self.state = ConnectionState::Connected;
    }
}

/// WebSocket session manager configuration
#[derive(Debug, Clone)]
pub struct WsSessionManagerConfig {
    /// Bind address
    pub bind_addr: String,
    /// Connection timeout (no activity)
    pub connection_timeout: Duration,
    /// Authentication timeout
    pub auth_timeout: Duration,
    /// Heartbeat interval
    pub heartbeat_interval: Duration,
    /// Maximum connections per IP
    pub max_connections_per_ip: usize,
    /// Maximum total connections
    pub max_total_connections: usize,
    /// Enable heartbeat
    pub enable_heartbeat: bool,
}

impl Default for WsSessionManagerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:8080".to_string(),
            connection_timeout: Duration::from_secs(300), // 5 minutes
            auth_timeout: Duration::from_secs(180),       // 180 seconds
            heartbeat_interval: Duration::from_secs(180), // 180 seconds
            max_connections_per_ip: 10,
            max_total_connections: 1000,
            enable_heartbeat: true,
        }
    }
}

/// WebSocket session manager
///
/// Manages WebSocket connections, handles session lifecycle, and provides
/// message broadcasting capabilities.
pub struct WebSocketSessionManager {
    config: WsSessionManagerConfig,
    /// Active sessions by session ID
    sessions: Arc<RwLock<HashMap<String, Arc<RwLock<WebSocketSession>>>>>,
    /// Sessions by user ID (for targeted messaging)
    user_sessions: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Connection count by IP address
    ip_connections: Arc<RwLock<HashMap<std::net::IpAddr, usize>>>,
    /// Shutdown signal sender
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl WebSocketSessionManager {
    /// Create a new WebSocket session manager
    pub fn new(config: WsSessionManagerConfig) -> Self {
        Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            user_sessions: Arc::new(RwLock::new(HashMap::new())),
            ip_connections: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: None,
        }
    }

    /// Create with default configuration
    pub fn default() -> Self {
        Self::new(WsSessionManagerConfig::default())
    }

    /// Start the WebSocket server
    pub async fn start(&mut self) -> Result<()> {
        let listener = TcpListener::bind(&self.config.bind_addr)
            .await
            .map_err(|e| {
                AgentError::platform(format!(
                    "Failed to bind to {}: {}",
                    self.config.bind_addr, e
                ))
            })?;

        info!("WebSocket server listening on {}", self.config.bind_addr);

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        let sessions = self.sessions.clone();
        let user_sessions = self.user_sessions.clone();
        let ip_connections = self.ip_connections.clone();
        let config = self.config.clone();
        let cleanup_interval = self.config.heartbeat_interval;

        // Spawn connection handler
        let handle_connections = async move {
            loop {
                tokio::select! {
                    Ok((stream, addr)) = listener.accept() => {
                        let sessions = sessions.clone();
                        let user_sessions = user_sessions.clone();
                        let ip_connections = ip_connections.clone();
                        let config = config.clone();

                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_connection(
                                stream,
                                addr,
                                sessions,
                                user_sessions,
                                ip_connections,
                                config,
                            ).await {
                                error!("WebSocket connection error from {}: {}", addr, e);
                            }
                        });
                    }
                    _ = shutdown_rx.recv() => {
                        info!("WebSocket server shutting down");
                        break;
                    }
                }
            }
        };

        // Spawn session cleanup task
        let sessions = self.sessions.clone();
        let cleanup_sessions = async move {
            let mut interval = tokio::time::interval(cleanup_interval);
            loop {
                interval.tick().await;

                let mut sessions_guard = sessions.write().await;
                let now = Instant::now();
                let timeout = Duration::from_secs(3600); // 1 hour default

                let to_remove: Vec<String> = sessions_guard
                    .iter()
                    .filter(|(_, s)| {
                        // Check if session has timed out by checking last_activity
                        // Note: This is a synchronous check, but should be fast
                        let last_activity = s
                            .try_read()
                            .map(|sess| sess.last_activity)
                            .unwrap_or_else(|_| now.clone());
                        last_activity.elapsed() > timeout
                    })
                    .map(|(k, _)| k.clone())
                    .collect();

                for key in to_remove {
                    sessions_guard.remove(&key);
                }
            }
        };

        // Run both tasks
        tokio::spawn(async move {
            tokio::join!(handle_connections, cleanup_sessions);
        });

        Ok(())
    }

    /// Stop the WebSocket server
    pub async fn stop(&self) -> Result<()> {
        if let Some(tx) = &self.shutdown_tx {
            tx.send(()).await.map_err(|e| {
                AgentError::platform(format!("Failed to send shutdown signal: {}", e))
            })?;
        }

        // Close all sessions
        let sessions = self.sessions.read().await;
        for (session_id, session) in sessions.iter() {
            let mut session = session.write().await;
            session.state = ConnectionState::Closing;
            let _ = session.send(WsProtocolMessage::Close {
                reason: Some("Server shutting down".to_string()),
            });
            info!("Closing session: {}", session_id);
        }

        Ok(())
    }

    /// Handle a new WebSocket connection
    async fn handle_connection(
        stream: TcpStream,
        addr: SocketAddr,
        sessions: Arc<RwLock<HashMap<String, Arc<RwLock<WebSocketSession>>>>>,
        user_sessions: Arc<RwLock<HashMap<String, Vec<String>>>>,
        ip_connections: Arc<RwLock<HashMap<std::net::IpAddr, usize>>>,
        config: WsSessionManagerConfig,
    ) -> Result<()> {
        // Check connection limits
        {
            let ip_counts = ip_connections.read().await;
            let ip_count = ip_counts.get(&addr.ip()).copied().unwrap_or(0);
            if ip_count >= config.max_connections_per_ip {
                warn!("Connection limit reached for IP: {}", addr.ip());
                return Err(AgentError::platform("Connection limit reached for IP"));
            }
        }

        let total_sessions = sessions.read().await.len();
        if total_sessions >= config.max_total_connections {
            warn!("Maximum connections reached: {}", total_sessions);
            return Err(AgentError::platform("Maximum connections reached"));
        }

        // Accept WebSocket connection
        let ws_stream = accept_async(stream)
            .await
            .map_err(|e| AgentError::platform(format!("WebSocket handshake failed: {}", e)))?;

        let session_id = uuid::Uuid::new_v4().to_string();
        info!("New WebSocket connection: {} from {}", session_id, addr);

        // Create message channel
        let (message_tx, mut message_rx) = mpsc::unbounded_channel();

        // Create session
        let session = Arc::new(RwLock::new(WebSocketSession::new(
            session_id.clone(),
            addr,
            message_tx,
        )));

        // Register session
        {
            let mut sessions_guard = sessions.write().await;
            sessions_guard.insert(session_id.clone(), session.clone());
        }

        // Update IP connection count
        {
            let mut ip_counts = ip_connections.write().await;
            *ip_counts.entry(addr.ip()).or_insert(0) += 1;
        }

        // Split WebSocket stream
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // Set authentication timeout
        let auth_timeout = config.auth_timeout;
        let session_clone = session.clone();

        // Handle messages
        let session_id_for_handler = session_id.clone();
        let handle_messages = async move {
            let mut authenticated = false;
            let mut last_ping = Instant::now();

            loop {
                tokio::select! {
                    // Receive from WebSocket
                    Some(msg) = ws_receiver.next() => {
                        match msg {
                            Ok(WsMessage::Text(text)) => {
                                match serde_json::from_str::<WsProtocolMessage>(&text) {
                                    Ok(protocol_msg) => {
                                        let mut session = session_clone.write().await;
                                        session.touch();

                                        match protocol_msg {
                                            WsProtocolMessage::Ping => {
                                                let _ = session.send(WsProtocolMessage::Pong);
                                            }
                                            WsProtocolMessage::Auth { token: _ } => {
                                                // TODO: Implement proper authentication
                                                authenticated = true;
                                                let user_id = format!("user_{}", session_id_for_handler.clone());
                                                let session_key = SessionKey::new(
                                                    &user_id,
                                                    SessionType::WebSocket,
                                                );
                                                session.set_authenticated(user_id.clone(), session_key);
                                                let _ = session.send(WsProtocolMessage::AuthResponse {
                                                    success: true,
                                                    message: "Authenticated".to_string(),
                                                });

                                                // Register user session
                                                let mut user_sessions_guard = user_sessions.write().await;
                                                user_sessions_guard
                                                    .entry(user_id)
                                                    .or_insert_with(Vec::new)
                                                    .push(session_id_for_handler.clone());
                                            }
                                            WsProtocolMessage::Close { reason } => {
                                                info!("Client requested close: {:?}", reason);
                                                break;
                                            }
                                            _ => {
                                                // Handle other messages
                                                debug!("Received message: {:?}", protocol_msg);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to parse message: {}", e);
                                        let session = session_clone.read().await;
                                        let _ = session.send(WsProtocolMessage::Error {
                                            code: 400,
                                            message: "Invalid message format".to_string(),
                                        });
                                    }
                                }
                            }
                            Ok(WsMessage::Binary(data)) => {
                                let mut session = session_clone.write().await;
                                session.touch();
                                debug!("Received binary data: {} bytes", data.len());
                            }
                            Ok(WsMessage::Ping(data)) => {
                                let _ = ws_sender.send(WsMessage::Pong(data)).await;
                            }
                            Ok(WsMessage::Close(_)) => {
                                info!("WebSocket closed by client");
                                break;
                            }
                            Err(e) => {
                                error!("WebSocket error: {}", e);
                                break;
                            }
                            _ => {}
                        }
                    }
                    // Send to WebSocket
                    Some(msg) = message_rx.recv() => {
                        let ws_msg = match msg {
                            WsProtocolMessage::Text { content } => WsMessage::Text(content),
                            WsProtocolMessage::Binary { data } => WsMessage::Binary(data),
                            WsProtocolMessage::Close { reason } => {
                                let close_frame = reason.map(|r| tokio_tungstenite::tungstenite::protocol::CloseFrame {
                                    code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal,
                                    reason: r.into(),
                                });
                                WsMessage::Close(close_frame)
                            }
                            _ => WsMessage::Text(serde_json::to_string(&msg).unwrap_or_default()),
                        };

                        if let Err(e) = ws_sender.send(ws_msg).await {
                            error!("Failed to send WebSocket message: {}", e);
                            break;
                        }
                    }
                    // Heartbeat
                    _ = tokio::time::sleep(config.heartbeat_interval), if config.enable_heartbeat => {
                        if last_ping.elapsed() > config.heartbeat_interval {
                            let session = session_clone.read().await;
                            let _ = session.send(WsProtocolMessage::Ping);
                            last_ping = Instant::now();
                        }
                    }
                }

                // Check authentication timeout
                if !authenticated {
                    let session = session_clone.read().await;
                    if session.connected_at.elapsed() > auth_timeout {
                        warn!(
                            "Authentication timeout for session: {}",
                            session_id_for_handler.clone()
                        );
                        break;
                    }
                }
            }
        };

        // Run message handler
        handle_messages.await;

        // Cleanup
        {
            let mut sessions_guard = sessions.write().await;
            sessions_guard.remove(&session_id);
        }

        {
            let mut ip_counts = ip_connections.write().await;
            if let Some(count) = ip_counts.get_mut(&addr.ip()) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    ip_counts.remove(&addr.ip());
                }
            }
        }

        info!("WebSocket connection closed: {}", session_id);
        Ok(())
    }

    /// Run session cleanup task
    #[allow(dead_code)]
    async fn run_session_cleanup(&self) {
        let mut interval = interval(Duration::from_secs(60));
        let sessions = self.sessions.clone();
        let config = self.config.clone();

        loop {
            interval.tick().await;

            let sessions_guard = sessions.read().await;
            let mut timed_out_sessions = Vec::new();

            for (session_id, session) in sessions_guard.iter() {
                let session = session.read().await;
                if session.is_timed_out(config.connection_timeout) {
                    timed_out_sessions.push(session_id.clone());
                }
            }

            drop(sessions_guard);

            for session_id in timed_out_sessions {
                warn!("Session timed out: {}", session_id);
                if let Some(session) = sessions.read().await.get(&session_id) {
                    let mut session = session.write().await;
                    session.state = ConnectionState::Closing;
                    let _ = session.send(WsProtocolMessage::Close {
                        reason: Some("Session timeout".to_string()),
                    });
                }
            }
        }
    }

    /// Broadcast message to all connected sessions
    pub async fn broadcast(&self, message: WsProtocolMessage) -> Result<usize> {
        let sessions = self.sessions.read().await;
        let mut sent_count = 0;

        for (session_id, session) in sessions.iter() {
            let session = session.read().await;
            if session.is_active() {
                if let Err(e) = session.send(message.clone()) {
                    warn!("Failed to send to session {}: {}", session_id, e);
                } else {
                    sent_count += 1;
                }
            }
        }

        debug!("Broadcasted message to {} sessions", sent_count);
        Ok(sent_count)
    }

    /// Send message to specific session
    pub async fn send_to_session(
        &self,
        session_id: &str,
        message: WsProtocolMessage,
    ) -> Result<()> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| AgentError::not_found(format!("Session not found: {}", session_id)))?;

        let session = session.read().await;
        session.send(message)
    }

    /// Send message to specific user (all their sessions)
    pub async fn send_to_user(&self, user_id: &str, message: WsProtocolMessage) -> Result<usize> {
        let user_sessions = self.user_sessions.read().await;
        let session_ids = user_sessions.get(user_id).cloned().unwrap_or_default();

        drop(user_sessions);

        let mut sent_count = 0;
        for session_id in session_ids {
            if self
                .send_to_session(&session_id, message.clone())
                .await
                .is_ok()
            {
                sent_count += 1;
            }
        }

        Ok(sent_count)
    }

    /// Get active session count
    pub async fn get_session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// Get session information
    pub async fn get_session(&self, session_id: &str) -> Option<Arc<RwLock<WebSocketSession>>> {
        self.sessions.read().await.get(session_id).cloned()
    }

    /// List all active sessions
    pub async fn list_sessions(&self) -> Vec<String> {
        self.sessions.read().await.keys().cloned().collect()
    }

    /// Disconnect a specific session
    pub async fn disconnect_session(&self, session_id: &str, reason: &str) -> Result<()> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| AgentError::not_found(format!("Session not found: {}", session_id)))?;

        let mut session = session.write().await;
        session.state = ConnectionState::Closing;
        session.send(WsProtocolMessage::Close {
            reason: Some(reason.to_string()),
        })
    }
}

impl Default for WebSocketSessionManager {
    fn default() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_state_display() {
        assert_eq!(format!("{}", ConnectionState::Connecting), "connecting");
        assert_eq!(format!("{}", ConnectionState::Connected), "connected");
        assert_eq!(
            format!("{}", ConnectionState::Authenticating),
            "authenticating"
        );
        assert_eq!(format!("{}", ConnectionState::Closing), "closing");
        assert_eq!(format!("{}", ConnectionState::Closed), "closed");
    }

    #[test]
    fn test_ws_protocol_message_serialization() {
        let msg = WsProtocolMessage::Text {
            content: "Hello".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Text\""));
        assert!(json.contains("\"content\":\"Hello\""));

        let msg = WsProtocolMessage::Ping;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Ping\""));
    }

    #[test]
    fn test_ws_session_manager_config_default() {
        let config = WsSessionManagerConfig::default();
        assert_eq!(config.bind_addr, "0.0.0.0:8080");
        assert_eq!(config.connection_timeout, Duration::from_secs(300));
        assert_eq!(config.auth_timeout, Duration::from_secs(180));
        assert_eq!(config.heartbeat_interval, Duration::from_secs(180));
        assert_eq!(config.max_connections_per_ip, 10);
        assert_eq!(config.max_total_connections, 1000);
        assert!(config.enable_heartbeat);
    }

    #[tokio::test]
    async fn test_websocket_session() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let addr = "127.0.0.1:8080".parse().unwrap();
        let session = WebSocketSession::new("test_id".to_string(), addr, tx);

        assert_eq!(session.id, "test_id");
        // New session starts in Connecting state, not yet active
        assert!(!session.is_active());
        assert!(session.user_id.is_none());
    }
}
