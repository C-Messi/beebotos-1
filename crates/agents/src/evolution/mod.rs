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

pub mod memory_nudge;
pub mod memory_quality;
pub mod skill_distiller;
pub mod skill_lineage;
pub mod patch_engine;
pub mod capo;
pub mod atropos;
pub mod dapo;
pub mod papo;
pub mod scheduler;
pub mod sandbox;
pub mod benchmark;

pub use memory_nudge::{NudgeEngine, NudgeConfig, MemoryCandidate, MemoryWriter, NudgeTrigger};
pub use memory_quality::{MemoryQualityEvaluator, ConsolidationEngine, RedundancyCheck};
pub use skill_distiller::{SkillDistiller, DistillerConfig, DistilledSkill, DistillDecision, DistillTrigger};
pub use skill_lineage::{SkillLineage, LineageNode, LineageSource, SkillLifecycleManager, UsageStats, LifecycleAction};
pub use patch_engine::{PatchEngine, SkillPatch, PatchOp, PatchResult, Precondition};
pub use capo::{CapoEngine, CapoConfig, ContextScorer, EditOp, AttributionScore, VersionMetrics, EvolutionResult};
pub use atropos::{AtroposFramework, AtroposConfig, TrailCollector, AnnotatedTrail, EnvironmentManager, EvalEnvPool, DataPipeline, TrailStorage, InMemoryTrailStorage};
pub use dapo::{DapoTrainer, DapoConfig, TemperatureScheduler, TrainingMetrics, TrajectoryBatch, Policy};
pub use papo::{PapoTrainer, PapoConfig, CreditAssigner, CreditAssignmentStrategy, ProcessReward, ToolCallValidator, CodeExecutionValidator, HttpApiValidator, FileOperationValidator};
pub use scheduler::{EvolutionScheduler, EvolutionSchedule, EvolutionSummary};
pub use sandbox::{EvolutionSandbox, EvolutionProposal, EvolutionTarget, SafetyViolation};
