//! Skill Feedback Module
//!
//! Provides self-improvement loop for skills:
//! - Collects feedback after skill execution
//! - LLM self-evaluation of execution quality
//! - Aggregates feedback into improvement suggestions

use serde::{Deserialize, Serialize};

/// Feedback collected after a skill execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFeedback {
    pub skill_id: String,
    pub execution_success: bool,
    pub user_satisfaction: Option<f32>,
    pub llm_self_evaluation: String,
    pub suggested_improvements: Vec<String>,
    pub execution_time_ms: u64,
    pub token_cost: u32,
}

/// Skill improvement suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillImprovement {
    pub skill_id: String,
    pub suggestion: String,
    pub priority: ImprovementPriority,
    pub category: ImprovementCategory,
}

/// Priority of improvement
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImprovementPriority {
    Low,
    Medium,
    High,
    Critical,
}

/// Category of improvement
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImprovementCategory {
    Description,
    Parameters,
    Examples,
    ErrorHandling,
    Performance,
}

/// 🆕 PHASE 5: Evolution-specific feedback extending SkillFeedback
///
/// Adds lineage-aware fields for tracking how a skill evolves over time:
/// - version snapshot at execution time
/// - quality delta from previous version
/// - whether the execution triggered a lineage update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEvolutionFeedback {
    pub base: SkillFeedback,
    pub version_at_execution: String,
    pub quality_score: f32,
    pub triggered_distillation: bool,
    pub triggered_patch: bool,
    pub lineage_node_id: Option<String>,
}

/// Skill improvement engine
pub struct SkillImprovementEngine;

impl SkillImprovementEngine {
    pub fn new() -> Self {
        Self
    }

    /// Collect feedback after skill execution
    pub fn collect_feedback(
        &self,
        skill_id: &str,
        execution_success: bool,
        execution_time_ms: u64,
    ) -> SkillFeedback {
        SkillFeedback {
            skill_id: skill_id.to_string(),
            execution_success,
            user_satisfaction: None,
            llm_self_evaluation: String::new(),
            suggested_improvements: Vec::new(),
            execution_time_ms,
            token_cost: 0,
        }
    }

    /// Generate self-evaluation prompt for LLM
    pub fn build_evaluation_prompt(
        &self,
        skill_id: &str,
        input: &str,
        output: &str,
        success: bool,
    ) -> String {
        format!(
            "请评估以下 skill 执行的效果，并给出改进建议。\n\nSkill ID: {}\n输入: {}\n输出: \
             {}\n执行结果: {}\n\n请回答：\n1. 执行是否成功完成了用户请求？\n2. \
             输出质量如何（1-10分）？\n3. 有哪些可以改进的地方？\n4. 是否需要更新 skill \
             的描述或示例？",
            skill_id,
            input.chars().take(200).collect::<String>(),
            output.chars().take(500).collect::<String>(),
            if success { "成功" } else { "失败" }
        )
    }

    /// Aggregate feedback into improvement report
    pub fn aggregate_feedback(&self, feedbacks: &[SkillFeedback]) -> Vec<SkillImprovement> {
        let mut improvements = Vec::new();

        for fb in feedbacks {
            if !fb.execution_success {
                improvements.push(SkillImprovement {
                    skill_id: fb.skill_id.clone(),
                    suggestion: "Improve error handling based on recent failures".to_string(),
                    priority: ImprovementPriority::High,
                    category: ImprovementCategory::ErrorHandling,
                });
            }

            if fb.execution_time_ms > 10000 {
                improvements.push(SkillImprovement {
                    skill_id: fb.skill_id.clone(),
                    suggestion: format!(
                        "Execution time {}ms exceeds threshold, consider optimization",
                        fb.execution_time_ms
                    ),
                    priority: ImprovementPriority::Medium,
                    category: ImprovementCategory::Performance,
                });
            }

            for suggestion in &fb.suggested_improvements {
                improvements.push(SkillImprovement {
                    skill_id: fb.skill_id.clone(),
                    suggestion: suggestion.clone(),
                    priority: ImprovementPriority::Medium,
                    category: ImprovementCategory::Description,
                });
            }
        }

        improvements
    }

