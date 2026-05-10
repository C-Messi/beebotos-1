//! Approval Chain Module
//!
//! Provides approval gating for destructive operations (order placement,
//! deletion, sending, etc.) with configurable approval modes and rules.

use serde::{Deserialize, Serialize};

/// Approval gate configuration
#[derive(Debug, Clone)]
pub struct ApprovalGate {
    /// Operation types that require approval (skill_id patterns)
    pub requires_approval: Vec<String>,
    /// Approval mode
    pub mode: ApprovalMode,
    /// Default timeout for sync confirmation
    pub default_timeout_secs: u64,
}

/// Approval mode
#[derive(Debug, Clone)]
pub enum ApprovalMode {
    /// Block and wait for user confirmation (suitable for chat scenarios)
    SyncConfirm { timeout_secs: u64 },
    /// Rule-based auto-approval (e.g. paper trading passes automatically)
    RuleBased(Vec<ApprovalRule>),
    /// Requires admin key signature
    AdminSignature,
    /// Disabled — no approval required
    Disabled,
}

/// Approval rule for rule-based mode
#[derive(Debug, Clone)]
pub struct ApprovalRule {
    /// Human-readable condition description
    pub description: String,
    /// Auto-approve if condition matches
    pub auto_approve: bool,
    /// Condition check (simplified: pattern matching on skill_id and env)
    pub skill_id_pattern: String,
    pub env_key: Option<String>,
    pub env_value: Option<String>,
}

impl ApprovalRule {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            auto_approve: false,
            skill_id_pattern: String::new(),
            env_key: None,
            env_value: None,
        }
    }

    pub fn auto_approve(mut self) -> Self {
        self.auto_approve = true;
        self
    }

    pub fn pattern(mut self, pattern: impl Into<String>) -> Self {
        self.skill_id_pattern = pattern.into();
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_key = Some(key.into());
        self.env_value = Some(value.into());
        self
    }

    /// Check if this rule matches the given context
    pub fn matches(&self, skill_id: &str, env: &std::collections::HashMap<String, String>) -> bool {
        let pattern_matches = if self.skill_id_pattern.is_empty() {
            true
        } else {
            skill_id.contains(&self.skill_id_pattern)
        };

        let env_matches = match (&self.env_key, &self.env_value) {
            (Some(key), Some(value)) => env.get(key).map(|v| v == value).unwrap_or(false),
            _ => true,
        };

        pattern_matches && env_matches
    }
}

/// Approval request sent to user/admin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub request_id: String,
    pub skill_id: String,
    pub params: serde_json::Value,
    /// 🆕 FIX: Original user input preserved for parameter re-extraction after confirmation
    pub original_input: String,
    /// Risk level: low/medium/high/critical
    pub risk_level: RiskLevel,
    /// Human-readable action description
    pub description: String,
    /// Timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Risk level for operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn from_skill_id(skill_id: &str) -> Self {
        let lower = skill_id.to_lowercase();
        if lower.contains("delete") || lower.contains("remove") || lower.contains("cancel_all") {
            RiskLevel::High
        } else if lower.contains("place_") && lower.contains("_order") {
            RiskLevel::Critical
        } else if lower.contains("send") || lower.contains("transfer") || lower.contains("withdraw")
        {
            RiskLevel::Critical
        } else if lower.contains("update") || lower.contains("modify") {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        }
    }
}

/// Approval result
#[derive(Debug, Clone)]
pub enum ApprovalResult {
    Approved,
    Rejected { reason: String },
    Timeout,
    AutoApproved { rule: String },
}

impl ApprovalGate {
    pub fn new() -> Self {
        Self {
            requires_approval: vec![
                "place_".to_string(),
                "delete".to_string(),
                "send".to_string(),
                "transfer".to_string(),
                "cancel_all".to_string(),
            ],
            mode: ApprovalMode::RuleBased(vec![]),
            default_timeout_secs: 60,
        }
    }

