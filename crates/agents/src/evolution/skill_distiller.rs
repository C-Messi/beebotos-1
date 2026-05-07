//! Skill Distiller — Automatic Skill Extraction from Tool Trails
//!
//! Converts successful task execution trails into reusable SKILL.md artifacts.
//! Pipeline: trail ingestion → sanitization → abstraction → validation → output.

use crate::error::Result;
use crate::planning::{ToolTrail, TrailStatus, ToolCallRecord};
use crate::skills::registry::RegisteredSkill;

/// Trigger conditions for skill distillation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DistillTrigger {
    /// Tool call count exceeds threshold
    ToolCallThreshold { count: usize },
    /// Task succeeded after self-healing (error → recovery)
    SelfHealed,
    /// Explicit user confirmation
    UserConfirmed { feedback: String },
    /// Implicit adoption (user accepted without modification)
    ImplicitAdoption,
    /// Novel workflow (low similarity to existing skills)
    NovelWorkflow,
}

/// Configuration for skill distillation
#[derive(Debug, Clone)]
pub struct DistillerConfig {
    /// Minimum tool calls to trigger distillation
    pub min_tool_calls: usize,
    /// Minimum quality score (0-10) to accept generated skill
    pub min_quality_score: f32,
    /// Maximum similarity to existing skill before treating as patch
    pub patch_similarity_threshold: f32,
    /// Minimum similarity to treat as duplicate (skip creation)
    pub duplicate_similarity_threshold: f32,
    /// Max output size for a single skill (characters)
    pub max_skill_chars: usize,
}

impl Default for DistillerConfig {
    fn default() -> Self {
        Self {
            min_tool_calls: 5,
            min_quality_score: 6.0,
            patch_similarity_threshold: 0.7,
            duplicate_similarity_threshold: 0.3,
            max_skill_chars: 5000,
        }
    }
}

/// Sanitized workflow step extracted from a trail
#[derive(Debug, Clone)]
pub struct WorkflowStep {
    pub step_number: usize,
    pub tool_name: String,
    pub description: String,
    pub params_template: String,
    pub validation_hint: String,
}

/// Extracted decision points (if/then/else branches observed in trail)
#[derive(Debug, Clone)]
pub struct DecisionPoint {
    pub condition: String,
    pub if_branch: String,
    pub else_branch: Option<String>,
}

/// Pitfall observed in the trail (error + recovery)
#[derive(Debug, Clone)]
pub struct Pitfall {
    pub error_description: String,
    pub recovery_action: String,
}

/// Intermediate representation of a distilled skill
#[derive(Debug, Clone)]
pub struct DistilledSkill {
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub applicable_scenario: String,
    pub steps: Vec<WorkflowStep>,
    pub decisions: Vec<DecisionPoint>,
    pub pitfalls: Vec<Pitfall>,
    pub quality_score: f32,
    pub source_trail_id: String,
}

/// Skill distiller engine
#[derive(Debug, Clone)]
pub struct SkillDistiller {
    pub config: DistillerConfig,
}

impl SkillDistiller {
    pub fn new(config: DistillerConfig) -> Self {
        Self { config }
    }

    /// Check if a trail should trigger skill distillation
    pub fn should_distill(&self, trail: &ToolTrail, trigger: &DistillTrigger) -> bool {
        if !matches!(trail.status, TrailStatus::Success) {
            return false;
        }

        let tool_call_count: usize = trail.steps.iter()
            .map(|s| s.tool_calls.len())
            .sum();

        match trigger {
            DistillTrigger::ToolCallThreshold { count } => tool_call_count >= *count,
            DistillTrigger::SelfHealed => self.detect_self_healing(trail),
            DistillTrigger::UserConfirmed { .. } => true,
            DistillTrigger::ImplicitAdoption => true,
            DistillTrigger::NovelWorkflow => tool_call_count >= self.config.min_tool_calls,
        }
    }

