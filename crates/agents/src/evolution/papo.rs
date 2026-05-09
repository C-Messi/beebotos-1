//! PAPO — Process-Aware Policy Optimization
//!
//! Provides fine-grained process rewards for each intermediate tool call,
//! solving the sparse reward problem in agent RL training.

use crate::planning::ToolTrail;

/// Process reward for a single step
#[derive(Debug, Clone)]
pub struct ProcessReward {
    pub score: f32, // [-1.0, +1.0]
    pub reason: String,
}

impl ProcessReward {
    pub fn positive(score: f32, reason: impl Into<String>) -> Self {
        Self {
            score: score.clamp(0.0, 1.0),
            reason: reason.into(),
        }
    }

    pub fn negative(score: f32, reason: impl Into<String>) -> Self {
        Self {
            score: -score.clamp(0.0, 1.0),
            reason: reason.into(),
        }
    }

    pub fn neutral(reason: impl Into<String>) -> Self {
        Self {
            score: 0.0,
            reason: reason.into(),
        }
    }
}

/// Tool call validator trait: provides process rewards
#[async_trait::async_trait]
pub trait ToolCallValidator: Send + Sync {
    /// Tool name this validator handles
    fn tool_name(&self) -> &str;
    /// Validate a tool call result
    async fn validate(
        &self,
        params: &serde_json::Value,
        result: &serde_json::Value,
    ) -> ProcessReward;
}

/// Code execution validator
pub struct CodeExecutionValidator;

#[async_trait::async_trait]
impl ToolCallValidator for CodeExecutionValidator {
    fn tool_name(&self) -> &str {
        "execute_code"
    }

    async fn validate(
        &self,
        _params: &serde_json::Value,
        result: &serde_json::Value,
    ) -> ProcessReward {
        if let Some(exit_code) = result.get("exit_code").and_then(|v| v.as_i64()) {
            if exit_code == 0 {
                let output = result.get("output").and_then(|v| v.as_str()).unwrap_or("");
                if output.contains("test passed") || output.contains("OK") {
                    ProcessReward::positive(1.0, "Code executed and tests passed")
                } else {
                    ProcessReward::positive(0.5, "Code executed but no tests")
                }
            } else {
                let stderr = result.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
                if stderr.contains("syntax error") {
                    ProcessReward::negative(1.0, "Syntax error in code")
                } else {
                    ProcessReward::negative(0.5, "Runtime error")
                }
            }
        } else {
            ProcessReward::neutral("Unknown execution state")
        }
    }
}

/// HTTP API call validator
pub struct HttpApiValidator;

#[async_trait::async_trait]
impl ToolCallValidator for HttpApiValidator {
    fn tool_name(&self) -> &str {
        "http_request"
    }

    async fn validate(
        &self,
        _params: &serde_json::Value,
        result: &serde_json::Value,
    ) -> ProcessReward {
        let status = result.get("status").and_then(|v| v.as_u64()).unwrap_or(0);

        match status {
            200..=299 => {
                let body = result.get("body").and_then(|v| v.as_str()).unwrap_or("");
                if serde_json::from_str::<serde_json::Value>(body).is_ok() {
                    ProcessReward::positive(1.0, "Valid JSON response")
                } else {
                    ProcessReward::positive(0.7, "Successful HTTP but non-JSON response")
                }
            }
            400..=499 => ProcessReward::negative(0.6, &format!("Client error: {}", status)),
            500..=599 => ProcessReward::negative(0.4, &format!("Server error: {}", status)),
            _ => ProcessReward::negative(0.5, "Unexpected HTTP status"),
        }
    }
}

/// File operation validator
pub struct FileOperationValidator;

#[async_trait::async_trait]
impl ToolCallValidator for FileOperationValidator {
    fn tool_name(&self) -> &str {
        "file_operation"
    }

    async fn validate(
        &self,
        _params: &serde_json::Value,
        result: &serde_json::Value,
    ) -> ProcessReward {
        let success = result
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if success {
            ProcessReward::positive(0.8, "File operation succeeded")
        } else {
            let error = result
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            if error.contains("not found") || error.contains("No such file") {
                ProcessReward::negative(0.8, "File not found")
            } else if error.contains("permission") {
                ProcessReward::negative(0.6, "Permission denied")
            } else {
                ProcessReward::negative(0.5, "File operation failed")
            }
        }
    }
}

