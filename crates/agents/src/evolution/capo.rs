//! CAPO — Context-Aware Prompt Optimization
//!
//! Symbol-level evolution engine that optimizes prompts, skill docs, and
//! SOUL.md by analyzing success/failure trajectories and making directed edits.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::error::AgentError;
use crate::planning::ToolTrail;

/// Context-aware scorer for evaluating document versions
#[derive(Debug, Clone)]
pub struct ContextScorer {
    pub success_weight: f32,
    pub token_efficiency_weight: f32,
    pub satisfaction_weight: f32,
    pub latency_weight: f32,
}

impl Default for ContextScorer {
    fn default() -> Self {
        Self {
            success_weight: 0.6,
            token_efficiency_weight: 0.2,
            satisfaction_weight: 0.1,
            latency_weight: 0.1,
        }
    }
}

impl ContextScorer {
    /// Score a version based on multi-dimensional metrics
    pub fn score(&self, metrics: &VersionMetrics) -> f32 {
        let success_score = metrics.success_rate;
        let token_score = metrics.avg_token_efficiency;
        let satisfaction_score = metrics.avg_user_satisfaction.unwrap_or(0.5);
        // Lower latency is better → invert and normalize
        let latency_score = if metrics.avg_latency_ms > 0.0 {
            (5000.0 / metrics.avg_latency_ms).min(1.0) as f32
        } else {
            0.5
        };

        success_score * self.success_weight
            + token_score * self.token_efficiency_weight
            + satisfaction_score * self.satisfaction_weight
            + latency_score * self.latency_weight
    }
}

/// CAPO optimization configuration
#[derive(Debug, Clone)]
pub struct CapoConfig {
    /// Maximum optimization iterations
    pub max_iterations: usize,
    /// Performance improvement threshold to adopt a change (0-1)
    pub improvement_threshold: f32,
    /// Edit temperature: 0.0 = conservative, 1.0 = creative
    pub edit_temperature: f32,
    /// Number of historical versions to keep for rollback
    pub rollback_depth: usize,
    /// Minimum trajectories needed for analysis
    pub min_trajectories: usize,
    /// Top-K problem paragraphs to target per iteration
    pub top_k_problems: usize,
}

impl Default for CapoConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            improvement_threshold: 0.05,
            edit_temperature: 0.3,
            rollback_depth: 5,
            min_trajectories: 5,
            top_k_problems: 3,
        }
    }
}

/// A directed edit operation on a document
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp {
    /// Replace a paragraph with new text
    Rewrite {
        paragraph_id: String,
        new_text: String,
    },
    /// Add conditional branch or example after a paragraph
    Augment {
        paragraph_id: String,
        addition: String,
    },
    /// Remove a paragraph
    Prune { paragraph_id: String },
    /// Reorder paragraphs
    Reorder { paragraph_ids: Vec<String> },
}

/// Context attribution: how much a paragraph influenced outcomes
#[derive(Debug, Clone)]
pub struct AttributionScore {
    pub paragraph_id: String,
    pub paragraph_text: String,
    pub success_correlation: f32, // +1.0 = strongly associated with success
    pub failure_correlation: f32, // +1.0 = strongly associated with failure
    pub overall_impact: f32,      // composite score
}

/// Performance metrics for a document version
#[derive(Debug, Clone, Default)]
pub struct VersionMetrics {
    pub success_rate: f32,
    pub avg_token_efficiency: f32, // output tokens / total tokens
    pub avg_latency_ms: f64,
    pub trajectory_count: usize,
    pub avg_user_satisfaction: Option<f32>,
}

/// A version of a document under optimization
#[derive(Debug, Clone)]
pub struct DocumentVersion {
    pub version_id: String,
    pub content: String,
    pub metrics: VersionMetrics,
    pub parent_id: Option<String>,
    pub applied_edit: Option<EditOp>,
}

/// CAPO evolution engine
#[derive(Debug, Clone)]
pub struct CapoEngine {
    config: CapoConfig,
    /// History of versions for rollback
    version_history: Arc<RwLock<Vec<DocumentVersion>>>,
}