    /// Detect if trail contains error → recovery pattern
    fn detect_self_healing(&self, trail: &ToolTrail) -> bool {
        let mut had_failure = false;
        for step in &trail.steps {
            for call in &step.tool_calls {
                if !call.success {
                    had_failure = true;
                } else if had_failure {
                    // Success after failure = recovery
                    return true;
                }
            }
        }
        false
    }

    /// Main pipeline: ToolTrail → DistilledSkill
    pub fn distill(&self, trail: &ToolTrail) -> Result<DistilledSkill> {
        let steps = self.extract_steps(trail);
        let decisions = self.extract_decisions(trail);
        let pitfalls = self.extract_pitfalls(trail);

        let scenario = self.infer_scenario(&steps);
        let skill_id = format!("auto-{}", trail.plan_id);

        // Compute quality score (0-10)
        let quality = self.compute_quality(&steps, &pitfalls, trail);

        Ok(DistilledSkill {
            skill_id,
            name: scenario.clone(),
            description: format!("Auto-distilled skill for: {}", scenario),
            applicable_scenario: scenario,
            steps,
            decisions,
            pitfalls,
            quality_score: quality,
            source_trail_id: trail.plan_id.clone(),
        })
    }

    fn extract_steps(&self, trail: &ToolTrail) -> Vec<WorkflowStep> {
        let mut steps = Vec::new();
        let mut step_num = 1;

        for trail_step in &trail.steps {
            for call in &trail_step.tool_calls {
                // Generalize parameters: replace specific values with placeholders
                let params_template = generalize_params(&call.parameters);

                steps.push(WorkflowStep {
                    step_number: step_num,
                    tool_name: call.tool_name.clone(),
                    description: format!("{}: {}", call.tool_name, call.result_summary.chars().take(80).collect::<String>()),
                    params_template,
                    validation_hint: self.infer_validation(&call),
                });
                step_num += 1;
            }
        }

        steps
    }

    fn extract_decisions(&self, _trail: &ToolTrail) -> Vec<DecisionPoint> {
        // TODO: In full implementation, use LLM to analyze branch patterns
        // For now, return empty (decisions are extracted by downstream CAPO)
        Vec::new()
    }

    fn extract_pitfalls(&self, trail: &ToolTrail) -> Vec<Pitfall> {
        let mut pitfalls = Vec::new();
        let mut last_failure: Option<String> = None;

        for step in &trail.steps {
            for call in &step.tool_calls {
                if !call.success {
                    last_failure = Some(format!("{} failed: {}", call.tool_name, call.result_summary));
                } else if let Some(ref failure) = last_failure {
                    pitfalls.push(Pitfall {
                        error_description: failure.clone(),
                        recovery_action: format!("Retry with {}", call.tool_name),
                    });
                    last_failure = None;
                }
            }
        }

        pitfalls
    }

    fn infer_scenario(&self, steps: &[WorkflowStep]) -> String {
        if steps.is_empty() {
            return "unknown_workflow".to_string();
        }

        let tool_names: Vec<String> = steps.iter()
            .map(|s| s.tool_name.clone())
            .collect();

        // Simple heuristic: name based on first and last tool
        format!("{}_to_{}_workflow", tool_names.first().unwrap(), tool_names.last().unwrap())
    }

    fn infer_validation(&self, call: &ToolCallRecord) -> String {
        if call.result_summary.contains("status") || call.result_summary.contains("ok") {
            "Check status field is 'ok'".to_string()
        } else if call.result_summary.contains("error") {
            "Verify no error in response".to_string()
        } else {
            "Result is non-empty".to_string()
        }
    }

