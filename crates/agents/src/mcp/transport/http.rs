//! MCP HTTP Transport
//!
//! Communicates with an MCP server over HTTP.
//! Supports two modes:
//! - Simple HTTP: POST request → HTTP response (JSON-RPC)
//! - SSE: HTTP POST for requests, SSE stream for server-pushed responses
//!
//! Follows OpenClaw HTTP/SSE transport rules.

use async_trait::async_trait;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

use secrecy::{ExposeSecret, SecretString};

use super::Transport;
use crate::mcp::types::{JsonRpcRequest, JsonRpcResponse};
use crate::mcp::MCPError;

/// Configuration for HTTP transport
#[derive(Clone)]
pub struct HttpTransportConfig {
    /// Base URL of the MCP server (e.g., "https://api.github.com/mcp")
    pub base_url: String,
    /// Optional Bearer token for authentication
    pub auth_token: Option<SecretString>,
    /// Additional HTTP headers
    pub timeout_ms: u64,
    /// Whether to use SSE for receiving responses
    pub use_sse: bool,
}

impl std::fmt::Debug for HttpTransportConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTransportConfig")
            .field("base_url", &self.base_url)
            .field("headers", &self.headers)
            .field("timeout_ms", &self.timeout_ms)
            .field("use_sse", &self.use_sse)
            .finish()
    }
}

impl Default for HttpTransportConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            auth_token: None,
            headers: HashMap::new(),
            timeout_ms: 3,
            use_sse: false,
        }
    }
}

/// HTTP transport implementation
pub struct HttpTransport {
    config: HttpTransportConfig,
    client: reqwest::Client,
    connected: Arc<std::sync::atomic::AtomicBool>,
    // For SSE mode: a background task reads the SSE stream and pushes responses
    sse_response_tx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<JsonRpcResponse>>>>,
    // For non-SSE mode: responses from HTTP POST are queued here
    response_queue: Arc<Mutex<VecDeque<JsonRpcResponse>>>,
}