    /// Generate markdown report from improvements
    pub fn generate_report(&self, improvements: &[SkillImprovement]) -> String {
        let mut report = String::from("# Skill Improvement Report\n\n");

        for imp in improvements {
            let priority_str = match imp.priority {
                ImprovementPriority::Low => "🟢 Low",
                ImprovementPriority::Medium => "🟡 Medium",
                ImprovementPriority::High => "🔴 High",
                ImprovementPriority::Critical => "🚨 Critical",
            };

            report.push_str(&format!(
                "## {}\n- **Skill**: `{}`\n- **Priority**: {}\n- **Category**: {:?}\n- \
                 **Suggestion**: {}\n\n",
                imp.skill_id, imp.skill_id, priority_str, imp.category, imp.suggestion
            ));
        }

        report
    }

    /// 🆕 OPTIMIZATION PHASE 3: Generate structured skill update proposals from
    /// feedback
    ///
    /// Converts aggregated improvements into actionable skill update
    /// instructions that can be fed into the PatchEngine or SkillDistiller.
    pub fn generate_skill_updates(
        &self,
        skill_id: &str,
        feedbacks: &[SkillFeedback],
    ) -> Vec<SkillUpdateProposal> {
        let mut proposals = Vec::new();
        let mut description_changes = Vec::new();
        let mut example_changes = Vec::new();
        let mut parameter_changes = Vec::new();
        let mut error_patterns = Vec::new();

        for fb in feedbacks {
            if fb.skill_id != skill_id {
                continue;
            }
            // Collect failure patterns for error handling improvements
            if !fb.execution_success {
                error_patterns.push(format!("Execution failed ({}ms)", fb.execution_time_ms));
            }
            // Collect user-suggested improvements
            for suggestion in &fb.suggested_improvements {
                let lower = suggestion.to_lowercase();
                if lower.contains("example") || lower.contains("示例") {
                    example_changes.push(suggestion.clone());
                } else if lower.contains("parameter") || lower.contains("参数") {
                    parameter_changes.push(suggestion.clone());
                } else {
                    description_changes.push(suggestion.clone());
                }
            }
            // Parse LLM self-evaluation for additional hints
            if !fb.llm_self_evaluation.is_empty() {
                description_changes.push(format!("[LLM评估] {}", fb.llm_self_evaluation));
            }
        }

        // Build proposals by category
        if !description_changes.is_empty() {
            proposals.push(SkillUpdateProposal {
                skill_id: skill_id.to_string(),
                category: ImprovementCategory::Description,
                changes: description_changes,
                reason: "Aggregated user and LLM feedback".to_string(),
            });
        }
        if !example_changes.is_empty() {
            proposals.push(SkillUpdateProposal {
                skill_id: skill_id.to_string(),
                category: ImprovementCategory::Examples,
                changes: example_changes,
                reason: "User requests for better examples".to_string(),
            });
        }
        if !parameter_changes.is_empty() {
            proposals.push(SkillUpdateProposal {
                skill_id: skill_id.to_string(),
                category: ImprovementCategory::Parameters,
                changes: parameter_changes,
                reason: "User requests for parameter clarity".to_string(),
            });
        }
        if !error_patterns.is_empty() {
            proposals.push(SkillUpdateProposal {
                skill_id: skill_id.to_string(),
                category: ImprovementCategory::ErrorHandling,
                changes: error_patterns,
                reason: "Recurring execution failures".to_string(),
            });
        }

        proposals
    }
}

/// 🆕 OPTIMIZATION PHASE 3: Structured proposal for updating a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillUpdateProposal {
    pub skill_id: String,
    pub category: ImprovementCategory,
    pub changes: Vec<String>,
    pub reason: String,
}

impl Default for SkillImprovementEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_feedback() {
        let engine = SkillImprovementEngine::new();
        let fb = engine.collect_feedback("test_skill", true, 1500);
        assert_eq!(fb.skill_id, "test_skill");
        assert!(fb.execution_success);
    }

    #[test]
    fn test_aggregate_feedback() {
        let engine = SkillImprovementEngine::new();
        let feedbacks = vec![
            SkillFeedback {
                skill_id: "skill1".to_string(),
                execution_success: false,
                user_satisfaction: None,
                llm_self_evaluation: String::new(),
                suggested_improvements: vec!["Add more examples".to_string()],
                execution_time_ms: 500,
                token_cost: 100,
            },
            SkillFeedback {
                skill_id: "skill2".to_string(),
                execution_success: true,
                user_satisfaction: Some(0.8),
                llm_self_evaluation: String::new(),
                suggested_improvements: Vec::new(),
                execution_time_ms: 15000,
                token_cost: 200,
            },
        ];

        let improvements = engine.aggregate_feedback(&feedbacks);
        assert_eq!(improvements.len(), 3); // 1 failure + 1 slow + 1 suggestion
    }
}
