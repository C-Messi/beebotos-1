//! Atropos — Async Coordination Framework for Evolution
//!
//! Infrastructure layer for:
//! 1. Trail collection (async buffering + batch persistence)
//! 2. Environment management (CAPO eval pools + RL training isolation)
//! 3. Data pipeline (annotation + formatting)

use std::collections::VecDeque;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::error::AgentError;
use crate::planning::ToolTrail;
use crate::security::session_isolation::ResourceLimits;

/// Atropos framework configuration
#[derive(Debug, Clone)]
pub struct AtroposConfig {
    /// In-memory trail buffer size before flush
    pub trail_buffer_size: usize,
    /// Concurrent evaluation environments
    pub eval_env_concurrency: usize,
    /// Data retention days
    pub data_retention_days: u32,
    /// Batch flush interval (seconds)
    pub batch_flush_interval_secs: u64,
}

impl Default for AtroposConfig {
    fn default() -> Self {
        Self {
            trail_buffer_size: 100,
            eval_env_concurrency: 4,
            data_retention_days: 90,
            batch_flush_interval_secs: 300,
        }
    }
}

/// Annotated trajectory with metadata
#[derive(Debug, Clone)]
pub struct AnnotatedTrail {
    pub trail: ToolTrail,
    pub success_rate: f32,
    pub user_satisfaction: Option<f32>,
    pub token_consumption: usize,
    pub complexity_score: f32,
    pub timestamp: DateTime<Utc>,
}

/// Trait for persistent trail storage
#[async_trait::async_trait]
pub trait TrailStorage: Send + Sync {
    async fn store_batch(&self, trails: &[AnnotatedTrail]) -> Result<(), AgentError>;
    async fn load_batch(&self, limit: usize) -> Result<Vec<AnnotatedTrail>, AgentError>;
}

/// In-memory trail storage (for testing / ephemeral use)
pub struct InMemoryTrailStorage {
    trails: RwLock<Vec<AnnotatedTrail>>,
}

