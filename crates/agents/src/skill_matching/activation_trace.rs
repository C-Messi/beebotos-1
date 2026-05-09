//! Skill Activation Trace
//!
//! Full observability of every skill matching decision for debugging and self-optimization.

use chrono::{DateTime, Utc};

use crate::skill_matching::intent_analyzer::IntentAnalysisV2;
use crate::skill_matching::skill_selector::SkillScore;

/// Full trace of a skill activation decision
#[derive(Debug, Clone)]
pub struct SkillActivationTrace {
    pub trace_id: String,
    pub timestamp: DateTime<Utc>,
    pub user_query: String,
    pub intent_analysis: IntentAnalysisV2,
    /// Retrieval layer results
    pub retrieval: RetrievalTrace,
    /// Ranking layer results
    pub ranking: RankingTrace,
    /// Execution layer results (populated after execution)
    pub execution: Option<ExecutionTrace>,
    /// User feedback (if any)
    pub feedback: Option<UserFeedback>,
}

impl SkillActivationTrace {
    pub fn new(query: &str, intent: IntentAnalysisV2) -> Self {
        Self {
            trace_id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            user_query: query.to_string(),
            intent_analysis: intent,
            retrieval: RetrievalTrace {
                method: String::new(),
                candidate_skills: Vec::new(),
                recall_scores: Vec::new(),
            },
            ranking: RankingTrace {
                llm_model: String::new(),
                scores: Vec::new(),
                selected_skill: None,
                reasoning: String::new(),
                confidence: 0.0,
            },
            execution: None,
            feedback: None,
        }
    }

    pub fn with_retrieval(mut self, retrieval: RetrievalTrace) -> Self {
        self.retrieval = retrieval;
        self
    }

    pub fn with_ranking(mut self, ranking: RankingTrace) -> Self {
        self.ranking = ranking;
        self
    }

    pub fn with_execution(mut self, execution: ExecutionTrace) -> Self {
        self.execution = Some(execution);
        self
    }

    pub fn with_feedback(mut self, feedback: UserFeedback) -> Self {
        self.feedback = Some(feedback);
        self
    }

    /// Whether this was a false positive (skill selected but shouldn't have been)
    pub fn is_false_positive(&self) -> bool {
        match &self.feedback {
            Some(fb) => fb.was_correct == false && self.ranking.selected_skill.is_some(),
            None => false,
        }
    }

    /// Whether this was a false negative (no skill selected but one should have been)
    pub fn is_false_negative(&self) -> bool {
        match &self.feedback {
            Some(fb) => fb.was_correct == false && self.ranking.selected_skill.is_none(),
            None => false,
        }
    }
}

/// Retrieval layer trace
#[derive(Debug, Clone)]
pub struct RetrievalTrace {
    pub method: String, // "registry_search" | "embedding" | "session_inherit"
    pub candidate_skills: Vec<String>, // skill_ids
    pub recall_scores: Vec<(String, f32)>, // (skill_id, score)
}

/// Ranking layer trace
#[derive(Debug, Clone)]
pub struct RankingTrace {
    pub llm_model: String,
    pub scores: Vec<SkillScore>,
    pub selected_skill: Option<String>,
    pub reasoning: String,
    pub confidence: f32,
}

/// Execution layer trace
#[derive(Debug, Clone)]
pub struct ExecutionTrace {
    pub skill_id: String,
    pub execution_method: String, // "mcp_bridge" | "llm_fallback" | "wasm"
    pub execution_time_ms: u64,
    pub success: bool,
    pub output_length: usize,
    pub error: Option<String>,
}

/// User feedback on skill activation
#[derive(Debug, Clone)]
pub struct UserFeedback {
    pub was_correct: bool,
    pub comment: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// Trait for storing activation traces
#[async_trait::async_trait]
pub trait TraceStore: Send + Sync {
    async fn store(&self, trace: &SkillActivationTrace) -> Result<(), TraceStoreError>;
    async fn get_traces_for_skill(
        &self,
        skill_id: &str,
        limit: usize,
    ) -> Result<Vec<SkillActivationTrace>, TraceStoreError>;
    async fn get_false_positives(
        &self,
        skill_id: &str,
        limit: usize,
    ) -> Result<Vec<SkillActivationTrace>, TraceStoreError>;
    async fn get_false_negatives(
        &self,
        skill_id: &str,
        limit: usize,
    ) -> Result<Vec<SkillActivationTrace>, TraceStoreError>;
    async fn get_low_confidence_matches(
        &self,
        threshold: f32,
        limit: usize,
    ) -> Result<Vec<SkillActivationTrace>, TraceStoreError>;
}

/// In-memory trace store (for initial implementation)
pub struct InMemoryTraceStore {
    traces: tokio::sync::RwLock<Vec<SkillActivationTrace>>,
}

impl InMemoryTraceStore {
    pub fn new() -> Self {
        Self {
            traces: tokio::sync::RwLock::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl TraceStore for InMemoryTraceStore {
    async fn store(&self, trace: &SkillActivationTrace) -> Result<(), TraceStoreError> {
        let mut traces = self.traces.write().await;
        traces.push(trace.clone());
        // Keep only last 10000 traces to prevent memory bloat
        if traces.len() > 10000 {
            traces.remove(0);
        }
        Ok(())
    }

    async fn get_traces_for_skill(
        &self,
        skill_id: &str,
        limit: usize,
    ) -> Result<Vec<SkillActivationTrace>, TraceStoreError> {
        let traces = self.traces.read().await;
        let filtered: Vec<_> = traces
            .iter()
            .filter(|t| {
                t.ranking.selected_skill.as_ref() == Some(&skill_id.to_string())
            })
            .rev()
            .take(limit)
            .cloned()
            .collect();
        Ok(filtered)
    }

    async fn get_false_positives(
        &self,
        skill_id: &str,
        limit: usize,
    ) -> Result<Vec<SkillActivationTrace>, TraceStoreError> {
        let traces = self.traces.read().await;
        let filtered: Vec<_> = traces
            .iter()
            .filter(|t| {
                t.is_false_positive()
                    && t.ranking.selected_skill.as_ref() == Some(&skill_id.to_string())
            })
            .rev()
            .take(limit)
            .cloned()
            .collect();
        Ok(filtered)
    }

    async fn get_false_negatives(
        &self,
        skill_id: &str,
        limit: usize,
    ) -> Result<Vec<SkillActivationTrace>, TraceStoreError> {
        let traces = self.traces.read().await;
        let filtered: Vec<_> = traces
            .iter()
            .filter(|t| {
                t.is_false_negative()
                    && t.retrieval.candidate_skills.contains(&skill_id.to_string())
            })
            .rev()
            .take(limit)
            .cloned()
            .collect();
        Ok(filtered)
    }

    async fn get_low_confidence_matches(
        &self,
        threshold: f32,
        limit: usize,
    ) -> Result<Vec<SkillActivationTrace>, TraceStoreError> {
        let traces = self.traces.read().await;
        let filtered: Vec<_> = traces
            .iter()
            .filter(|t| {
                t.ranking.selected_skill.is_some() && t.ranking.confidence < threshold
            })
            .rev()
            .take(limit)
            .cloned()
            .collect();
        Ok(filtered)
    }
}

/// Trace store errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum TraceStoreError {
    #[error("Storage error: {0}")]
    StorageError(String),
}
