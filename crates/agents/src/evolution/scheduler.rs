//! Evolution Scheduler — Orchestrates Three-Layer Co-Evolution
//!
//! Coordinates Memory → Skill → CAPO → Atropos triggers with:
//! - Frequency-based scheduling (turn count, task count, time-based)
//! - Resource budgeting (evolution overhead < 5% task latency)
//! - Safety gating via EvolutionSandbox

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::warn;

use crate::error::AgentError;
use crate::planning::ToolTrail;
use crate::evolution::skill_distiller::{SkillDistiller, DistillerConfig};
use crate::evolution::skill_lineage::SkillLifecycleManager;
use crate::evolution::capo::{CapoEngine, CapoConfig};
use crate::evolution::atropos::{AtroposFramework, AtroposConfig, InMemoryTrailStorage};
use crate::evolution::sandbox::EvolutionSandbox;

/// Evolution layer trigger frequencies
#[derive(Debug, Clone)]
pub struct EvolutionSchedule {
    /// Memory nudge: every N user turns
    pub memory_turn_threshold: usize,
    /// Skill distillation: every N successful complex tasks
    pub skill_task_threshold: usize,
    /// CAPO: every N trajectories accumulated
    pub capo_trajectory_threshold: usize,
    /// Atropos batch flush: interval in seconds
    pub atropos_flush_interval_secs: u64,
    /// Max evolution CPU time budget per task (ms)
    pub max_evolution_budget_ms: u64,
}

impl Default for EvolutionSchedule {
    fn default() -> Self {
        Self {
            memory_turn_threshold: 10,
            skill_task_threshold: 5,
            capo_trajectory_threshold: 100,
            atropos_flush_interval_secs: 300,
            max_evolution_budget_ms: 500,
        }
    }
}

/// Global evolution scheduler state
pub struct EvolutionScheduler {
    pub schedule: EvolutionSchedule,
    /// User turn counter (increments per user message)
    pub turn_counter: AtomicUsize,
    /// Successful complex task counter
    pub task_counter: AtomicUsize,
    /// Trajectory counter for CAPO
    pub trajectory_counter: AtomicUsize,
    /// Last Atropos flush timestamp
    pub last_atropos_flush: RwLock<Instant>,
    /// Safety sandbox
    pub sandbox: EvolutionSandbox,
    /// Whether memory evolution is enabled
    pub memory_enabled: AtomicBool,
    /// Whether skill evolution is enabled
    pub skill_enabled: AtomicBool,
    /// Whether CAPO is enabled
    pub capo_enabled: AtomicBool,
    /// Whether Atropos collection is enabled
    pub atropos_enabled: AtomicBool,
    /// Skill distiller (reused across tasks)
    pub skill_distiller: SkillDistiller,
    /// Skill lifecycle manager
    pub lifecycle_manager: SkillLifecycleManager,
    /// CAPO engine
    pub capo_engine: CapoEngine,
    /// Atropos framework
    pub atropos: AtroposFramework,
    /// Cumulative evolution overhead (for adaptive throttling)
    pub cumulative_overhead_ms: AtomicUsize,
    /// Total tasks processed
    pub total_tasks: AtomicUsize,
}

impl std::fmt::Debug for EvolutionScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvolutionScheduler")
            .field("schedule", &self.schedule)
            .field("turn_counter", &self.turn_counter.load(Ordering::SeqCst))
            .field("task_counter", &self.task_counter.load(Ordering::SeqCst))
            .field("trajectory_counter", &self.trajectory_counter.load(Ordering::SeqCst))
            .field("memory_enabled", &self.memory_enabled.load(Ordering::SeqCst))
            .field("skill_enabled", &self.skill_enabled.load(Ordering::SeqCst))
            .field("capo_enabled", &self.capo_enabled.load(Ordering::SeqCst))
            .field("atropos_enabled", &self.atropos_enabled.load(Ordering::SeqCst))
            .field("total_tasks", &self.total_tasks.load(Ordering::SeqCst))
            .finish()
    }
}