/// Credit assignment strategy for distributing process rewards
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CreditAssignmentStrategy {
    /// Equal distribution across all steps
    Uniform,
    /// Later steps get more weight (exponential decay backwards)
    Decay { decay_factor: f32 },
    /// Deviation from mean determines credit
    Advantage,
    /// Direct validator scores
    ValidatorAttribution,
}

/// PAPO configuration
#[derive(Debug, Clone)]
pub struct PapoConfig {
    pub process_reward_weight: f32,
    pub final_reward_weight: f32,
    pub validation_timeout_ms: u64,
    pub credit_assignment: CreditAssignmentStrategy,
}

impl Default for PapoConfig {
    fn default() -> Self {
        Self {
            process_reward_weight: 0.5,
            final_reward_weight: 0.5,
            validation_timeout_ms: 5000,
            credit_assignment: CreditAssignmentStrategy::ValidatorAttribution,
        }
    }
}

/// Credit assigner: distributes rewards across trajectory steps
#[derive(Debug, Clone)]
pub struct CreditAssigner {
    config: PapoConfig,
}

impl CreditAssigner {
    pub fn new(config: PapoConfig) -> Self {
        Self { config }
    }

    /// Assign process rewards to each step
    pub fn assign(&self, step_rewards: &[ProcessReward]) -> Vec<f32> {
        let n = step_rewards.len();
        if n == 0 {
            return Vec::new();
        }

        match self.config.credit_assignment {
            CreditAssignmentStrategy::Uniform => step_rewards
                .iter()
                .map(|r| r.score * self.config.process_reward_weight / n as f32)
                .collect(),
            CreditAssignmentStrategy::Decay { decay_factor } => {
                let total_weight: f32 = (0..n).map(|i| decay_factor.powi(i as i32)).sum();
                step_rewards
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        let weight = decay_factor.powi(i as i32) / total_weight;
                        r.score * self.config.process_reward_weight * weight
                    })
                    .collect()
            }
            CreditAssignmentStrategy::Advantage => {
                let baseline = step_rewards.iter().map(|r| r.score).sum::<f32>() / n as f32;
                step_rewards
                    .iter()
                    .map(|r| {
                        let advantage = r.score - baseline;
                        advantage * self.config.process_reward_weight
                    })
                    .collect()
            }
            CreditAssignmentStrategy::ValidatorAttribution => step_rewards
                .iter()
                .map(|r| r.score * self.config.process_reward_weight)
                .collect(),
        }
    }
}

/// PAPO trainer
pub struct PapoTrainer {
    validators: Vec<std::sync::Arc<dyn ToolCallValidator>>,
    credit_assigner: CreditAssigner,
    config: PapoConfig,
}

impl Clone for PapoTrainer {
    fn clone(&self) -> Self {
        Self {
            validators: self.validators.clone(),
            credit_assigner: self.credit_assigner.clone(),
            config: self.config.clone(),
        }
    }
}

impl std::fmt::Debug for PapoTrainer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PapoTrainer")
            .field("validator_count", &self.validators.len())
            .field("credit_assigner", &self.credit_assigner)
            .field("config", &self.config)
            .finish()
    }
}

impl PapoTrainer {
    pub fn new(config: PapoConfig) -> Self {
        let assigner = CreditAssigner::new(config.clone());
        Self {
            validators: Vec::new(),
            credit_assigner: assigner,
            config,
        }
    }

    /// Register a validator
    pub fn register_validator(&mut self, validator: std::sync::Arc<dyn ToolCallValidator>) {
        self.validators.push(validator);
    }

    /// Register default validators
    pub fn with_defaults(mut self) -> Self {
        self.register_validator(std::sync::Arc::new(CodeExecutionValidator));
        self.register_validator(std::sync::Arc::new(HttpApiValidator));
        self.register_validator(std::sync::Arc::new(FileOperationValidator));
        self
    }

    /// Validate a single tool call using matching validator
    pub async fn validate_tool_call(
        &self,
        tool_name: &str,
        params: &serde_json::Value,
        result: &serde_json::Value,
    ) -> ProcessReward {
        for validator in &self.validators {
            if validator.tool_name() == tool_name {
                return validator.validate(params, result).await;
            }
        }
        ProcessReward::neutral(format!("No validator for tool: {}", tool_name))
    }

