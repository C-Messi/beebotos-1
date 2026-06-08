use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::a2a::message::A2AMessage;

#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, message: A2AMessage, endpoint: &str) -> Result<(), TransportError>;
    async fn receive(&self) -> Result<A2AMessage, TransportError>;
    async fn close(&self) -> Result<(), TransportError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportError {
    ConnectionFailed(String),
    SendFailed(String),
    ReceiveFailed(String),
    Timeout,
    AuthenticationFailed,
    EncryptionError(String),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            TransportError::SendFailed(msg) => write!(f, "Send failed: {}", msg),
            TransportError::ReceiveFailed(msg) => write!(f, "Receive failed: {}", msg),
            TransportError::Timeout => write!(f, "Timeout"),
            TransportError::AuthenticationFailed => write!(f, "Authentication failed"),
            TransportError::EncryptionError(msg) => write!(f, "Encryption error: {}", msg),
        }
    }
}

impl std::error::Error for TransportError {}

/// HTTP Transport with connection pooling
pub struct HttpTransport {
    client: reqwest::Client,
    base_url: String,
}

impl HttpTransport {
    /// Create new HTTP transport with connection pooling
    ///
    /// # Connection Pool Settings
    /// - Pool max idle per host: 10 connections
    /// - Connection timeout: 180 seconds
    /// - Pool idle timeout: 90 seconds
    pub fn new(base_url: String) -> Self {
        let client = reqwest::Client::builder()
            // Connection pool settings for high throughput
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .connect_timeout(std::time::Duration::from_secs(180))
            .build()
            .expect("Failed to create HTTP client with connection pool");

        Self { client, base_url }
    }

    /// Create with custom reqwest client (for advanced configuration)
    pub fn with_client(base_url: String, client: reqwest::Client) -> Self {
        Self { client, base_url }
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn send(&self, message: A2AMessage, endpoint: &str) -> Result<(), TransportError> {
        let url = format!("{}/{}", self.base_url, endpoint);
        let response = self
            .client
            .post(&url)
            .json(&message)
            .send()
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(TransportError::SendFailed(format!(
                "HTTP {}",
                response.status()
            )))
        }
    }

    async fn receive(&self) -> Result<A2AMessage, TransportError> {
        todo!("HTTP transport receive requires webhook setup")
    }

    async fn close(&self) -> Result<(), TransportError> {
        Ok(())
    }
}

pub struct WebSocketTransport {
    #[allow(dead_code)]
    url: String,
}

impl WebSocketTransport {
    pub fn new(url: String) -> Self {
        Self { url }
    }
}

pub struct TransportManager {
    http_transport: Option<HttpTransport>,
    retry_policy: RetryPolicy,
}

/// Retry policy for transport operations
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 100,
            max_delay_ms: 5000,
        }
    }
}

impl RetryPolicy {
    /// Calculate delay with exponential backoff
    pub fn delay_for_attempt(&self, attempt: u32) -> u64 {
        let delay = self.base_delay_ms * 2_u64.pow(attempt);
        delay.min(self.max_delay_ms)
    }
}

impl TransportManager {
    pub fn new() -> Self {
        Self {
            http_transport: None,
            retry_policy: RetryPolicy::default(),
        }
    }

    pub fn with_http_transport(mut self, base_url: String) -> Self {
        self.http_transport = Some(HttpTransport::new(base_url));
        self
    }

    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// Send message to endpoint with retry logic
    pub async fn send(
        &self,
        message: &A2AMessage,
        endpoint: &str,
    ) -> Result<A2AMessage, TransportError> {
        let transport = self.http_transport.as_ref().ok_or_else(|| {
            TransportError::ConnectionFailed("No HTTP transport configured".to_string())
        })?;

        // Serialize message
        let payload = serde_json::to_vec(message)
            .map_err(|e| TransportError::SendFailed(format!("Serialization failed: {}", e)))?;

        // Try sending with retries
        let mut last_error = None;

        for attempt in 0..=self.retry_policy.max_retries {
            match self
                .try_send_with_transport(transport, endpoint, &payload)
                .await
            {
                Ok(response) => return Ok(response),
                Err(e) => {
                    last_error = Some(e);

                    if attempt < self.retry_policy.max_retries {
                        let delay = self.retry_policy.delay_for_attempt(attempt);
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                    }
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| TransportError::SendFailed("Max retries exceeded".to_string())))
    }

    /// Single send attempt
    async fn try_send_with_transport(
        &self,
        transport: &HttpTransport,
        endpoint: &str,
        payload: &[u8],
    ) -> Result<A2AMessage, TransportError> {
        // Build full URL
        let url = if endpoint.starts_with("http") {
            endpoint.to_string()
        } else {
            format!("{}/{}", transport.base_url, endpoint)
        };

        // Send request
        let response = transport
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-A2A-Version", "1.0")
            .body(payload.to_vec())
            .timeout(tokio::time::Duration::from_secs(180))
            .send()
            .await
            .map_err(|e| TransportError::SendFailed(format!("HTTP request failed: {}", e)))?;

        // Check status
        if !response.status().is_success() {
            return Err(TransportError::SendFailed(format!(
                "HTTP {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        // Parse response
        let response_bytes = response.bytes().await.map_err(|e| {
            TransportError::ReceiveFailed(format!("Failed to read response: {}", e))
        })?;

        let response_message: A2AMessage =
            serde_json::from_slice(&response_bytes).map_err(|e| {
                TransportError::ReceiveFailed(format!("Failed to parse response: {}", e))
            })?;

        Ok(response_message)
    }

    /// Broadcast message to multiple endpoints
    pub async fn broadcast(
        &self,
        message: &A2AMessage,
        endpoints: &[String],
    ) -> Vec<Result<A2AMessage, TransportError>> {
        let mut results = Vec::new();

        for endpoint in endpoints {
            let result = self.send(message, endpoint).await;
            results.push(result);
        }

        results
    }

    /// Health check for transport
    pub async fn health_check(&self, endpoint: &str) -> Result<(), TransportError> {
        let transport = self.http_transport.as_ref().ok_or_else(|| {
            TransportError::ConnectionFailed("No HTTP transport configured".to_string())
        })?;

        let url = if endpoint.starts_with("http") {
            format!("{}/health", endpoint)
        } else {
            format!("{}/{}/health", transport.base_url, endpoint)
        };

        let response = transport
            .client
            .get(&url)
            .timeout(tokio::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(TransportError::ConnectionFailed(format!(
                "Health check failed: HTTP {}",
                response.status()
            )))
        }
    }
}

impl Default for TransportManager {
    fn default() -> Self {
        Self::new()
    }
}