impl EvolutionScheduler {
    pub fn new(schedule: EvolutionSchedule) -> Self {
        let atropos_config = AtroposConfig {
            trail_buffer_size: 100,
            eval_env_concurrency: 2,
            data_retention_days: 90,
            batch_flush_interval_secs: schedule.atropos_flush_interval_secs,
        };
        let storage = Arc::new(InMemoryTrailStorage::new());

        Self {
            schedule,
            turn_counter: AtomicUsize::new(0),
            task_counter: AtomicUsize::new(0),
            trajectory_counter: AtomicUsize::new(0),
            last_atropos_flush: RwLock::new(Instant::now()),
            sandbox: EvolutionSandbox::new(),
            memory_enabled: AtomicBool::new(true),
            skill_enabled: AtomicBool::new(true),
            capo_enabled: AtomicBool::new(true),
            atropos_enabled: AtomicBool::new(true),
            skill_distiller: SkillDistiller::new(DistillerConfig::default()),
            lifecycle_manager: SkillLifecycleManager::new(),
            capo_engine: CapoEngine::new(CapoConfig::default()),
            atropos: AtroposFramework::new(storage, atropos_config),
            cumulative_overhead_ms: AtomicUsize::new(0),
            total_tasks: AtomicUsize::new(0),
        }
    }

    /// Increment turn counter, return true if memory nudge should trigger
    pub fn on_user_turn(&self) -> bool {
        let turns = self.turn_counter.fetch_add(1, Ordering::SeqCst) + 1;
        self.memory_enabled.load(Ordering::SeqCst) && turns % self.schedule.memory_turn_threshold == 0
    }

    /// Post-task hook: orchestrate all evolution layers
    pub async fn on_task_completed(
        &self,
        trail: &ToolTrail,
        _goal: &str,
        success: bool,
    ) -> Result<EvolutionSummary, AgentError> {
        let start = Instant::now();
        let mut summary = EvolutionSummary::default();

        // 1. Atropos: always collect trail (non-blocking, lightweight)
        if self.atropos_enabled.load(Ordering::SeqCst) {
            self.atropos.collect_trail(trail.clone()).await;
            let mut last_flush = self.last_atropos_flush.write().await;
            if last_flush.elapsed().as_secs() >= self.schedule.atropos_flush_interval_secs {
                let _ = self.atropos.flush().await;
                *last_flush = Instant::now();
            }
            summary.atropos_collected = true;
        }

        // 2. Skill evolution: trigger on successful complex tasks
        if self.skill_enabled.load(Ordering::SeqCst) && success {
            let tool_count: usize = trail.steps.iter().map(|s| s.tool_calls.len()).sum();
            if tool_count >= self.skill_distiller.config.min_tool_calls {
                let tasks = self.task_counter.fetch_add(1, Ordering::SeqCst) + 1;
                if tasks % self.schedule.skill_task_threshold == 0 {
                    summary.skill_distill_triggered = true;
                }
            }
        }

        // 3. CAPO: trigger when enough trajectories accumulated
        if self.capo_enabled.load(Ordering::SeqCst) {
            let trajs = self.trajectory_counter.fetch_add(1, Ordering::SeqCst) + 1;
            if trajs % self.schedule.capo_trajectory_threshold == 0 {
                summary.capo_triggered = true;
            }
        }

        // Track overhead
        let elapsed_ms = start.elapsed().as_millis() as usize;
        self.cumulative_overhead_ms.fetch_add(elapsed_ms, Ordering::SeqCst);
        self.total_tasks.fetch_add(1, Ordering::SeqCst);
        summary.overhead_ms = elapsed_ms as u64;

        Ok(summary)
    }

    /// Check if evolution overhead is within budget (< 5% of average task latency)
    pub fn overhead_ratio(&self) -> f32 {
        let total = self.total_tasks.load(Ordering::SeqCst).max(1);
        let overhead = self.cumulative_overhead_ms.load(Ordering::SeqCst) as f32;
        // Assume average task latency ~2000ms for ratio calculation
        let assumed_avg_latency_ms = 2000.0;
        let total_budget_ms = total as f32 * assumed_avg_latency_ms * 0.05;
        if total_budget_ms > 0.0 {
            (overhead / total_budget_ms).min(1.0)
        } else {
            0.0
        }
    }

