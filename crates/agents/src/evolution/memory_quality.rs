//! Memory Quality Evaluation & Active Consolidation
//!
//! Provides:
//! - MemoryQualityEvaluator: scores candidates (0.0-1.0) for persistence
//!   worthiness
//! - ConsolidationEngine: compresses/deduplicates when L1/L2 approach capacity
//! - RedundancyCheck: cosine similarity + BM25 hybrid redundancy detection

use super::memory_nudge::MemoryCandidate;
use crate::memory::search::SearchResult;

/// Quality evaluator for memory candidates
#[derive(Debug, Clone)]
pub struct MemoryQualityEvaluator {
    /// Stability weight
    stability_weight: f32,
    /// Reuse value weight
    reuse_weight: f32,
    /// Novelty weight
    novelty_weight: f32,
    /// User confirmation weight
    confirmation_weight: f32,
    /// Redundancy similarity threshold
    redundancy_threshold: f32,
}

impl MemoryQualityEvaluator {
    pub fn new() -> Self {
        Self {
            stability_weight: 0.3,
            reuse_weight: 0.3,
            novelty_weight: 0.2,
            confirmation_weight: 0.2,
            redundancy_threshold: 0.85,
        }
    }

    /// Evaluate a candidate against existing memories.
    /// Returns 0.0-1.0 quality score.
    pub fn evaluate(&self, candidate: &MemoryCandidate, existing: &[SearchResult]) -> f32 {
        let mut score = 0.0;

        // Stability: non-temporary facts score higher
        if candidate.is_stable_fact {
            score += self.stability_weight;
        }

        // Cross-session reuse value
        if candidate.cross_session_value {
            score += self.reuse_weight;
        }

        // Novelty: not redundant with existing memories
        if !self.is_redundant(candidate, existing) {
            score += self.novelty_weight;
        }

        // User confirmation
        if candidate.has_user_confirmation {
            score += self.confirmation_weight;
        }

        score
    }

    /// Check if candidate is redundant with any existing memory
    fn is_redundant(&self, candidate: &MemoryCandidate, existing: &[SearchResult]) -> bool {
        if candidate.embedding.is_empty() || existing.is_empty() {
            // Fallback: simple text overlap check
            let content_lower = candidate.content.to_lowercase();
            return existing.iter().any(|e| {
                let existing_lower = e.content.to_lowercase();
                // Check if one is a substring of the other or high word overlap
                content_lower.contains(&existing_lower) || existing_lower.contains(&content_lower)
            });
        }

        // Use cosine similarity on embedding vectors
        existing.iter().any(|e| {
            if let Some(ref emb_str) = e.metadata.get("embedding") {
                if let Ok(existing_emb) = parse_embedding(emb_str) {
                    let sim = cosine_similarity(&candidate.embedding, &existing_emb);
                    return sim > self.redundancy_threshold;
                }
            }
            // Fallback: text overlap
            let existing_lower = e.content.to_lowercase();
            candidate.content.to_lowercase().contains(&existing_lower)
                || existing_lower.contains(&candidate.content.to_lowercase())
        })
    }

    /// Configure with custom weights
    pub fn with_weights(
        mut self,
        stability: f32,
        reuse: f32,
        novelty: f32,
        confirmation: f32,
    ) -> Self {
        self.stability_weight = stability;
        self.reuse_weight = reuse;
        self.novelty_weight = novelty;
        self.confirmation_weight = confirmation;
        self
    }
}

impl Default for MemoryQualityEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

/// Active consolidation engine for L1/L2 capacity management
#[derive(Debug, Clone)]
pub struct ConsolidationEngine {
    /// L1 max characters
    pub l1_max_chars: usize,
    /// L2 max characters
    pub l2_max_chars: usize,
    /// Compression ratio target (0.0-1.0)
    pub compression_target: f32,
}

impl ConsolidationEngine {
    pub fn new(l1_max: usize, l2_max: usize) -> Self {
        Self {
            l1_max_chars: l1_max,
            l2_max_chars: l2_max,
            compression_target: 0.7,
        }
    }

    /// Check if L1 content is approaching capacity
    pub fn l1_needs_consolidation(&self, current_chars: usize) -> bool {
        current_chars as f32 > self.l1_max_chars as f32 * 0.85
    }

    /// Check if L2 content is approaching capacity
    pub fn l2_needs_consolidation(&self, current_chars: usize) -> bool {
        current_chars as f32 > self.l2_max_chars as f32 * 0.85
    }