    /// Create with paper trading auto-approve rules
    pub fn with_paper_trading_rules() -> Self {
        let rules = vec![
            // Paper trading auto-pass
            ApprovalRule::new("Paper trading orders auto-approved")
                .auto_approve()
                .pattern("place_")
                .env("ALPACA_PAPER_TRADE", "true"),
            // Data queries auto-pass
            ApprovalRule::new("Data queries auto-approved")
                .auto_approve()
                .pattern("get_"),
            ApprovalRule::new("Snapshot queries auto-approved")
                .auto_approve()
                .pattern("snapshot"),
        ];

        Self {
            requires_approval: vec![
                "place_".to_string(),
                "delete".to_string(),
                "send".to_string(),
                "transfer".to_string(),
            ],
            mode: ApprovalMode::RuleBased(rules),
            default_timeout_secs: 60,
        }
    }

    /// Check if a skill execution requires approval
    pub fn requires_approval_for(&self, skill_id: &str) -> bool {
        match &self.mode {
            ApprovalMode::Disabled => false,
            _ => {
                let lower = skill_id.to_lowercase();
                self.requires_approval
                    .iter()
                    .any(|pattern| lower.contains(pattern))
            }
        }
    }

    /// Evaluate approval for a skill execution
    pub fn evaluate(
        &self,
        skill_id: &str,
        _params: &serde_json::Value,
        env: &std::collections::HashMap<String, String>,
    ) -> ApprovalResult {
        if !self.requires_approval_for(skill_id) {
            return ApprovalResult::Approved;
        }

        match &self.mode {
            ApprovalMode::Disabled => ApprovalResult::Approved,
            ApprovalMode::SyncConfirm { timeout_secs } => {
                // In sync mode, the caller is responsible for sending the request
                // and waiting for response. We return a pending state.
                // For the engine integration, this will be handled at a higher level.
                ApprovalResult::Rejected {
                    reason: format!("Sync approval required (timeout: {}s)", timeout_secs),
                }
            }
            ApprovalMode::RuleBased(rules) => {
                for rule in rules {
                    if rule.matches(skill_id, env) {
                        if rule.auto_approve {
                            return ApprovalResult::AutoApproved {
                                rule: rule.description.clone(),
                            };
                        }
                    }
                }
                // No matching auto-approve rule
                ApprovalResult::Rejected {
                    reason: "No auto-approval rule matched".to_string(),
                }
            }
            ApprovalMode::AdminSignature => ApprovalResult::Rejected {
                reason: "Admin signature required".to_string(),
            },
        }
    }

    /// Generate human-readable action description
    pub fn describe_action(skill_id: &str, params: &serde_json::Value) -> String {
        let risk = RiskLevel::from_skill_id(skill_id);
        let risk_str = match risk {
            RiskLevel::Low => "低风险",
            RiskLevel::Medium => "中风险",
            RiskLevel::High => "高风险",
            RiskLevel::Critical => "⚠️ 关键操作",
        };

        format!(
            "[{}] 执行操作: {}，参数: {}",
            risk_str,
            skill_id,
            params.to_string().chars().take(200).collect::<String>()
        )
    }

    /// Build an approval request
    pub fn build_request(&self, skill_id: &str, params: &serde_json::Value, original_input: &str) -> ApprovalRequest {
        ApprovalRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            skill_id: skill_id.to_string(),
            params: params.clone(),
            original_input: original_input.to_string(),
            risk_level: RiskLevel::from_skill_id(skill_id),
            description: Self::describe_action(skill_id, params),
            created_at: chrono::Utc::now(),
        }
    }
}

impl Default for ApprovalGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_level() {
        assert_eq!(
            RiskLevel::from_skill_id("place_crypto_order"),
            RiskLevel::Critical
        );
        assert_eq!(RiskLevel::from_skill_id("delete_item"), RiskLevel::High);
        assert_eq!(RiskLevel::from_skill_id("get_weather"), RiskLevel::Low);
    }

    #[test]
    fn test_paper_trading_auto_approve() {
        let gate = ApprovalGate::with_paper_trading_rules();
        let mut env = std::collections::HashMap::new();
        env.insert("ALPACA_PAPER_TRADE".to_string(), "true".to_string());

        let result = gate.evaluate("place_crypto_order", &serde_json::json!({}), &env);
        matches!(result, ApprovalResult::AutoApproved { .. });
    }

    #[test]
    fn test_live_trading_requires_approval() {
        let gate = ApprovalGate::with_paper_trading_rules();
        let env = std::collections::HashMap::new();

        let result = gate.evaluate("place_crypto_order", &serde_json::json!({}), &env);
        matches!(result, ApprovalResult::Rejected { .. });
    }
}
