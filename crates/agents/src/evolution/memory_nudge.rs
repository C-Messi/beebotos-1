//! Nudge Engine — Proactive Memory Consolidation
//!
//! The Nudge Engine actively triggers memory consolidation based on:
//! - Turn counter (every N user turns)
//! - Task complexity (tool call count threshold)
//! - Explicit user feedback ("remember this")
//! - Implicit adoption (user didn't modify output)
//!
//! It evaluates whether information is worth persisting using
//! the MemoryQualityEvaluator, then writes to L1/L2/L3 layers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::memory_quality::MemoryQualityEvaluator;
use crate::error::{AgentError, Result};
use crate::memory::search::{MemorySearch, SearchResult};
use crate::planning::{ToolTrail, TrailStatus};

/// Trigger conditions for Nudge Engine
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NudgeTrigger {
    /// Periodic: every N user turns
    Periodic { turn_count: u64 },
    /// Task completed with sufficient tool calls
    TaskComplexity { tool_call_count: usize },
    /// Explicit user confirmation
    ExplicitFeedback { feedback: String },
    /// Implicit adoption (user didn't modify)
    ImplicitAdoption,
    /// Agent actively called memory tool (resets counter)
    MemoryToolInvoked,
}

/// Configuration for Nudge Engine
#[derive(Debug, Clone)]
pub struct NudgeConfig {
    /// Trigger Memory Nudge every N user turns
    pub memory_nudge_interval: u64,
    /// Trigger Skill Nudge every N successful skill executions
    pub skill_nudge_interval: u64,
    /// Minimum tool calls for a task to be worth remembering
    pub min_tool_calls_for_memory: usize,
    /// Maximum characters per memory entry (L1/L2)
    pub max_memory_entry_chars: usize,
    /// Quality threshold (0.0-1.0) for writing to L1/L2
    pub quality_threshold: f32,
    /// Maximum L1 (MEMORY.md) capacity in characters
    pub max_l1_chars: usize,
    /// Maximum L2 (USER.md) capacity in characters
    pub max_l2_chars: usize,
}

impl Default for NudgeConfig {
    fn default() -> Self {
        Self {
            memory_nudge_interval: 10,
            skill_nudge_interval: 5,
            min_tool_calls_for_memory: 5,
            max_memory_entry_chars: 2200,
            quality_threshold: 0.6,
            max_l1_chars: 2200,
            max_l2_chars: 1375,
        }
    }
}

/// A candidate memory entry awaiting evaluation
#[derive(Debug, Clone)]
pub struct MemoryCandidate {
    /// Proposed content
    pub content: String,
    /// Category: "project_fact", "user_preference", "pitfall", "workflow"
    pub category: String,
    /// Whether this is a stable fact (not temporary state)
    pub is_stable_fact: bool,
    /// Whether this has cross-session reuse value
    pub cross_session_value: bool,
    /// Whether user explicitly or implicitly confirmed
    pub has_user_confirmation: bool,
    /// Embedding vector for redundancy checking
    pub embedding: Vec<f32>,
    /// Source session/task ID
    pub source_id: String,
}

/// Memory write executor
#[derive(Debug, Clone)]
pub struct MemoryWriter {
    /// Base directory for memory files
    pub base_path: std::path::PathBuf,
}

