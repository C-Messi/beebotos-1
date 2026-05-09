//! MCP stdio Transport
//!
//! Communicates with an MCP server via a local subprocess.
//! Follows OpenClaw stdio transport rules:
//! - Launches command with args and env vars
//! - Sends JSON-RPC requests via stdin (newline-delimited)
//! - Receives JSON-RPC responses via stdout (newline-delimited)
//! - Validates command path to prevent path traversal

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use super::{validate_command_path, validate_command_whitelist, Transport};
use crate::mcp::types::{JsonRpcRequest, JsonRpcResponse};
use crate::mcp::MCPError;

/// Configuration for stdio transport
#[derive(Debug, Clone)]
pub struct StdioTransportConfig {
    /// Command to execute (e.g., "npx", "python", absolute path)
    pub command: String,
    /// Arguments passed to the command
    pub args: Vec<String>,
    /// Environment variables to set for the child process
    pub env: HashMap<String, String>,
    /// Optional working directory for the child process
    pub working_dir: Option<std::path::PathBuf>,
}

impl Default for StdioTransportConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            args: Vec::new(),
            env: HashMap::new(),
            working_dir: None,
        }
    }
}

/// stdio transport implementation
pub struct StdioTransport {
    config: StdioTransportConfig,
    child: Arc<Mutex<Option<Child>>>,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    stdout_reader: Arc<Mutex<Option<BufReader<ChildStdout>>>>,
    connected: Arc<std::sync::atomic::AtomicBool>,
}

impl StdioTransport {
    /// Create a new stdio transport (does not spawn the process yet).
    ///
    /// For security, use `new_with_policy` to enforce a command whitelist.
    pub fn new(config: StdioTransportConfig) -> Result<Self, MCPError> {
        Self::new_with_policy(config, &[])
    }

    /// Create a new stdio transport with a command whitelist policy.
    pub fn new_with_policy(
        config: StdioTransportConfig,
        allowed_commands: &[String],
    ) -> Result<Self, MCPError> {
        validate_command_path(&config.command)?;
        validate_command_whitelist(&config.command, allowed_commands)?;
        Ok(Self {
            config,
            child: Arc::new(Mutex::new(None)),
            stdin: Arc::new(Mutex::new(None)),
            stdout_reader: Arc::new(Mutex::new(None)),
            connected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// Spawn the child process and open stdin/stdout pipes.
    pub async fn connect(&self) -> Result<(), MCPError> {
        if self.connected.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }

        let mut cmd = Command::new(&self.config.command);
        cmd.args(&self.config.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        // Set environment variables
        for (key, value) in &self.config.env {
            cmd.env(key, value);
        }

        // Set working directory if specified
        if let Some(ref dir) = self.config.working_dir {
            cmd.current_dir(dir);
        }

        let mut child = cmd.spawn().map_err(|e| {
            MCPError::ConnectionFailed(format!(
                "Failed to spawn MCP process '{}': {}",
                self.config.command, e
            ))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| MCPError::ConnectionFailed("Failed to open stdin pipe".to_string()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| MCPError::ConnectionFailed("Failed to open stdout pipe".to_string()))?;

        *self.child.lock().await = Some(child);
        *self.stdin.lock().await = Some(stdin);
        *self.stdout_reader.lock().await = Some(BufReader::new(stdout));
        self.connected
            .store(true, std::sync::atomic::Ordering::SeqCst);

        tracing::info!(
            "MCP stdio transport connected: {} {}",
            self.config.command,
            self.config.args.join(" ")
        );
        Ok(())
    }

    /// Check if the child process is still alive.
    async fn is_child_alive(&self) -> bool {
        if let Some(ref mut child) = *self.child.lock().await {
            match child.try_wait() {
                Ok(None) => true,           // still running
                Ok(Some(_status)) => false, // exited
                Err(_) => false,
            }
        } else {
            false
        }
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn send(&self, request: JsonRpcRequest) -> Result<(), MCPError> {
        if !self.is_connected() || !self.is_child_alive().await {
            return Err(MCPError::ConnectionFailed(
                "MCP process not running".to_string(),
            ));
        }

        let json = serde_json::to_string(&request)
            .map_err(|e| MCPError::SerializationFailed(e.to_string()))?;

        let mut stdin_guard = self.stdin.lock().await;
        let stdin = stdin_guard
            .as_mut()
            .ok_or_else(|| MCPError::ConnectionFailed("Stdin not available".to_string()))?;

        stdin
            .write_all(json.as_bytes())
            .await
            .map_err(|e| MCPError::ConnectionFailed(format!("Stdin write failed: {}", e)))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| MCPError::ConnectionFailed(format!("Stdin write failed: {}", e)))?;
        stdin
            .flush()
            .await
            .map_err(|e| MCPError::ConnectionFailed(format!("Stdin flush failed: {}", e)))?;

        Ok(())
    }

    async fn receive(&self) -> Result<JsonRpcResponse, MCPError> {
        let mut reader_guard = self.stdout_reader.lock().await;
        let reader = reader_guard
            .as_mut()
            .ok_or_else(|| MCPError::ConnectionFailed("Stdout not available".to_string()))?;

        let mut line = String::new();
        let bytes_read = reader
            .read_line(&mut line)
            .await
            .map_err(|e| MCPError::ConnectionFailed(format!("Stdout read failed: {}", e)))?;

        if bytes_read == 0 {
            return Err(MCPError::ConnectionFailed(
                "MCP process closed stdout".to_string(),
            ));
        }

        let line = line.trim();
        if line.is_empty() {
            // Skip empty lines, read next
            drop(reader_guard);
            return self.receive().await;
        }

        serde_json::from_str(line)
            .map_err(|e| MCPError::SerializationFailed(format!("Invalid JSON from MCP: {}", e)))
    }

    async fn close(&self) -> Result<(), MCPError> {
        self.connected
            .store(false, std::sync::atomic::Ordering::SeqCst);

        // Close stdin first to signal EOF to the child
        {
            let mut stdin_guard = self.stdin.lock().await;
            if let Some(mut stdin) = stdin_guard.take() {
                let _ = stdin.shutdown().await;
            }
        }

        // Kill the child process
        {
            let mut child_guard = self.child.lock().await;
            if let Some(mut child) = child_guard.take() {
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
        }

        tracing::info!("MCP stdio transport closed");
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        // Best-effort cleanup in drop: mark disconnected and try to kill child
        self.connected
            .store(false, std::sync::atomic::Ordering::SeqCst);

        // Try to acquire lock and kill the child process synchronously.
        // Use try_lock to avoid blocking the dropping thread.
        if let Ok(mut child_guard) = self.child.try_lock() {
            if let Some(mut child) = child_guard.take() {
                // Attempt synchronous kill; ignore errors during drop
                let _ = child.start_kill();
            }
        }
    }
}