    fn compute_quality(&self, steps: &[WorkflowStep], pitfalls: &[Pitfall], trail: &ToolTrail) -> f32 {
        let mut score = 5.0; // baseline

        // +1 for each step (up to +3)
        score += (steps.len() as f32 * 0.5).min(3.0);

        // +1 for each pitfall documented (recovery knowledge is valuable)
        score += (pitfalls.len() as f32 * 0.5).min(2.0);

        // -2 for very short trails
        if steps.len() < 3 {
            score -= 2.0;
        }

        // Bonus for self-healing
        if self.detect_self_healing(trail) {
            score += 1.5;
        }

        score.clamp(0.0, 10.0)
    }

    /// Compare distilled skill to existing skills for patch/deduplicate decision
    pub fn compare_to_existing(
        &self,
        distilled: &DistilledSkill,
        existing: &[RegisteredSkill],
    ) -> DistillDecision {
        if existing.is_empty() {
            return DistillDecision::CreateNew;
        }

        // Simple text similarity: compare applicable_scenario to existing skill names/descriptions
        let mut best_match: Option<(String, f32)> = None;

        for skill in existing {
            let existing_text = format!("{} {}", skill.skill.name, skill.skill.manifest.description);
            let sim = text_similarity(&distilled.applicable_scenario, &existing_text);

            if best_match.as_ref().map(|(_, s)| sim > *s).unwrap_or(true) {
                best_match = Some((skill.skill.id.clone(), sim));
            }
        }

        if let Some((skill_id, similarity)) = best_match {
            if similarity >= self.config.patch_similarity_threshold {
                DistillDecision::PatchExisting { skill_id }
            } else if similarity >= self.config.duplicate_similarity_threshold {
                DistillDecision::UpdateExisting { skill_id }
            } else {
                DistillDecision::CreateNew
            }
        } else {
            DistillDecision::CreateNew
        }
    }

    /// Generate SKILL.md content from distilled skill
    pub fn to_skill_markdown(&self, distilled: &DistilledSkill) -> String {
        let mut md = String::new();

        md.push_str(&format!("# Skill: {}\n\n", distilled.skill_id));
        md.push_str("## 元信息\n");
        md.push_str(&format!("- **version**: 1.0.0\n"));
        md.push_str(&format!("- **auto_generated**: true\n"));
        md.push_str(&format!("- **quality_score**: {:.1}\n", distilled.quality_score));
        md.push_str(&format!("- **source_trail**: {}\n", distilled.source_trail_id));
        md.push_str("\n");

        md.push_str("## 适用场景\n");
        md.push_str(&format!("{}\n\n", distilled.applicable_scenario));

        md.push_str("## 执行步骤\n");
        for step in &distilled.steps {
            md.push_str(&format!(
                "{}. {}\n   - 工具: `{}`\n   - 参数模板: `{}`\n   - 验证: {}\n",
                step.step_number, step.description, step.tool_name, step.params_template, step.validation_hint
            ));
        }
        md.push_str("\n");

        if !distilled.decisions.is_empty() {
            md.push_str("## 关键决策点\n");
            for d in &distilled.decisions {
                md.push_str(&format!("- **IF** {} **THEN** {}\n", d.condition, d.if_branch));
            }
            md.push_str("\n");
        }

        if !distilled.pitfalls.is_empty() {
            md.push_str("## 已知陷阱 (Pitfalls)\n");
            for p in &distilled.pitfalls {
                md.push_str(&format!("- **{}**: {}\n", p.error_description, p.recovery_action));
            }
            md.push_str("\n");
        }

        md.push_str("## 谱系历史\n");
        md.push_str(&format!("- v1.0.0: 自动从轨迹 {} 提炼\n", distilled.source_trail_id));

        md
    }
}

/// Decision on what to do with a distilled skill
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DistillDecision {
    /// Create entirely new skill
    CreateNew,
    /// Patch existing skill with delta
    PatchExisting { skill_id: String },
    /// Update existing skill (significant change)
    UpdateExisting { skill_id: String },
}