impl MemoryWriter {
    pub fn new(base_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    /// Write or update MEMORY.md (L1)
    pub async fn write_l1(&self, content: &str) -> Result<()> {
        let path = self.base_path.join("MEMORY.md");
        // TODO: implement Patch-based write (add/replace/remove by substring matching)
        // For now, write atomically
        let temp = path.with_extension("tmp");
        tokio::fs::write(&temp, content)
            .await
            .map_err(|e| AgentError::storage(format!("Failed to write L1 temp: {}", e)))?;
        tokio::fs::rename(&temp, &path)
            .await
            .map_err(|e| AgentError::storage(format!("Failed to rename L1: {}", e)))?;
        Ok(())
    }

    /// Write or update USER.md (L2)
    pub async fn write_l2(&self, content: &str) -> Result<()> {
        let path = self.base_path.join("USER.md");
        let temp = path.with_extension("tmp");
        tokio::fs::write(&temp, content)
            .await
            .map_err(|e| AgentError::storage(format!("Failed to write L2 temp: {}", e)))?;
        tokio::fs::rename(&temp, &path)
            .await
            .map_err(|e| AgentError::storage(format!("Failed to rename L2: {}", e)))?;
        Ok(())
    }

    /// Append to L3 (SQLite-backed history)
    pub async fn write_l3(
        &self,
        memory_system: &Arc<dyn MemorySearch>,
        content: &str,
        metadata: HashMap<String, String>,
    ) -> Result<()> {
        memory_system
            .add_entry(uuid::Uuid::new_v4(), content, metadata)
            .await
            .map_err(|e| AgentError::storage(format!("Failed to write L3: {}", e)))?;
        Ok(())
    }
}

/// Nudge Engine — proactive memory consolidation
#[derive(Debug)]
pub struct NudgeEngine {
    /// User turn counter
    turn_counter: AtomicU64,
    /// Successful skill execution counter
    skill_counter: AtomicU64,
    /// Configuration
    pub config: NudgeConfig,
    /// Quality evaluator
    evaluator: MemoryQualityEvaluator,
    /// Memory writer
    writer: MemoryWriter,
}

impl NudgeEngine {
    pub fn new(config: NudgeConfig, writer: MemoryWriter) -> Self {
        Self {
            turn_counter: AtomicU64::new(0),
            skill_counter: AtomicU64::new(0),
            config,
            evaluator: MemoryQualityEvaluator::new(),
            writer,
        }
    }

    /// Increment user turn counter. Returns true if a Memory Nudge should
    /// trigger.
    pub fn increment_turn(&self) -> bool {
        let count = self.turn_counter.fetch_add(1, Ordering::Relaxed) + 1;
        count >= self.config.memory_nudge_interval
    }

    /// Reset turn counter (called when agent invokes memory tool)
    pub fn reset_turn_counter(&self) {
        self.turn_counter.store(0, Ordering::Relaxed);
    }

    /// Increment skill counter. Returns true if a Skill Nudge should trigger.
    pub fn increment_skill(&self) -> bool {
        let count = self.skill_counter.fetch_add(1, Ordering::Relaxed) + 1;
        count >= self.config.skill_nudge_interval
    }

    /// Check if a trigger condition is met
    pub fn should_nudge(&self, trigger: &NudgeTrigger) -> bool {
        match trigger {
            NudgeTrigger::Periodic { .. } => self.increment_turn(),
            NudgeTrigger::TaskComplexity { tool_call_count } => {
                *tool_call_count >= self.config.min_tool_calls_for_memory
            }
            NudgeTrigger::ExplicitFeedback { .. } => true,
            NudgeTrigger::ImplicitAdoption => true,
            NudgeTrigger::MemoryToolInvoked => {
                self.reset_turn_counter();
                false // Nudge already handled by memory tool
            }
        }
    }

