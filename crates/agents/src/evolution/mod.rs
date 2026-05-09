//! BeeBotOS Auto-Evolution System
//!
//! Phase 1: Memory Auto-Evolution
//! - NudgeEngine: proactive memory consolidation trigger
//! - MemoryQualityEvaluator: quality scoring + deduplication
//!
//! Phase 2: Skills Auto-Evolution
//! - SkillDistiller: trajectory → SKILL.md extraction
//! - SkillLineage: version tree + rollback tracking
//! - PatchEngine: diff-based skill updates with safety
//!
//! Phase 3: Agent Auto-Evolution
//! - CapoEngine: context-aware prompt optimization
//! - AtroposFramework: async trail collection + environment coordination
//! - DapoTrainer: dynamic sampling policy optimization (entropy-aware RL)
//! - PapoTrainer: process-aware policy optimization (fine-grained step rewards)

pub mod atropos;
pub mod benchmark;
pub mod capo;
pub mod dapo;
pub mod memory_nudge;
pub mod memory_quality;
pub mod papo;
pub mod patch_engine;
pub mod sandbox;
pub mod scheduler;
pub mod skill_distiller;
pub mod skill_lineage;

pub use atropos::{
    AnnotatedTrail, AtroposConfig, AtroposFramework, DataPipeline, EnvironmentManager, EvalEnvPool,
    InMemoryTrailStorage, TrailCollector, TrailStorage,
};
pub use capo::{
    AttributionScore, CapoConfig, CapoEngine, ContextScorer, EditOp, EvolutionResult,
    VersionMetrics,
};
pub use dapo::{
    DapoConfig, DapoTrainer, Policy, TemperatureScheduler, TrainingMetrics, TrajectoryBatch,
};
pub use memory_nudge::{MemoryCandidate, MemoryWriter, NudgeConfig, NudgeEngine, NudgeTrigger};
pub use memory_quality::{ConsolidationEngine, MemoryQualityEvaluator, RedundancyCheck};
pub use papo::{
    CodeExecutionValidator, CreditAssigner, CreditAssignmentStrategy, FileOperationValidator,
    HttpApiValidator, PapoConfig, PapoTrainer, ProcessReward, ToolCallValidator,
};
pub use patch_engine::{PatchEngine, PatchOp, PatchResult, Precondition, SkillPatch};
pub use sandbox::{EvolutionProposal, EvolutionSandbox, EvolutionTarget, SafetyViolation};
pub use scheduler::{EvolutionSchedule, EvolutionScheduler, EvolutionSummary};
pub use skill_distiller::{
    DistillDecision, DistillTrigger, DistilledSkill, DistillerConfig, SkillDistiller,
};
pub use skill_lineage::{
    LifecycleAction, LineageNode, LineageSource, SkillLifecycleManager, SkillLineage, UsageStats,
};
