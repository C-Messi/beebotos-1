//! Cron Job Manager Tool (HTTP Loopback)
//!
//! A `SkillTool` that lets LLM agents manage scheduled cron jobs by calling
//! the Gateway HTTP API via loopback (`http://127.0.0.1:8080`).
//!
//! Authentication token priority:
//! 1. `CRON_TOOL_API_TOKEN` environment variable (recommended for production)
//! 2. `INTERNAL_SERVICE_TOKEN` environment variable (shared with other internal
//!    tools)
//! 3. Fallback to `demo-token` (logs a warning — insecure in production)
//!
//! Supported actions: list, create, update, delete, run, history.

use serde_json::Value;

use crate::skills::SkillTool;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8080";
const DEMO_TOKEN: &str = "demo-token";

pub struct CronJobManagerTool {
    client: reqwest::Client,
    base_url: String,
    auth_token: String,
}

impl CronJobManagerTool {
    pub fn new() -> Self {
        let auth_token = Self::resolve_auth_token();
        Self {
            client: reqwest::Client::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
            auth_token,
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Resolve auth token from environment (secure) or fallback to demo token
    /// (insecure).
    fn resolve_auth_token() -> String {
        if let Ok(token) = std::env::var("CRON_TOOL_API_TOKEN") {
            if !token.is_empty() {
                return token;
            }
        }
        if let Ok(token) = std::env::var("INTERNAL_SERVICE_TOKEN") {
            if !token.is_empty() {
                return token;
            }
        }
        tracing::warn!(
            "CronJobManagerTool: CRON_TOOL_API_TOKEN and INTERNAL_SERVICE_TOKEN not set. Falling \
             back to hard-coded demo-token. This is INSECURE in production!"
        );
        DEMO_TOKEN.to_string()
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.auth_token)
    }

    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<String, String> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self
            .client
            .request(method, &url)
            .header("Authorization", self.auth_header());
        if let Some(b) = body {
            req = req.header("Content-Type", "application/json").json(&b);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;
        if !status.is_success() {
            return Err(format!("HTTP {}: {}", status, text));
        }
        Ok(text)
    }
}

impl Default for CronJobManagerTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SkillTool for CronJobManagerTool {
    fn name(&self) -> &str {
        "cron_job_manager"
    }