    /// Compute step-level rewards for a trajectory
    pub async fn compute_step_rewards(&self, trail: &ToolTrail) -> Vec<ProcessReward> {
        let mut rewards = Vec::new();
        for step in &trail.steps {
            for call in &step.tool_calls {
                let tool_name = call.tool_name.split(':').last().unwrap_or(&call.tool_name);
                let params = &call.parameters;
                let result = match serde_json::from_str::<serde_json::Value>(&call.result_summary) {
                    Ok(v) => v,
                    Err(_) => serde_json::json!({"raw": call.result_summary}),
                };
                let reward = self.validate_tool_call(tool_name, params, &result).await;
                rewards.push(reward);
            }
        }
        rewards
    }

    /// Compute total reward = process rewards + final reward
    pub fn compute_total_reward(&self, step_rewards: &[ProcessReward], trail_success: bool) -> f32 {
        let credits = self.credit_assigner.assign(step_rewards);
        let process_total: f32 = credits.iter().sum();

        let final_reward = if trail_success {
            self.config.final_reward_weight
        } else {
            -self.config.final_reward_weight
        };

        process_total + final_reward
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_reward_constructors() {
        let pos = ProcessReward::positive(0.8, "good");
        assert!(pos.score > 0.0);
        let neg = ProcessReward::negative(0.8, "bad");
        assert!(neg.score < 0.0);
        let neu = ProcessReward::neutral("ok");
        assert_eq!(neu.score, 0.0);
    }

    #[tokio::test]
    async fn test_code_validator() {
        let v = CodeExecutionValidator;
        let result = serde_json::json!({"exit_code": 0, "output": "test passed"});
        let reward = v.validate(&serde_json::Value::Null, &result).await;
        assert!(reward.score > 0.9);

        let result = serde_json::json!({"exit_code": 1, "stderr": "syntax error at line 5"});
        let reward = v.validate(&serde_json::Value::Null, &result).await;
        assert!(reward.score < -0.9);
    }

    #[tokio::test]
    async fn test_http_validator() {
        let v = HttpApiValidator;
        let result = serde_json::json!({"status": 200, "body": "{\"ok\": true}"});
        let reward = v.validate(&serde_json::Value::Null, &result).await;
        assert!(reward.score > 0.9);

        let result = serde_json::json!({"status": 500, "body": "error"});
        let reward = v.validate(&serde_json::Value::Null, &result).await;
        assert!(reward.score < 0.0);
    }

    #[tokio::test]
    async fn test_file_validator() {
        let v = FileOperationValidator;
        let result = serde_json::json!({"success": true});
        let reward = v.validate(&serde_json::Value::Null, &result).await;
        assert!(reward.score > 0.0);

        let result = serde_json::json!({"success": false, "error": "No such file"});
        let reward = v.validate(&serde_json::Value::Null, &result).await;
        assert!(reward.score < 0.0);
    }

    #[test]
    fn test_credit_assigner_uniform() {
        let config = PapoConfig {
            process_reward_weight: 1.0,
            credit_assignment: CreditAssignmentStrategy::Uniform,
            ..Default::default()
        };
        let assigner = CreditAssigner::new(config);
        let rewards = vec![
            ProcessReward::positive(1.0, "a"),
            ProcessReward::positive(1.0, "b"),
        ];
        let credits = assigner.assign(&rewards);
        assert_eq!(credits.len(), 2);
        assert!((credits[0] - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_credit_assigner_decay() {
        let config = PapoConfig {
            process_reward_weight: 1.0,
            credit_assignment: CreditAssignmentStrategy::Decay { decay_factor: 0.5 },
            ..Default::default()
        };
        let assigner = CreditAssigner::new(config);
        let rewards = vec![
            ProcessReward::positive(1.0, "a"),
            ProcessReward::positive(1.0, "b"),
        ];
        let credits = assigner.assign(&rewards);
        // Step 0 gets more weight (decay from start)
        assert!(credits[0] > credits[1]);
    }

    #[tokio::test]
    async fn test_papo_trainer_total_reward() {
        let trainer = PapoTrainer::new(PapoConfig::default()).with_defaults();
        let rewards = vec![
            ProcessReward::positive(1.0, "step1"),
            ProcessReward::negative(0.5, "step2"),
        ];
        let total = trainer.compute_total_reward(&rewards, true);
        assert!(total > 0.0); // success bonus

        let total_fail = trainer.compute_total_reward(&rewards, false);
        assert!(total_fail < total); // failure penalty
    }
}