    /// Simple rule-based consolidation (LLM-based summarization can be added
    /// later) Returns consolidated content and list of archived entries
    pub fn consolidate_l1(&self, entries: &[String]) -> (String, Vec<String>) {
        let total_len: usize = entries.iter().map(|e| e.len()).sum();
        if total_len <= self.l1_max_chars {
            return (entries.join("\n\n"), Vec::new());
        }

        // Keep highest importance / most recent entries, archive others
        // Simple heuristic: keep first 70% by character budget, archive rest
        let mut kept = Vec::new();
        let mut archived = Vec::new();
        let mut current_len = 0;
        let budget = (self.l1_max_chars as f32 * self.compression_target) as usize;

        for entry in entries {
            if current_len + entry.len() <= budget {
                kept.push(entry.clone());
                current_len += entry.len();
            } else {
                archived.push(entry.clone());
            }
        }

        // If still over budget, compress kept entries by truncating
        let mut result = kept.join("\n\n");
        if result.len() > self.l1_max_chars {
            let truncate_to = self.l1_max_chars.saturating_sub(100);
            result = format!(
                "{}\n\n[... {} characters truncated ...]",
                &result[..truncate_to],
                result.len() - truncate_to
            );
        }

        (result, archived)
    }

    /// Generate consolidation prompt for LLM-based compression
    pub fn build_compression_prompt(&self, entries: &[String], target_chars: usize) -> String {
        format!(
            "请将以下记忆条目压缩为不超过 {} \
             字符的精炼版本。保留关键事实、偏好和陷阱，去除冗余和临时状态。用简洁的 Markdown \
             列表格式输出。\n\n{}",
            target_chars,
            entries.join("\n\n")
        )
    }
}

/// Redundancy detection helper
pub struct RedundancyCheck;

impl RedundancyCheck {
    /// Compute cosine similarity between two vectors
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot_product / (norm_a * norm_b)
    }

    /// Compute Jaccard similarity for keyword overlap
    pub fn jaccard_similarity(a: &str, b: &str) -> f32 {
        let words_a: std::collections::HashSet<String> = a
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        let words_b: std::collections::HashSet<String> = b
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        let intersection: usize = words_a.intersection(&words_b).count();
        let union: usize = words_a.union(&words_b).count();

        if union == 0 {
            return 0.0;
        }

        intersection as f32 / union as f32
    }
}

/// Parse embedding string (comma-separated floats)
fn parse_embedding(s: &str) -> Result<Vec<f32>, String> {
    s.split(',')
        .map(|v| {
            v.trim()
                .parse::<f32>()
                .map_err(|e| format!("Parse error: {}", e))
        })
        .collect()
}

/// Cosine similarity (public re-export)
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    RedundancyCheck::cosine_similarity(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_evaluation() {
        let evaluator = MemoryQualityEvaluator::new();
        let candidate = MemoryCandidate {
            content: "用户偏好使用 Rust 进行后端开发".to_string(),
            category: "user_preference".to_string(),
            is_stable_fact: true,
            cross_session_value: true,
            has_user_confirmation: true,
            embedding: vec![],
            source_id: "test".to_string(),
        };

        let score = evaluator.evaluate(&candidate, &[]);
        assert!(
            score >= 0.9,
            "High quality candidate should score >= 0.9, got {}",
            score
        );
    }

    #[test]
    fn test_low_quality_evaluation() {
        let evaluator = MemoryQualityEvaluator::new();
        let candidate = MemoryCandidate {
            content: "临时状态".to_string(),
            category: "temp".to_string(),
            is_stable_fact: false,
            cross_session_value: false,
            has_user_confirmation: false,
            embedding: vec![],
            source_id: "test".to_string(),
        };

        let score = evaluator.evaluate(&candidate, &[]);
        // Novelty gives 0.2 even for low-quality candidates (not redundant)
        assert_eq!(
            score, 0.2,
            "Low quality candidate should score novelty only"
        );
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((RedundancyCheck::cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!((RedundancyCheck::cosine_similarity(&a, &c)).abs() < 0.001);
    }

    #[test]
    fn test_consolidation_engine() {
        let engine = ConsolidationEngine::new(100, 100);
        let entries = vec![
            "Fact A: 使用 Docker 部署".to_string(),
            "Fact B: 数据库用 PostgreSQL".to_string(),
            "Fact C: 缓存用 Redis".to_string(),
        ];

        let (consolidated, archived) = engine.consolidate_l1(&entries);
        assert!(!consolidated.is_empty());
        assert!(archived.is_empty()); // Under budget
    }

    #[test]
    fn test_consolidation_triggers() {
        let engine = ConsolidationEngine::new(100, 100);
        assert!(engine.l1_needs_consolidation(90)); // 90 > 85
        assert!(!engine.l1_needs_consolidation(50)); // 50 < 85
    }
}
