//! Skill Matching Module (V2)
//!
//! Pure LLM-driven skill matching system with zero hardcoded rules.
//!
//! Architecture:
//! 1. Retrieval Layer: Registry search / embedding recall → Top-K candidates
//! 2. Intent Analysis Layer: LLM analyzes user query → structured intent
//! 3. Skill Ranking Layer: LLM scores each candidate → selection or rejection
//! 4. Execution Layer: Progressive disclosure (L1→L2→L3)
//! 5. Trace Layer: Full observability of every matching decision

pub mod activation_trace;
pub mod intent_analyzer;
pub mod skill_selector;

pub use activation_trace::{
    ExecutionTrace, InMemoryTraceStore, RankingTrace, RetrievalTrace, SkillActivationTrace, TraceStore,
};
pub use intent_analyzer::{IntentAnalysisV2, LLMIntentAnalyzer, PlanningStrategyHint};
pub use skill_selector::{SkillScore, SkillSelection, SkillSelector};
