//! MCP Transport Layer
//!
//! Provides stdio and HTTP/SSE transports for MCP client-server communication.
//! Follows OpenClaw MCP transport requirements:
//! - stdio: local subprocess via stdin/stdout (JSON-RPC lines)
//! - HTTP/SSE: remote server via HTTP POST + SSE event stream

use std::sync::Arc;

use async_trait::async_trait;

use super::types::{JsonRpcRequest, JsonRpcResponse};
use super::MCPError;

pub mod http;
pub mod stdio;

pub use http::{HttpTransport, HttpTransportConfig};
pub use stdio::{StdioTransport, StdioTransportConfig};

/// Generic transport interface for MCP communication.
///
/// Implementations handle the actual I/O (stdio pipes, HTTP, SSE, WebSocket)
/// while the MCPClient handles JSON-RPC framing and request-response matching.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send a JSON-RPC request to the MCP server.
    async fn send(&self, request: JsonRpcRequest) -> Result<(), MCPError>;

    /// Receive a JSON-RPC response from the MCP server.
    /// This is a blocking call that waits for the next response.
    async fn receive(&self) -> Result<JsonRpcResponse, MCPError>;

    /// Close the transport connection.
    async fn close(&self) -> Result<(), MCPError>;

    /// Check if the transport is still connected/healthy.
    fn is_connected(&self) -> bool;
}

/// Transport bridge that wires MCPClient channels to a Transport
/// implementation.
///
/// Spawns a background task that:
/// 1. Reads outgoing requests from `request_rx` and sends them via transport
/// 2. Reads incoming responses from transport and forwards them to
///    `response_tx`
pub struct TransportBridge {
    _handle: tokio::task::JoinHandle<()>,
}

impl TransportBridge {
    /// Start a bridge between MCPClient channels and a transport.
    pub fn spawn<T: Transport + 'static>(
        transport: Arc<T>,
        mut request_rx: tokio::sync::mpsc::UnboundedReceiver<JsonRpcRequest>,
        response_tx: tokio::sync::mpsc::UnboundedSender<JsonRpcResponse>,
    ) -> Self {
        let handle = tokio::spawn(async move {
            let transport_clone = transport.clone();

            // Task 1: Forward outgoing requests
            let request_task = tokio::spawn(async move {
                while let Some(request) = request_rx.recv().await {
                    if let Err(e) = transport.send(request).await {
                        tracing::warn!("MCP transport send error: {}", e);
                        break;
                    }
                }
                tracing::debug!("MCP request forwarder stopped");
            });

            // Task 2: Forward incoming responses
            let response_task = tokio::spawn(async move {
                loop {
                    match transport_clone.receive().await {
                        Ok(response) => {
                            if response_tx.send(response).is_err() {
                                tracing::debug!("MCP response channel closed");
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::warn!("MCP transport receive error: {}", e);
                            break;
                        }
                    }
                }
                tracing::debug!("MCP response forwarder stopped");
            });

            // Wait for either task to finish
            let _ = tokio::join!(request_task, response_task);
            tracing::info!("MCP transport bridge stopped");
        });

        Self { _handle: handle }
    }
}

/// Validate that a command path is safe (no path traversal).
pub(crate) fn validate_command_path(path: &str) -> Result<(), MCPError> {
    if path.is_empty() {
        return Err(MCPError::InvalidParams(
            "Command path cannot be empty".to_string(),
        ));
    }
    if path.contains("..") {
        return Err(MCPError::InvalidParams(format!(
            "Command path contains traversal sequences: {}",
            path
        )));
    }
    Ok(())
}

/// Validate that a command is in the allowed whitelist.
///
/// If `allowed_commands` is empty, all commands are permitted (backward
/// compatible). Otherwise, the command must match one of the allowed entries
/// exactly.
pub(crate) fn validate_command_whitelist(
    command: &str,
    allowed_commands: &[String],
) -> Result<(), MCPError> {
    if allowed_commands.is_empty() {
        return Ok(());
    }

    // Extract the base command name (strip path prefix)
    let base_name = std::path::Path::new(command)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(command);

    if allowed_commands.iter().any(|allowed| {
        // Allow exact match or base name match
        allowed == command || allowed == base_name
    }) {
        Ok(())
    } else {
        Err(MCPError::InvalidParams(format!(
            "Command '{}' is not in the allowed whitelist: {:?}",
            command, allowed_commands
        )))
    }
}