/// Generalize concrete parameter values into placeholders
fn generalize_params(params: &serde_json::Value) -> String {
    match params {
        serde_json::Value::Object(map) => {
            let mut generalized = serde_json::Map::new();
            for (key, value) in map {
                let generalized_value = match value {
                    serde_json::Value::String(s) => {
                        if looks_like_symbol(s) {
                            serde_json::json!(format!("{{{}}}", key))
                        } else if looks_like_path(s) {
                            serde_json::json!(format!("{{project_root}}/{}", std::path::Path::new(s).file_name().map(|f| f.to_string_lossy()).unwrap_or_default()))
                        } else {
                            serde_json::json!(format!("{{{}}}", key))
                        }
                    }
                    _ => value.clone(),
                };
                generalized.insert(key.clone(), generalized_value);
            }
            serde_json::Value::Object(generalized).to_string()
        }
        _ => params.to_string(),
    }
}

fn looks_like_symbol(s: &str) -> bool {
    s.chars().all(|c| c.is_alphanumeric() || c == '/' || c == '-' || c == '_')
        && s.len() <= 10
        && s.to_uppercase() == s.to_uppercase() // not all lowercase words
}

fn looks_like_path(s: &str) -> bool {
    s.contains('/') || s.contains('\\') || s.starts_with("./") || s.starts_with("../")
}

/// Simple text similarity: Jaccard index on word sets
fn text_similarity(a: &str, b: &str) -> f32 {
    let words_a: std::collections::HashSet<String> = a.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(|w| w.to_string())
        .collect();
    let words_b: std::collections::HashSet<String> = b.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(|w| w.to_string())
        .collect();

    if words_a.is_empty() || words_b.is_empty() {
        return 0.0;
    }

    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();

    intersection as f32 / union as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distill_decision_thresholds() {
        let distiller = SkillDistiller::new(DistillerConfig::default());
        let dummy = DistilledSkill {
            skill_id: "test".to_string(),
            name: "test".to_string(),
            description: "test".to_string(),
            applicable_scenario: "deploy flask app".to_string(),
            steps: vec![],
            decisions: vec![],
            pitfalls: vec![],
            quality_score: 7.0,
            source_trail_id: "trail-1".to_string(),
        };

        // No existing skills → CreateNew
        assert_eq!(distiller.compare_to_existing(&dummy, &[]), DistillDecision::CreateNew);
    }

    #[test]
    fn test_generalize_params() {
        let params = serde_json::json!({
            "symbol": "AAPL",
            "path": "/home/user/project/main.py",
            "count": 10
        });
        let generalized = generalize_params(&params);
        assert!(generalized.contains("{symbol}"));
        assert!(generalized.contains("{project_root}")); // paths get project_root prefix
    }

    #[test]
    fn test_text_similarity() {
        let sim = text_similarity("deploy flask to kubernetes", "flask k8s deployment");
        assert!(sim > 0.15, "Similar texts should have similarity > 0.15, got {}", sim);

        let diff = text_similarity("deploy flask", "analyze stock data");
        assert!(diff < 0.15, "Different texts should have similarity < 0.15, got {}", diff);
    }

    #[test]
    fn test_compute_quality() {
        let distiller = SkillDistiller::new(DistillerConfig::default());
        let trail = ToolTrail::new("test".to_string());
        let steps = vec![
            WorkflowStep { step_number: 1, tool_name: "fetch".to_string(), description: "Fetch data".to_string(), params_template: "{}".to_string(), validation_hint: "".to_string() },
            WorkflowStep { step_number: 2, tool_name: "process".to_string(), description: "Process".to_string(), params_template: "{}".to_string(), validation_hint: "".to_string() },
            WorkflowStep { step_number: 3, tool_name: "save".to_string(), description: "Save".to_string(), params_template: "{}".to_string(), validation_hint: "".to_string() },
        ];
        let quality = distiller.compute_quality(&steps, &[], &trail);
        assert!(quality >= 5.0, "3-step workflow should score >= 5.0");
    }
}
