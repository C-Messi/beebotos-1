//! MCP HTTP Transport
//!
//! Communicates with an MCP server over HTTP.
//! Supports two modes:
//! - Simple HTTP: POST request → HTTP response (JSON-RPC)
//! - SSE: HTTP POST for requests, SSE stream for server-pushed responses
//!
//! Follows OpenClaw HTTP/SSE transport rules.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::Mutex;

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
    pub headers: HashMap<String, String>,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Whether to use SSE for receiving responses
    pub use_sse: bool,
}

impl std::fmt::Debug for HttpTransportConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTransportConfig")
            .field("base_url", &self.base_url)
            .field(
                "auth_token",
                &self.auth_token.as_ref().map(|_| "[REDACTED]"),
            )
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
            timeout_ms: 60000, // 🆕 FIX: Increased from 30s to 60s — matches ClientConfig default
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
}

impl HttpTransport {
    /// Create a new HTTP transport.
    pub fn new(config: HttpTransportConfig) -> Result<Self, MCPError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| {
                MCPError::ConnectionFailed(format!("Failed to build HTTP client: {}", e))
            })?;

        Ok(Self {
            config,
            client,
            connected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            sse_response_tx: Arc::new(Mutex::new(None)),
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
            if let Ok(value) =
                reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token.expose_secret()))
            {
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
    pub async fn start_sse_listener(
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
            loop {
                if !connected.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                match client.get(&sse_url).headers(headers.clone()).send().await {
                    Ok(resp) => {
                        let mut stream = resp.bytes_stream();
                        while let Some(chunk_result) = stream.next().await {
                            match chunk_result {
                                Ok(chunk) => {
                                    if let Ok(text) = String::from_utf8(chunk.to_vec()) {
                                        Self::parse_sse_events(&text, &response_tx);
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

    /// Parse SSE event text and extract JSON-RPC responses.
    fn parse_sse_events(text: &str, tx: &tokio::sync::mpsc::UnboundedSender<JsonRpcResponse>) {
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("data:") {
                let data = line[5..].trim();
                if data.is_empty() {
                    continue;
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
}

#[async_trait]
impl Transport for HttpTransport {
    async fn send(&self, request: JsonRpcRequest) -> Result<(), MCPError> {
        if !self.is_connected() {
            return Err(MCPError::ConnectionFailed(
                "HTTP transport not connected".to_string(),
            ));
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
            .map_err(|e| MCPError::ConnectionFailed(format!("HTTP POST failed: {}", e)))?;

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

        // If not using SSE, read the response directly
        if !self.config.use_sse {
            let body = response
                .text()
                .await
                .map_err(|e| MCPError::ConnectionFailed(format!("HTTP body read failed: {}", e)))?;

            let rpc_response: JsonRpcResponse = serde_json::from_str(&body).map_err(|e| {
                MCPError::SerializationFailed(format!("Invalid JSON-RPC response: {}", e))
            })?;

            // Forward to the response channel
            if let Some(ref tx) = *self.sse_response_tx.lock().await {
                let _ = tx.send(rpc_response);
            }
        }

        Ok(())
    }

    async fn receive(&self) -> Result<JsonRpcResponse, MCPError> {
        // In non-SSE mode, receive() is not used (response comes back via HTTP POST
        // response) In SSE mode, this would read from a channel populated by
        // the SSE listener For simplicity, we return an error here and expect
        // non-SSE mode
        if self.config.use_sse {
            return Err(MCPError::RequestFailed(
                "SSE mode: use start_sse_listener() instead of receive()".to_string(),
            ));
        }
        Err(MCPError::ConnectionFailed(
            "HTTP transport does not support blocking receive in non-SSE mode".to_string(),
        ))
    }

    async fn close(&self) -> Result<(), MCPError> {
        self.connected
            .store(false, std::sync::atomic::Ordering::SeqCst);
        *self.sse_response_tx.lock().await = None;
        tracing::info!("MCP HTTP transport closed");
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::SeqCst)
    }
}

// Import futures::StreamExt for bytes_stream()
use futures::StreamExt;
