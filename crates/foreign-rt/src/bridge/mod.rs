//! Host function bridge between foreign runtimes and BeeBotOS kernel

pub mod host_funcs;

use serde::{Deserialize, Serialize};

/// Bridge call request from script to host
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeCall {
    /// API namespace
    pub namespace: String,
    /// API method
    pub method: String,
    /// Arguments (JSON)
    pub args: Vec<serde_json::Value>,
}

/// Bridge call response from host to script
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeResponse {
    /// Success flag
    pub success: bool,
    /// Result data
    pub data: Option<serde_json::Value>,
    /// Error message if failed
    pub error: Option<String>,
}

impl BridgeResponse {
    /// Create a successful response
    pub fn success(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

/// Bridge protocol version
pub const BRIDGE_PROTOCOL_VERSION: u32 = 1;
