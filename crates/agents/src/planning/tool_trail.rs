//! ToolTrail Module
//!
//! Execution visualization and tracing for planning processes.
//! Tracks step execution, tool calls, reasoning, and resource usage.

use serde::{Deserialize, Serialize};

/// Tool execution trail for a plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTrail {
    pub plan_id: String,
    pub steps: Vec<TrailStep>,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub status: TrailStatus,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
}

/// Status of a trail
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrailStatus {
    Running,
    Success,
    Failed,
    Cancelled,
}

/// Individual step in the trail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrailStep {
    pub step_number: usize,
    pub description: String,
    pub tool_calls: Vec<ToolCallRecord>,
    pub reasoning: Option<String>,
    pub status: StepStatus,
    pub duration_ms: u64,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Status of a trail step
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepStatus {
    Pending,
    Running,
    Success,
    Failed,
    Skipped,
}

/// Record of a single tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub tool_name: String,
    pub parameters: serde_json::Value,
    pub result_summary: String,
    pub success: bool,
}

impl ToolTrail {
    pub fn new(plan_id: impl Into<String>) -> Self {
        Self {
            plan_id: plan_id.into(),
            steps: Vec::new(),
            start_time: Some(chrono::Utc::now()),
            end_time: None,
            status: TrailStatus::Running,
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    pub fn add_step(&mut self, step_number: usize, description: impl Into<String>) -> &mut TrailStep {
        let step = TrailStep {
            step_number,
            description: description.into(),
            tool_calls: Vec::new(),
            reasoning: None,
            status: StepStatus::Pending,
            duration_ms: 0,
            input_tokens: 0,
            output_tokens: 0,
        };
        self.steps.push(step);
        self.steps.last_mut().unwrap()
    }

    pub fn record_tool_call(
        &mut self,
        step_number: usize,
        tool_name: impl Into<String>,
        parameters: serde_json::Value,
        result: &str,
        success: bool,
    ) {
        if let Some(step) = self.steps.iter_mut().find(|s| s.step_number == step_number) {
            step.tool_calls.push(ToolCallRecord {
                tool_name: tool_name.into(),
                parameters,
                result_summary: result.chars().take(200).collect(),
                success,
            });
        }
    }

    pub fn set_step_reasoning(&mut self, step_number: usize, reasoning: impl Into<String>) {
        if let Some(step) = self.steps.iter_mut().find(|s| s.step_number == step_number) {
            step.reasoning = Some(reasoning.into());
        }
    }

    pub fn set_step_status(&mut self, step_number: usize, status: StepStatus) {
        if let Some(step) = self.steps.iter_mut().find(|s| s.step_number == step_number) {
            step.status = status;
        }
    }

    pub fn set_step_duration(&mut self, step_number: usize, duration_ms: u64) {
        if let Some(step) = self.steps.iter_mut().find(|s| s.step_number == step_number) {
            step.duration_ms = duration_ms;
        }
    }

    pub fn add_tokens(&mut self, step_number: usize, input: u32, output: u32) {
        if let Some(step) = self.steps.iter_mut().find(|s| s.step_number == step_number) {
            step.input_tokens += input;
            step.output_tokens += output;
        }
        self.total_input_tokens += input;
        self.total_output_tokens += output;
    }

    pub fn finish(&mut self, status: TrailStatus) {
        self.status = status;
        self.end_time = Some(chrono::Utc::now());
    }

    pub fn duration_ms(&self) -> u64 {
        match (self.start_time, self.end_time) {
            (Some(start), Some(end)) => (end - start).num_milliseconds() as u64,
            _ => 0,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// 🆕 PHASE 5: Convert trail to RL training data format
    ///
    /// Produces a compact representation suitable for DAPO/PAPO training:
    /// - Action sequence (tool calls)
    /// - State snapshots (step descriptions)
    /// - Terminal reward (success/failure)
    /// - Token consumption
    pub fn to_training_data(&self) -> TrainingData {
        let actions: Vec<String> = self.steps.iter()
            .flat_map(|s| s.tool_calls.iter().map(|c| c.tool_name.clone()))
            .collect();

        let states: Vec<String> = self.steps.iter()
            .map(|s| s.description.clone())
            .collect();

        let step_rewards: Vec<f32> = self.steps.iter()
            .map(|s| {
                let successes = s.tool_calls.iter().filter(|c| c.success).count();
                let total = s.tool_calls.len().max(1);
                successes as f32 / total as f32
            })
            .collect();

        let final_reward = match self.status {
            TrailStatus::Success => 1.0,
            TrailStatus::Failed => -1.0,
            _ => 0.0,
        };

        TrainingData {
            plan_id: self.plan_id.clone(),
            states,
            actions,
            step_rewards,
            final_reward,
            total_tokens: self.total_input_tokens + self.total_output_tokens,
            duration_ms: self.duration_ms(),
        }
    }
}

/// RL training data extracted from a ToolTrail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingData {
    pub plan_id: String,
    pub states: Vec<String>,
    pub actions: Vec<String>,
    pub step_rewards: Vec<f32>,
    pub final_reward: f32,
    pub total_tokens: u32,
    pub duration_ms: u64,
}

/// Trail collector for aggregating trails across sessions
#[derive(Debug, Clone, Default)]
pub struct TrailCollector {
    trails: Vec<ToolTrail>,
}

impl TrailCollector {
    pub fn new() -> Self {
        Self { trails: Vec::new() }
    }

    pub fn add_trail(&mut self, trail: ToolTrail) {
        self.trails.push(trail);
    }

    pub fn get_trail(&self, plan_id: &str) -> Option<&ToolTrail> {
        self.trails.iter().find(|t| t.plan_id == plan_id)
    }

    pub fn recent_trails(&self, limit: usize) -> Vec<&ToolTrail> {
        self.trails.iter().rev().take(limit).collect()
    }

    pub fn success_rate(&self) -> f32 {
        if self.trails.is_empty() {
            return 0.0;
        }
        let success_count = self.trails.iter().filter(|t| t.status == TrailStatus::Success).count();
        success_count as f32 / self.trails.len() as f32
    }
}