    /// Execute memory nudge: evaluate candidates and write worthy ones
    pub async fn execute_memory_nudge(
        &self,
        candidates: Vec<MemoryCandidate>,
        existing_memories: &[SearchResult],
        memory_system: &Arc<dyn MemorySearch>,
    ) -> Result<NudgeResult> {
        let mut accepted = Vec::new();
        let mut rejected = Vec::new();

        for candidate in candidates {
            let score = self.evaluator.evaluate(&candidate, existing_memories);

            if score >= self.config.quality_threshold {
                // Determine layer based on category
                let metadata = {
                    let mut m = HashMap::new();
                    m.insert("category".to_string(), candidate.category.clone());
                    m.insert("source_id".to_string(), candidate.source_id.clone());
                    m.insert("quality_score".to_string(), format!("{:.2}", score));
                    m
                };

                // Write to appropriate layer
                match candidate.category.as_str() {
                    "user_preference" | "communication_style" => {
                        // L2: user profile
                        self.writer
                            .write_l3(memory_system, &candidate.content, metadata)
                            .await?;
                    }
                    "project_fact" | "pitfall" | "workflow" | "environment" => {
                        // L1: project memory
                        self.writer
                            .write_l3(memory_system, &candidate.content, metadata)
                            .await?;
                    }
                    _ => {
                        // L3: full history
                        self.writer
                            .write_l3(memory_system, &candidate.content, metadata)
                            .await?;
                    }
                }

                accepted.push((candidate, score));
            } else {
                rejected.push((candidate, score));
            }
        }

        Ok(NudgeResult { accepted, rejected })
    }

    /// Extract memory candidates from a ToolTrail
    pub fn extract_candidates_from_trail(&self, trail: &ToolTrail) -> Vec<MemoryCandidate> {
        let mut candidates = Vec::new();

        // Only process successful trails
        if !matches!(trail.status, TrailStatus::Success) {
            return candidates;
        }

        // Count tool calls
        let tool_call_count: usize = trail.steps.iter().map(|s| s.tool_calls.len()).sum();

        if tool_call_count < self.config.min_tool_calls_for_memory {
            return candidates;
        }

        // Extract key tool calls as workflow memory
        let workflow_desc: Vec<String> = trail
            .steps
            .iter()
            .flat_map(|s| s.tool_calls.iter())
            .map(|tc| {
                format!(
                    "{}: {}",
                    tc.tool_name,
                    tc.result_summary.chars().take(60).collect::<String>()
                )
            })
            .collect();

        if !workflow_desc.is_empty() {
            candidates.push(MemoryCandidate {
                content: format!(
                    "成功工作流 ({} steps): {}",
                    tool_call_count,
                    workflow_desc.join(" → ")
                ),
                category: "workflow".to_string(),
                is_stable_fact: true,
                cross_session_value: true,
                has_user_confirmation: false,
                embedding: Vec::new(), // populated by caller if available
                source_id: trail.plan_id.clone(),
            });
        }

        candidates
    }
}

/// Result of a nudge execution
#[derive(Debug, Clone)]
pub struct NudgeResult {
    /// Accepted candidates with scores
    pub accepted: Vec<(MemoryCandidate, f32)>,
    /// Rejected candidates with scores
    pub rejected: Vec<(MemoryCandidate, f32)>,
}

impl NudgeResult {
    pub fn accepted_count(&self) -> usize {
        self.accepted.len()
    }

    pub fn rejected_count(&self) -> usize {
        self.rejected.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_counter() {
        let engine = NudgeEngine::new(
            NudgeConfig {
                memory_nudge_interval: 3,
                ..Default::default()
            },
            MemoryWriter::new("/tmp"),
        );
        assert!(!engine.increment_turn()); // 1
        assert!(!engine.increment_turn()); // 2
        assert!(engine.increment_turn()); // 3 >= interval
    }

    #[test]
    fn test_should_nudge_complexity() {
        let engine = NudgeEngine::new(
            NudgeConfig {
                min_tool_calls_for_memory: 5,
                ..Default::default()
            },
            MemoryWriter::new("/tmp"),
        );
        assert!(engine.should_nudge(&NudgeTrigger::TaskComplexity { tool_call_count: 5 }));
        assert!(!engine.should_nudge(&NudgeTrigger::TaskComplexity { tool_call_count: 3 }));
    }

    #[test]
    fn test_reset_counter() {
        let engine = NudgeEngine::new(
            NudgeConfig {
                memory_nudge_interval: 2,
                ..Default::default()
            },
            MemoryWriter::new("/tmp"),
        );
        engine.increment_turn(); // 1
        engine.reset_turn_counter();
        assert!(!engine.increment_turn()); // back to 1
    }
}