    fn description(&self) -> &str {
        "Manage scheduled cron jobs via Gateway HTTP API. Actions: list (all jobs), create (new \
         job), update (modify job), delete (remove job), run (trigger immediately), history \
         (execution logs)."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "create", "update", "delete", "run", "history"],
                    "description": "Operation to perform"
                },
                "id": {
                    "type": "string",
                    "description": "Job ID (required for update/delete/run/history)"
                },
                "name": {
                    "type": "string",
                    "description": "Job name (required for create)"
                },
                "description": {
                    "type": "string",
                    "description": "Job description"
                },
                "schedule_type": {
                    "type": "string",
                    "enum": ["at", "every", "cron"],
                    "description": "Schedule type: at=one-shot, every=interval, cron=cron expression"
                },
                "schedule_expr": {
                    "type": "string",
                    "description": "Schedule expression (e.g. '2026-05-12 09:00', '30m', '0 9 * * *')"
                },
                "timezone": {
                    "type": "string",
                    "description": "Timezone, default Asia/Shanghai"
                },
                "prompt": {
                    "type": "string",
                    "description": "Prompt sent to the agent when the job executes"
                },
                "enabled": {
                    "type": "boolean",
                    "description": "Whether the job is enabled"
                },
                "context_mode": {
                    "type": "string",
                    "enum": ["main", "isolated"],
                    "description": "Context mode: main=shared session, isolated=independent session"
                },
                "delivery_channel": {
                    "type": "string",
                    "description": "Notification channel, e.g. webchat or webhook"
                },
                "delivery_target": {
                    "type": "string",
                    "description": "Notification target, e.g. channel name or webhook URL"
                },
                "max_runs": {
                    "type": "integer",
                    "description": "Maximum execution count (auto-disable when reached)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let action = params["action"]
            .as_str()
            .ok_or("Missing 'action' parameter")?;
        match action {
            "list" => self.list_jobs().await,
            "create" => self.create_job(params).await,
            "update" => self.update_job(params).await,
            "delete" => self.delete_job(params).await,
            "run" => self.run_job(params).await,
            "history" => self.job_history(params).await,
            _ => Err(format!("Unknown action: {}", action)),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

impl CronJobManagerTool {
    async fn list_jobs(&self) -> Result<String, String> {
        self.request(reqwest::Method::GET, "/api/v1/cron/jobs", None)
            .await
    }

    async fn create_job(&self, params: &Value) -> Result<String, String> {
        let name = params["name"]
            .as_str()
            .ok_or("'name' is required for create")?;
        let schedule_type = params["schedule_type"]
            .as_str()
            .ok_or("'schedule_type' is required for create")?;
        let schedule_expr = params["schedule_expr"]
            .as_str()
            .ok_or("'schedule_expr' is required for create")?;
        let prompt = params["prompt"]
            .as_str()
            .ok_or("'prompt' is required for create")?;

        let body = serde_json::json!({
            "name": name,
            "description": params["description"].as_str(),
            "schedule_type": schedule_type,
            "schedule_expr": schedule_expr,
            "timezone": params["timezone"].as_str(),
            "prompt": prompt,
            "enabled": params["enabled"].as_bool(),
            "context_mode": params["context_mode"].as_str(),
            "delivery_channel": params["delivery_channel"].as_str(),
            "delivery_target": params["delivery_target"].as_str(),
            "max_runs": params["max_runs"].as_i64(),
        });

        self.request(reqwest::Method::POST, "/api/v1/cron/jobs", Some(body))
            .await
    }

    /// Update merges provided fields with current job values, then sends a
    /// complete `CronJobRequest` so the backend deserialization succeeds.
    async fn update_job(&self, params: &Value) -> Result<String, String> {
        let id = params["id"].as_str().ok_or("'id' is required for update")?;

        // 1. Fetch current job
        let current_json = self
            .request(
                reqwest::Method::GET,
                &format!("/api/v1/cron/jobs/{}", id),
                None,
            )
            .await?;
        let current: Value = serde_json::from_str(&current_json)
            .map_err(|e| format!("Failed to parse current job: {}", e))?;

        // 2. Build complete body with current values as defaults
        let mut body = serde_json::Map::new();

        // Helper: get value from params or fallback to current job
        let get_str = |key: &str| -> String {
            params[key]
                .as_str()
                .or_else(|| current.get(key).and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string()
        };
        let get_opt_str = |key: &str| -> Option<String> {
            params[key]
                .as_str()
                .or_else(|| current.get(key).and_then(|v| v.as_str()))
                .map(|s| s.to_string())
        };
        let get_opt_bool = |key: &str| -> Option<bool> {
            params[key]
                .as_bool()
                .or_else(|| current.get(key).and_then(|v| v.as_bool()))
        };
        let get_opt_i64 = |key: &str| -> Option<i64> {
            params[key]
                .as_i64()
                .or_else(|| current.get(key).and_then(|v| v.as_i64()))
        };

        body.insert("name".to_string(), serde_json::json!(get_str("name")));
        body.insert(
            "description".to_string(),
            serde_json::json!(get_opt_str("description")),
        );
        body.insert(
            "schedule_type".to_string(),
            serde_json::json!(get_str("schedule_type")),
        );
        body.insert(
            "schedule_expr".to_string(),
            serde_json::json!(get_str("schedule_expr")),
        );
        body.insert(
            "timezone".to_string(),
            serde_json::json!(get_opt_str("timezone")),
        );
        body.insert("prompt".to_string(), serde_json::json!(get_str("prompt")));
        body.insert(
            "enabled".to_string(),
            serde_json::json!(get_opt_bool("enabled")),
        );
        body.insert(
            "context_mode".to_string(),
            serde_json::json!(get_opt_str("context_mode")),
        );
        body.insert(
            "delivery_channel".to_string(),
            serde_json::json!(get_opt_str("delivery_channel")),
        );
        body.insert(
            "delivery_target".to_string(),
            serde_json::json!(get_opt_str("delivery_target")),
        );
        body.insert(
            "max_runs".to_string(),
            serde_json::json!(get_opt_i64("max_runs")),
        );

        self.request(
            reqwest::Method::PUT,
            &format!("/api/v1/cron/jobs/{}", id),
            Some(Value::Object(body)),
        )
        .await
    }

    async fn delete_job(&self, params: &Value) -> Result<String, String> {
        let id = params["id"].as_str().ok_or("'id' is required for delete")?;
        self.request(
            reqwest::Method::DELETE,
            &format!("/api/v1/cron/jobs/{}", id),
            None,
        )
        .await
    }

    async fn run_job(&self, params: &Value) -> Result<String, String> {
        let id = params["id"].as_str().ok_or("'id' is required for run")?;
        self.request(
            reqwest::Method::POST,
            &format!("/api/v1/cron/jobs/{}/run", id),
            Some(serde_json::json!({})),
        )
        .await
    }

    async fn job_history(&self, params: &Value) -> Result<String, String> {
        let id = params["id"]
            .as_str()
            .ok_or("'id' is required for history")?;
        self.request(
            reqwest::Method::GET,
            &format!("/api/v1/cron/jobs/{}/runs", id),
            None,
        )
        .await
    }
}