impl HttpTransport {
    /// Create a new HTTP transport.
    pub fn new(config: HttpTransportConfig) -> Result<Self, MCPError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| MCPError::ConnectionFailed(format!("Failed to build HTTP client: {}", e)))?;

        Ok(Self {
            config,
            client,
            connected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            sse_response_tx: Arc::new(Mutex::new(None)),
            response_queue: Arc::new(Mutex::new(VecDeque::new())),
        })
    }

    /// Build request headers.
    fn build_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        if let Some(ref token) = self.config.auth_token {
            if let Ok(value) = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token.expose_secret())) {
                headers.insert(reqwest::header::AUTHORIZATION, value);
            }
        }

        for (key, value) in &self.config.headers {
            if let (Ok(name), Ok(val)) = (
                reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                reqwest::header::HeaderValue::from_str(value),
            ) {
                headers.insert(name, val);
            }
        }

        headers
    }

    /// Connect (for HTTP this is a no-op; connection is per-request).
    pub async fn connect(&self) -> Result<(), MCPError> {
        self.connected
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    /// Start SSE listener (for SSE mode).
    pub async fn start_sse(
        &self,
        response_tx: tokio::sync::mpsc::UnboundedSender<JsonRpcResponse>,
    ) -> Result<(), MCPError> {
        if !self.config.use_sse {
            return Ok(());
        }

        *self.sse_response_tx.lock().await = Some(response_tx.clone());

        let client = self.client.clone();
        let base_url = self.config.base_url.clone();
        let headers = self.build_headers();
        let connected = self.connected.clone();

        tokio::spawn(async move {
            let sse_url = format!("{}/events", base_url.trim_end_matches('/'));
            // Buffer for incomplete SSE events across chunk boundaries
            let mut line_buffer = String::new();

            loop {
                if !connected.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                match client
                    .get(&sse_url)
                    .headers(headers.clone())
                    .send()
                    .await
                {
                    Ok(resp) => {
                        let mut stream = resp.bytes_stream();
                        while let Some(chunk_result) = stream.next().await {
                            match chunk_result {
                                Ok(chunk) => {
                                    if let Ok(text) = std::str::from_utf8(&chunk) {
                                        line_buffer.push_str(text);
                                        // Extract complete lines, keep remainder in buffer
                                        let mut last_newline = 0;
                                        for (i, ch) in line_buffer.char_indices() {
                                            if ch == '\n' {
                                                let line = &line_buffer[last_newline..i];
                                                Self::parse_sse_line(line.trim_end_matches('\r'), &response_tx);
                                                last_newline = i + ch.len_utf8();
                                            }
                                        }
                                        if last_newline > 0 {
                                            line_buffer.drain(..last_newline);
                                        }
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("MCP SSE connection error: {}, retrying in 5s", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }
            }
            tracing::info!("MCP SSE listener stopped");
        });

        Ok(())
    }

    /// Parse a single SSE line and extract JSON-RPC responses.
    fn parse_sse_line(
        line: &str,
        tx: &tokio::sync::mpsc::UnboundedSender<JsonRpcResponse>,
    ) {
        if line.starts_with("data:") {
            let data = line[5..].trim();
            if data.is_empty() {
                return;
            }
            match serde_json::from_str::<JsonRpcResponse>(data) {
                Ok(response) => {
                    let _ = tx.send(response);
                }
                Err(e) => {
                    tracing::debug!("MCP SSE non-JSON data: {} (err: {})", data, e);
                }
            }
        }
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn send(&self, request: JsonRpcRequest) -> Result<(), MCPError> {
        if !self.is_connected() {
            return Err(MCPError::ConnectionFailed(
                "HTTP transport not connected".to_string(),
            ));
        }
                for id in stale_connections {
                    warn!(connection_id = %id, "Removing stale connection");

                    // Try to close gracefully
                    let connections_guard = connections.read().await;
                    if let Some(tx) = connections_guard.get(&id) {
                        let _ = tx.send(InternalMessage::Close {
                            code: 1001,
                            reason: "Connection timeout".to_string(),
                        });
                    }
                    drop(connections_guard);

                    // Remove from state
                    let mut connections_guard = connections.write().await;
                    connections_guard.remove(&id);
                    drop(connections_guard);

                    let mut states_guard = states.write().await;
                    states_guard.remove(&id);
                }
        let url = format!("{}/rpc", self.config.base_url.trim_end_matches('/'));
        let headers = self.build_headers();

        let json_body = serde_json::to_string(&request)
            .map_err(|e| MCPError::SerializationFailed(e.to_string()))?;

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .body(json_body)
            .send()
            .await
            .map_err(|e| {
                MCPError::ConnectionFailed(format!("HTTP POST failed: {}", e))
            })?;

        if !response.status().is_success() {
            return Err(MCPError::RequestFailed(format!(
                "HTTP error: {} {}",
                response.status(),
                response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string())
            )));
        }

        // If not using SSE, read the response directly and queue it
        if !self.config.use_sse {
            let body = response
                .text()
                .await
                .map_err(|e| MCPError::ConnectionFailed(format!("HTTP body read failed: {}", e)))?;

            let rpc_response: JsonRpcResponse = serde_json::from_str(&body)
                .map_err(|e| MCPError::SerializationFailed(format!("Invalid JSON-RPC response: {}", e)))?;

            // Queue the response for receive() to pick up
            let mut queue = self.response_queue.lock().await;
            queue.push_back(rpc_response);
        }

        Ok(())
    }

    async fn receive(&self) -> Result<JsonRpcResponse, MCPError> {
        if self.config.use_sse {
            return Err(MCPError::RequestFailed(
                "SSE mode: use start_sse_listener() instead of receive()".to_string(),
            ));
        }

        // Non-SSE mode: wait for a response to appear in the queue
        let deadline = tokio::time::Instant::now()
            + tokio::time::Duration::from_millis(self.config.timeout_ms);

        loop {
            {
                let mut queue = self.response_queue.lock().await;
                if let Some(response) = queue.pop_front() {
                    return Ok(response);
                }
            }

            if tokio::time::Instant::now() >= deadline {
                return Err(MCPError::Timeout);
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    }

    async fn close(&self) -> Result<(), MCPError> {
        self.connected
            .store(false, std::sync::atomic::Ordering::SeqCst);
        *self.sse_response_tx.lock().await = None;
        self.response_queue.lock().await.clear();
        tracing::info!("MCP HTTP transport closed");
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::SeqCst)
    }
}

// Import futures::StreamExt for bytes_stream()
use futures::StreamExt;
