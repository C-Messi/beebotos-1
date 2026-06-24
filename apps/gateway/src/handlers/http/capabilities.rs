//! Capability HTTP handlers.

use axum::Json;
use gateway::error::GatewayError;
use serde_json::json;

/// Get available capability types.
pub async fn list_capability_types() -> Json<serde_json::Value> {
    Json(json!({
        "capabilities": [
            {
                "type": "file_read",
                "description": "Read access to specific file system paths",
                "config_schema": {
                    "paths": ["string"]
                },
                "example": {
                    "type": "file_read",
                    "config": {
                        "paths": ["/tmp", "/data"]
                    }
                }
            },
            {
                "type": "file_write",
                "description": "Write access to specific file system paths",
                "config_schema": {
                    "paths": ["string"]
                },
                "example": {
                    "type": "file_write",
                    "config": {
                        "paths": ["/output"]
                    }
                }
            },
            {
                "type": "network_http",
                "description": "HTTP network access to specific hosts",
                "config_schema": {
                    "hosts": ["string"],
                    "methods": ["GET", "POST", "PUT", "DELETE", "PATCH"]
                },
                "example": {
                    "type": "network_http",
                    "config": {
                        "hosts": ["api.example.com"],
                        "methods": ["GET", "POST"]
                    }
                }
            },
            {
                "type": "network_tcp",
                "description": "TCP network access to specific ports",
                "config_schema": {
                    "ports": ["number"],
                    "hosts": ["string"]
                },
                "example": {
                    "type": "network_tcp",
                    "config": {
                        "ports": [5432, 6379],
                        "hosts": ["localhost"]
                    }
                }
            },
            {
                "type": "database",
                "description": "Database table access",
                "config_schema": {
                    "tables": ["string"],
                    "operations": ["select", "insert", "update", "delete"]
                },
                "example": {
                    "type": "database",
                    "config": {
                        "tables": ["users", "orders"],
                        "operations": ["select", "insert"]
                    }
                }
            },
            {
                "type": "llm",
                "description": "LLM/AI model access",
                "config_schema": {
                    "providers": ["string"],
                    "max_tokens_per_request": "number"
                },
                "example": {
                    "type": "llm",
                    "config": {
                        "providers": ["openai", "anthropic"],
                        "max_tokens_per_request": 4000
                    }
                }
            },
            {
                "type": "wallet",
                "description": "Blockchain wallet access",
                "config_schema": {
                    "chain_ids": ["number"],
                    "max_transaction_value": "string"
                },
                "example": {
                    "type": "wallet",
                    "config": {
                        "chain_ids": [1, 137],
                        "max_transaction_value": "1.0"
                    }
                }
            }
        ]
    }))
}

/// Validate capabilities and return normalized versions.
pub async fn validate_capabilities(
    Json(capabilities): Json<Vec<crate::capability::AgentCapability>>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    use crate::capability::AgentCapability;

    let mut validated = Vec::new();
    let mut errors = Vec::new();

    for (idx, cap) in capabilities.iter().enumerate() {
        let validation = match cap {
            AgentCapability::FileRead { paths } if paths.is_empty() => {
                Err("FileRead paths cannot be empty".to_string())
            }
            AgentCapability::FileWrite { paths } if paths.is_empty() => {
                Err("FileWrite paths cannot be empty".to_string())
            }
            AgentCapability::NetworkHttp { hosts, .. } if hosts.is_empty() => {
                Err("NetworkHttp hosts cannot be empty".to_string())
            }
            AgentCapability::NetworkTcp { ports, .. } if ports.is_empty() => {
                Err("NetworkTcp ports cannot be empty".to_string())
            }
            AgentCapability::Database { tables, .. } if tables.is_empty() => {
                Err("Database tables cannot be empty".to_string())
            }
            _ => Ok(()),
        };

        match validation {
            Ok(()) => {
                validated.push(json!({
                    "index": idx,
                    "capability": cap,
                    "description": cap.description(),
                    "compact_string": cap.to_compact_string(),
                    "valid": true,
                }));
            }
            Err(e) => {
                errors.push(json!({
                    "index": idx,
                    "capability": cap,
                    "error": e,
                }));
            }
        }
    }

    let is_valid = errors.is_empty();

    Ok(Json(json!({
        "valid": is_valid,
        "validated": validated,
        "errors": errors,
        "count": validated.len(),
    })))
}