impl InMemoryTrailStorage {
    pub fn new() -> Self {
        Self {
            trails: RwLock::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl TrailStorage for InMemoryTrailStorage {
    async fn store_batch(&self, trails: &[AnnotatedTrail]) -> Result<(), AgentError> {
        let mut buf = self.trails.write().await;
        buf.extend_from_slice(trails);
        Ok(())
    }

    async fn load_batch(&self, limit: usize) -> Result<Vec<AnnotatedTrail>, AgentError> {
        let buf = self.trails.read().await;
        Ok(buf.iter().rev().take(limit).cloned().collect())
    }
}

impl Default for InMemoryTrailStorage {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch flush trigger logic
#[derive(Debug, Clone)]
pub struct BatchTrigger {
    max_buffer_size: usize,
    _max_age_secs: u64,
}

impl BatchTrigger {
    pub fn new(max_buffer_size: usize, max_age_secs: u64) -> Self {
        Self {
            max_buffer_size,
            _max_age_secs: max_age_secs,
        }
    }

    pub fn should_flush(&self, buffer: &VecDeque<ToolTrail>) -> bool {
        if buffer.len() >= self.max_buffer_size {
            return true;
        }
        // Age check would require tracking insertion times; simplified here
        false
    }
}

/// Asynchronous trail collector
pub struct TrailCollector {
    buffer: Arc<RwLock<VecDeque<ToolTrail>>>,
    storage: Arc<dyn TrailStorage>,
    batch_trigger: BatchTrigger,
}

impl Clone for TrailCollector {
    fn clone(&self) -> Self {
        Self {
            buffer: self.buffer.clone(),
            storage: self.storage.clone(),
            batch_trigger: self.batch_trigger.clone(),
        }
    }
}

impl std::fmt::Debug for TrailCollector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrailCollector")
            .field("batch_trigger", &self.batch_trigger)
            .finish()
    }
}

impl TrailCollector {
    pub fn new(storage: Arc<dyn TrailStorage>, config: &AtroposConfig) -> Self {
        Self {
            buffer: Arc::new(RwLock::new(VecDeque::new())),
            storage,
            batch_trigger: BatchTrigger::new(
                config.trail_buffer_size,
                config.batch_flush_interval_secs,
            ),
        }
    }

    /// Collect a single trail (non-blocking)
    pub async fn collect(&self, trail: ToolTrail) {
        let mut buf = self.buffer.write().await;
        buf.push_back(trail);

        if self.batch_trigger.should_flush(&buf) {
            drop(buf);
            let _ = self.flush().await;
        }
    }

    /// Force flush all buffered trails
    pub async fn flush(&self) -> Result<(), AgentError> {
        let trails: Vec<ToolTrail> = {
            let mut buf = self.buffer.write().await;
            buf.drain(..).collect()
        };

        if trails.is_empty() {
            return Ok(());
        }

        let annotated: Vec<AnnotatedTrail> = trails.into_iter().map(|t| self.annotate(t)).collect();

        self.storage.store_batch(&annotated).await?;
        Ok(())
    }

    /// Annotate a single trail with metadata
    fn annotate(&self, trail: ToolTrail) -> AnnotatedTrail {
        let success_rate = self.calculate_success_rate(&trail);
        let complexity = self.assess_complexity(&trail);

        AnnotatedTrail {
            trail,
            success_rate,
            user_satisfaction: None,
            token_consumption: 0,
            complexity_score: complexity,
            timestamp: Utc::now(),
        }
    }

    fn calculate_success_rate(&self, trail: &ToolTrail) -> f32 {
        let total = trail.steps.len().max(1);
        let successful = trail
            .steps
            .iter()
            .filter(|s| s.tool_calls.iter().all(|c| c.success))
            .count();
        successful as f32 / total as f32
    }

    fn assess_complexity(&self, trail: &ToolTrail) -> f32 {
        let tool_count: usize = trail.steps.iter().map(|s| s.tool_calls.len()).sum();
        let step_count = trail.steps.len();
        (tool_count as f32 * 0.5 + step_count as f32 * 0.3).min(10.0)
    }
}

/// Lightweight evaluation environment
#[derive(Debug, Clone)]
pub struct EvalEnvironment {
    pub id: String,
    pub resource_limits: ResourceLimits,
}

/// Pool of reusable evaluation environments
#[derive(Debug, Clone)]
pub struct EvalEnvPool {
    envs: Arc<RwLock<Vec<EvalEnvironment>>>,
    max_size: usize,
    allocated: Arc<RwLock<usize>>,
}

impl EvalEnvPool {
    pub fn new(max_size: usize) -> Self {
        Self {
            envs: Arc::new(RwLock::new(Vec::new())),
            max_size,
            allocated: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn acquire(&self) -> Result<EvalEnvironment, AgentError> {
        let mut envs = self.envs.write().await;
        if let Some(env) = envs.pop() {
            return Ok(env);
        }

        let mut allocated = self.allocated.write().await;
        if *allocated < self.max_size {
            *allocated += 1;
            Ok(EvalEnvironment {
                id: format!("eval-{}", uuid::Uuid::new_v4()),
                resource_limits: ResourceLimits {
                    max_memory_mb: 256,
                    max_cpu_time_ms: 60000,
                    max_execution_time_secs: 60,
                    max_fs_usage_mb: 50,
                    max_network_requests_per_min: 10,
                },
            })
        } else {
            Err(AgentError::Execution(
                "Eval environment pool exhausted".to_string(),
            ))
        }
    }

    pub async fn release(&self, env: EvalEnvironment) {
        let mut envs = self.envs.write().await;
        if envs.len() < self.max_size {
            envs.push(env);
        }
    }
}

/// Training environment for RL (isolated)
#[derive(Debug, Clone)]
pub struct TrainingEnvironment {
    pub dataset: Vec<AnnotatedTrail>,
    pub resource_limits: ResourceLimits,
}

impl TrainingEnvironment {
    pub fn new(dataset: Vec<AnnotatedTrail>, limits: ResourceLimits) -> Self {
        Self {
            dataset,
            resource_limits: limits,
        }
    }
}

/// Environment manager: handles eval pools and training environments
#[derive(Debug, Clone)]
pub struct EnvironmentManager {
    eval_pool: EvalEnvPool,
    resource_limits: ResourceLimits,
}

impl EnvironmentManager {
    pub fn new(config: &AtroposConfig) -> Self {
        Self {
            eval_pool: EvalEnvPool::new(config.eval_env_concurrency),
            resource_limits: ResourceLimits {
                max_memory_mb: 512,
                max_cpu_time_ms: 120000,
                max_execution_time_secs: 120,
                max_fs_usage_mb: 100,
                max_network_requests_per_min: 0,
            },
        }
    }

    pub async fn acquire_eval_env(&self) -> Result<EvalEnvironment, AgentError> {
        self.eval_pool.acquire().await
    }

    pub async fn release_eval_env(&self, env: EvalEnvironment) {
        self.eval_pool.release(env).await;
    }

    pub fn create_training_env(&self, dataset: Vec<AnnotatedTrail>) -> TrainingEnvironment {
        TrainingEnvironment::new(dataset, self.resource_limits.clone())
    }
}

/// Data pipeline: transforms raw trails into training-ready datasets
#[derive(Debug, Clone, Default)]
pub struct DataPipeline;

impl DataPipeline {
    pub fn new() -> Self {
        Self
    }

    /// Filter and normalize trajectories for RL training
    pub fn prepare_training_dataset(&self, trails: &[AnnotatedTrail]) -> Vec<AnnotatedTrail> {
        trails
            .iter()
            .filter(|t| t.complexity_score > 1.0) // filter trivial trajectories
            .cloned()
            .collect()
    }

    /// Filter for CAPO document optimization
    pub fn prepare_capo_dataset(&self, trails: &[AnnotatedTrail]) -> Vec<AnnotatedTrail> {
        trails
            .iter()
            .filter(|t| t.success_rate < 1.0) // include both success and partial failure
            .cloned()
            .collect()
    }
}

/// Atropos top-level coordinator
#[derive(Debug, Clone)]
pub struct AtroposFramework {
    pub trail_collector: TrailCollector,
    pub environment_manager: EnvironmentManager,
    pub data_pipeline: DataPipeline,
    pub config: AtroposConfig,
}

impl AtroposFramework {
    pub fn new(storage: Arc<dyn TrailStorage>, config: AtroposConfig) -> Self {
        Self {
            trail_collector: TrailCollector::new(storage, &config),
            environment_manager: EnvironmentManager::new(&config),
            data_pipeline: DataPipeline::new(),
            config,
        }
    }

    /// Collect a trail into the pipeline
    pub async fn collect_trail(&self, trail: ToolTrail) {
        self.trail_collector.collect(trail).await;
    }

    /// Flush all pending trails
    pub async fn flush(&self) -> Result<(), AgentError> {
        self.trail_collector.flush().await
    }

    /// Get CAPO-ready dataset from storage
    pub async fn get_capo_dataset(
        &self,
        storage: &dyn TrailStorage,
        limit: usize,
    ) -> Result<Vec<AnnotatedTrail>, AgentError> {
        let trails = storage.load_batch(limit).await?;
        Ok(self.data_pipeline.prepare_capo_dataset(&trails))
    }

    /// Get RL training dataset from storage
    pub async fn get_training_dataset(
        &self,
        storage: &dyn TrailStorage,
        limit: usize,
    ) -> Result<Vec<AnnotatedTrail>, AgentError> {
        let trails = storage.load_batch(limit).await?;
        Ok(self.data_pipeline.prepare_training_dataset(&trails))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_trail_collector_flush() {
        let storage = Arc::new(InMemoryTrailStorage::new());
        let config = AtroposConfig {
            trail_buffer_size: 2,
            ..Default::default()
        };
        let collector = TrailCollector::new(storage.clone(), &config);

        let mut trail = ToolTrail::new("t1".to_string());
        trail.finish(crate::planning::TrailStatus::Success);
        collector.collect(trail.clone()).await;
        collector.collect(trail.clone()).await;

        // Buffer should have flushed automatically at size 2
        let stored = storage.load_batch(10).await.unwrap();
        assert_eq!(stored.len(), 2);
    }

    #[tokio::test]
    async fn test_eval_env_pool() {
        let pool = EvalEnvPool::new(2);
        let _env1 = pool.acquire().await.unwrap();
        let _env2 = pool.acquire().await.unwrap();
        assert!(pool.acquire().await.is_err()); // pool exhausted
    }

    #[test]
    fn test_data_pipeline_filter() {
        let pipeline = DataPipeline::new();
        let trails = vec![
            AnnotatedTrail {
                trail: ToolTrail::new("t1".to_string()),
                success_rate: 1.0,
                user_satisfaction: None,
                token_consumption: 100,
                complexity_score: 0.5,
                timestamp: Utc::now(),
            },
            AnnotatedTrail {
                trail: ToolTrail::new("t2".to_string()),
                success_rate: 0.5,
                user_satisfaction: None,
                token_consumption: 200,
                complexity_score: 3.0,
                timestamp: Utc::now(),
            },
        ];

        let training = pipeline.prepare_training_dataset(&trails);
        assert_eq!(training.len(), 1); // only complexity > 1.0

        let capo = pipeline.prepare_capo_dataset(&trails);
        assert_eq!(capo.len(), 1); // only success_rate < 1.0
    }
}