impl CapoEngine {
    pub fn new(config: CapoConfig) -> Self {
        Self {
            config,
            version_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Step 1: Context attribution analysis
    /// Maps each paragraph to its correlation with success/failure
    pub fn analyze_attribution(
        &self,
        document: &str,
        trajectories: &[(ToolTrail, bool)], // (trail, success)
    ) -> Vec<AttributionScore> {
        let paragraphs: Vec<(String, String)> = document
            .split("\n\n")
            .enumerate()
            .map(|(i, text)| (format!("p{}", i), text.trim().to_string()))
            .filter(|(_, text)| !text.is_empty())
            .collect();

        let mut scores = Vec::new();

        for (pid, text) in &paragraphs {
            let mut success_mentions = 0usize;
            let mut failure_mentions = 0usize;

            for (trail, success) in trajectories {
                // Heuristic: if trail tools/steps contain keywords from paragraph
                let trail_text = trail.to_json().to_lowercase();
                let para_keywords: Vec<String> = text
                    .to_lowercase()
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| w.len() >= 4)
                    .map(|w| w.to_string())
                    .collect();

                let keyword_hits = para_keywords
                    .iter()
                    .filter(|k| trail_text.contains(*k))
                    .count();
                let relevance = if para_keywords.is_empty() {
                    0.0
                } else {
                    keyword_hits as f32 / para_keywords.len() as f32
                };

                if *success {
                    if relevance > 0.3 {
                        success_mentions += 1;
                    }
                } else {
                    if relevance > 0.3 {
                        failure_mentions += 1;
                    }
                }
            }

            let total = trajectories.len().max(1);
            let success_corr = success_mentions as f32 / total as f32;
            let failure_corr = failure_mentions as f32 / total as f32;
            let impact = success_corr - failure_corr;

            scores.push(AttributionScore {
                paragraph_id: pid.clone(),
                paragraph_text: text.clone(),
                success_correlation: success_corr,
                failure_correlation: failure_corr,
                overall_impact: impact,
            });
        }

        // Sort by absolute impact (descending)
        scores.sort_by(|a, b| {
            b.overall_impact
                .abs()
                .partial_cmp(&a.overall_impact.abs())
                .unwrap()
        });
        scores
    }

    /// Step 2: Locate top-K problem paragraphs (negative impact) and high-value
    /// paragraphs (positive)
    pub fn locate_problems(
        &self,
        scores: &[AttributionScore],
    ) -> (Vec<AttributionScore>, Vec<AttributionScore>) {
        let problems: Vec<AttributionScore> = scores
            .iter()
            .filter(|s| s.overall_impact < -0.05)
            .take(self.config.top_k_problems)
            .cloned()
            .collect();

        let high_value: Vec<AttributionScore> = scores
            .iter()
            .filter(|s| s.overall_impact > 0.1)
            .cloned()
            .collect();

        (problems, high_value)
    }

    /// Step 3: Generate directed edits for problem paragraphs
    pub fn generate_edits(
        &self,
        _document: &str,
        problem: &AttributionScore,
        _high_value: &[AttributionScore],
    ) -> Vec<EditOp> {
        let mut edits = Vec::new();

        // Strategy based on failure correlation
        if problem.failure_correlation > 0.5 {
            // Strong failure association → rewrite or prune
            if self.config.edit_temperature > 0.5 {
                edits.push(EditOp::Prune {
                    paragraph_id: problem.paragraph_id.clone(),
                });
            } else {
                edits.push(EditOp::Rewrite {
                    paragraph_id: problem.paragraph_id.clone(),
                    new_text: format!(
                        "[REVISED] {}\n\n注意：此段指令已根据执行轨迹优化。",
                        problem.paragraph_text
                    ),
                });
            }
        } else {
            // Moderate issue → augment with conditions
            edits.push(EditOp::Augment {
                paragraph_id: problem.paragraph_id.clone(),
                addition: format!("\n\n> 补充条件：如果上述步骤失败，请先检查环境状态，然后重试。"),
            });
        }

        edits
    }

    /// Step 4: Lightweight evaluation — compute metrics from trajectories
    pub fn evaluate_version(
        &self,
        _document: &str,
        trajectories: &[(ToolTrail, bool)],
    ) -> VersionMetrics {
        let total = trajectories.len().max(1);
        let successes = trajectories.iter().filter(|(_, s)| *s).count();

        VersionMetrics {
            success_rate: successes as f32 / total as f32,
            avg_token_efficiency: 0.5, // placeholder
            avg_latency_ms: 1000.0,    // placeholder
            trajectory_count: total,
            avg_user_satisfaction: None,
        }
    }

    /// Step 5: Apply a single edit and return new document content
    pub fn apply_edit(&self, document: &str, edit: &EditOp) -> String {
        let paragraphs: Vec<String> = document.split("\n\n").map(|s| s.to_string()).collect();

        match edit {
            EditOp::Rewrite {
                paragraph_id,
                new_text,
            } => {
                let idx = paragraph_id
                    .strip_prefix('p')
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(0);
                let mut new_paras = paragraphs.clone();
                if idx < new_paras.len() {
                    new_paras[idx] = new_text.clone();
                }
                new_paras.join("\n\n")
            }
            EditOp::Augment {
                paragraph_id,
                addition,
            } => {
                let idx = paragraph_id
                    .strip_prefix('p')
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(0);
                let mut new_paras = paragraphs.clone();
                if idx < new_paras.len() {
                    new_paras[idx] = format!("{}{}", new_paras[idx], addition);
                }
                new_paras.join("\n\n")
            }
            EditOp::Prune { paragraph_id } => {
                let idx = paragraph_id
                    .strip_prefix('p')
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(0);
                let mut new_paras = paragraphs;
                if idx < new_paras.len() {
                    new_paras.remove(idx);
                }
                new_paras.join("\n\n")
            }
            EditOp::Reorder { paragraph_ids } => {
                let mut reordered = Vec::new();
                for pid in paragraph_ids {
                    let idx = pid
                        .strip_prefix('p')
                        .and_then(|n| n.parse::<usize>().ok())
                        .unwrap_or(0);
                    if idx < paragraphs.len() {
                        reordered.push(paragraphs[idx].clone());
                    }
                }
                // Append any paragraphs not in the reorder list
                for (i, p) in paragraphs.iter().enumerate() {
                    let expected_id = format!("p{}", i);
                    if !paragraph_ids.contains(&expected_id) {
                        reordered.push(p.clone());
                    }
                }
                reordered.join("\n\n")
            }
        }
    }

    /// Run one optimization iteration
    pub async fn optimize_once(
        &self,
        document: &str,
        trajectories: &[(ToolTrail, bool)],
    ) -> Result<(String, VersionMetrics, Vec<EditOp>), AgentError> {
        if trajectories.len() < self.config.min_trajectories {
            return Err(AgentError::Execution(format!(
                "Need {} trajectories, got {}",
                self.config.min_trajectories,
                trajectories.len()
            )));
        }

        // 1. Attribution
        let scores = self.analyze_attribution(document, trajectories);

        // 2. Locate problems
        let (problems, high_value) = self.locate_problems(&scores);

        if problems.is_empty() {
            return Err(AgentError::Execution(
                "No problem paragraphs found".to_string(),
            ));
        }

        // 3. Generate edits for the worst problem
        let edits = self.generate_edits(document, &problems[0], &high_value);

        // Apply first edit
        let new_doc = self.apply_edit(document, &edits[0]);

        // 4. Evaluate
        let metrics = self.evaluate_version(&new_doc, trajectories);

        Ok((new_doc, metrics, edits))
    }

    /// Full CAPO pipeline: iterate until improvement threshold or max
    /// iterations
    pub async fn evolve(
        &self,
        initial_document: &str,
        trajectories: &[(ToolTrail, bool)],
    ) -> Result<EvolutionResult, AgentError> {
        let mut current_doc = initial_document.to_string();
        let baseline_metrics = self.evaluate_version(&current_doc, trajectories);
        let mut best_doc = current_doc.clone();
        let mut best_metrics = baseline_metrics.clone();
        let mut history = self.version_history.write().await;

        history.push(DocumentVersion {
            version_id: "v0".to_string(),
            content: current_doc.clone(),
            metrics: baseline_metrics.clone(),
            parent_id: None,
            applied_edit: None,
        });

        let mut all_edits = Vec::new();

        for i in 0..self.config.max_iterations {
            match self.optimize_once(&current_doc, trajectories).await {
                Ok((new_doc, metrics, edits)) => {
                    let improvement = metrics.success_rate - baseline_metrics.success_rate;

                    history.push(DocumentVersion {
                        version_id: format!("v{}", i + 1),
                        content: new_doc.clone(),
                        metrics: metrics.clone(),
                        parent_id: Some(format!("v{}", i)),
                        applied_edit: edits.first().cloned(),
                    });

                    all_edits.extend(edits);

                    if improvement >= self.config.improvement_threshold {
                        if metrics.success_rate > best_metrics.success_rate {
                            best_doc = new_doc.clone();
                            best_metrics = metrics.clone();
                        }
                        current_doc = new_doc;
                    } else {
                        // No improvement → rollback and try next problem
                        break;
                    }
                }
                Err(e) => {
                    return Ok(EvolutionResult {
                        final_document: best_doc,
                        final_metrics: best_metrics,
                        iterations_run: i,
                        edits_applied: all_edits,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        // Trim history to rollback depth
        while history.len() > self.config.rollback_depth {
            history.remove(0);
        }

        Ok(EvolutionResult {
            final_document: best_doc,
            final_metrics: best_metrics,
            iterations_run: history.len().saturating_sub(1),
            edits_applied: all_edits,
            error: None,
        })
    }

    /// Rollback to a specific version
    pub async fn rollback_to(&self, version_id: &str) -> Option<String> {
        let history = self.version_history.read().await;
        history
            .iter()
            .find(|v| v.version_id == version_id)
            .map(|v| v.content.clone())
    }
}

/// Result of a CAPO evolution run
#[derive(Debug, Clone)]
pub struct EvolutionResult {
    pub final_document: String,
    pub final_metrics: VersionMetrics,
    pub iterations_run: usize,
    pub edits_applied: Vec<EditOp>,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::ToolTrail;

    fn make_trail(success: bool) -> (ToolTrail, bool) {
        let mut trail = ToolTrail::new("test".to_string());
        trail.finish(if success {
            crate::planning::TrailStatus::Success
        } else {
            crate::planning::TrailStatus::Failed
        });
        (trail, success)
    }

    #[test]
    fn test_attribution_analysis() {
        let engine = CapoEngine::new(CapoConfig::default());
        let doc = "Always check environment first.\n\nIgnore errors and continue.\n\nValidate all \
                   inputs.";
        let trajectories = vec![
            make_trail(true),
            make_trail(true),
            make_trail(false),
            make_trail(false),
            make_trail(false),
        ];

        let scores = engine.analyze_attribution(doc, &trajectories);
        assert!(!scores.is_empty());
    }

    #[test]
    fn test_apply_rewrite() {
        let engine = CapoEngine::new(CapoConfig::default());
        let doc = "Para A\n\nPara B\n\nPara C";
        let edit = EditOp::Rewrite {
            paragraph_id: "p1".to_string(),
            new_text: "Revised B".to_string(),
        };
        let result = engine.apply_edit(doc, &edit);
        assert!(result.contains("Revised B"));
        assert!(!result.contains("Para B"));
    }

    #[test]
    fn test_apply_prune() {
        let engine = CapoEngine::new(CapoConfig::default());
        let doc = "Para A\n\nPara B\n\nPara C";
        let edit = EditOp::Prune {
            paragraph_id: "p1".to_string(),
        };
        let result = engine.apply_edit(doc, &edit);
        assert!(!result.contains("Para B"));
        assert!(result.contains("Para A"));
        assert!(result.contains("Para C"));
    }

    #[test]
    fn test_evaluate_version() {
        let engine = CapoEngine::new(CapoConfig::default());
        let trajectories = vec![make_trail(true), make_trail(false), make_trail(true)];
        let metrics = engine.evaluate_version("test", &trajectories);
        assert_eq!(metrics.success_rate, 2.0 / 3.0);
    }
}