    /// Adaptive throttling: disable layers if overhead exceeds budget
    pub async fn adaptive_throttle(&self) {
        let ratio = self.overhead_ratio();
        if ratio > 0.8 {
            warn!("Evolution overhead at {:.0}% of budget, throttling CAPO", ratio * 100.0);
            self.capo_enabled.store(false, Ordering::SeqCst);
        } else if ratio < 0.3 {
            self.capo_enabled.store(true, Ordering::SeqCst);
        }
    }

    /// Reset all counters (e.g., after maintenance)
    pub fn reset_counters(&self) {
        self.turn_counter.store(0, Ordering::SeqCst);
        self.task_counter.store(0, Ordering::SeqCst);
        self.trajectory_counter.store(0, Ordering::SeqCst);
        self.cumulative_overhead_ms.store(0, Ordering::SeqCst);
        self.total_tasks.store(0, Ordering::SeqCst);
    }
}

impl Clone for EvolutionScheduler {
    fn clone(&self) -> Self {
        Self {
            schedule: self.schedule.clone(),
            turn_counter: AtomicUsize::new(self.turn_counter.load(Ordering::SeqCst)),
            task_counter: AtomicUsize::new(self.task_counter.load(Ordering::SeqCst)),
            trajectory_counter: AtomicUsize::new(self.trajectory_counter.load(Ordering::SeqCst)),
            last_atropos_flush: RwLock::new(Instant::now()),
            sandbox: self.sandbox.clone(),
            memory_enabled: AtomicBool::new(self.memory_enabled.load(Ordering::SeqCst)),
            skill_enabled: AtomicBool::new(self.skill_enabled.load(Ordering::SeqCst)),
            capo_enabled: AtomicBool::new(self.capo_enabled.load(Ordering::SeqCst)),
            atropos_enabled: AtomicBool::new(self.atropos_enabled.load(Ordering::SeqCst)),
            skill_distiller: self.skill_distiller.clone(),
            lifecycle_manager: self.lifecycle_manager.clone(),
            capo_engine: self.capo_engine.clone(),
            atropos: self.atropos.clone(),
            cumulative_overhead_ms: AtomicUsize::new(self.cumulative_overhead_ms.load(Ordering::SeqCst)),
            total_tasks: AtomicUsize::new(self.total_tasks.load(Ordering::SeqCst)),
        }
    }
}

/// Summary of evolution actions taken after a task
#[derive(Debug, Clone, Default)]
pub struct EvolutionSummary {
    pub atropos_collected: bool,
    pub skill_distill_triggered: bool,
    pub capo_triggered: bool,
    pub overhead_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_counter() {
        let sched = EvolutionScheduler::new(EvolutionSchedule {
            memory_turn_threshold: 3,
            ..Default::default()
        });
        assert!(!sched.on_user_turn()); // 1
        assert!(!sched.on_user_turn()); // 2
        assert!(sched.on_user_turn());  // 3
        assert!(!sched.on_user_turn()); // 4
        assert!(!sched.on_user_turn()); // 5
        assert!(sched.on_user_turn());  // 6
    }

    #[tokio::test]
    async fn test_on_task_completed() {
        let sched = EvolutionScheduler::new(EvolutionSchedule::default());
        let trail = ToolTrail::new("test".to_string());

        let summary = sched.on_task_completed(&trail, "goal", true).await.unwrap();
        assert!(summary.atropos_collected);
    }

    #[test]
    fn test_overhead_ratio() {
        let sched = EvolutionScheduler::new(EvolutionSchedule::default());
        // No tasks → ratio 0
        assert_eq!(sched.overhead_ratio(), 0.0);

        // Simulate some overhead
        sched.total_tasks.store(100, Ordering::SeqCst);
        sched.cumulative_overhead_ms.store(5000, Ordering::SeqCst); // 5s overhead on 100 tasks
        // budget = 100 * 2000 * 0.05 = 10000ms
        // ratio = 5000 / 10000 = 0.5
        let ratio = sched.overhead_ratio();
        assert!(ratio > 0.4 && ratio < 0.6, "ratio should be ~0.5, got {}", ratio);
    }
}
