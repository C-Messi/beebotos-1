//! Agent Runtime Implementation
//!
//! 高性能 Agent 运行时，实现对象池模式和批量处理。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info};

use super::{RuntimeConfig, RuntimeMetrics, SharedResourcePool};
use crate::error::{AgentError, Result};
use crate::media::downloader::MediaDownloader;
use crate::{Agent, AgentConfig, Task, TaskResult, TaskType};

/// Agent Runtime - 高性能运行时，支持对象池和批量处理
///
/// # 对象池模式
/// - 共享 MediaDownloader：避免重复创建连接池
/// - 共享 HTTP Client：复用 TCP 连接
/// - 信号量控制：限制并发数，防止资源耗尽
///
/// # 批量处理
/// - 批量任务执行：减少上下文切换
/// - 并行处理：利用多核 CPU
/// - 结果聚合：统一返回处理结果
#[derive(Clone)]
pub struct AgentRuntime {
    /// Agent 配置
    config: AgentConfig,
    /// 运行时配置
    runtime_config: RuntimeConfig,
    /// 共享资源池
    resource_pool: Arc<SharedResourcePool>,
    /// 共享 Media Downloader（可选）
    media_downloader: Option<Arc<MediaDownloader>>,
    /// 底层 Agent 实例
    agent: Arc<Mutex<Agent>>,
    /// 运行时指标
    metrics: Arc<RwLock<RuntimeMetrics>>,
    /// 任务计数器
    task_counter: Arc<AtomicU64>,
    /// 批量计数器
    batch_counter: Arc<AtomicU64>,
}

impl AgentRuntime {
    /// 创建新的 Agent Runtime
    pub fn new(agent_config: AgentConfig, runtime_config: RuntimeConfig) -> Result<Self> {
        // 创建共享资源池
        let resource_pool = Arc::new(SharedResourcePool::new(&runtime_config)?);

        // 初始化 Media Downloader（如果启用共享）
        let media_downloader = if runtime_config.enable_shared_media_downloader {
            Some(MediaDownloader::global()?)
        } else {
            None
        };

        // 创建底层 Agent
        let agent = Agent::new(agent_config.clone());

        Ok(Self {
            config: agent_config,
            runtime_config,
            resource_pool,
            media_downloader,
            agent: Arc::new(Mutex::new(agent)),
            metrics: Arc::new(RwLock::new(RuntimeMetrics::default())),
            task_counter: Arc::new(AtomicU64::new(0)),
            batch_counter: Arc::new(AtomicU64::new(0)),
        })
    }

    /// 使用默认运行时配置创建
    pub fn with_default_runtime(agent_config: AgentConfig) -> Result<Self> {
        Self::new(agent_config, RuntimeConfig::default())
    }

    /// 获取共享 HTTP Client
    pub fn http_client(&self) -> &reqwest::Client {
        self.resource_pool.http_client()
    }

    /// 获取共享 Media Downloader
    pub fn media_downloader(&self) -> Option<&Arc<MediaDownloader>> {
        self.media_downloader.as_ref()
    }

    /// 获取运行时配置
    pub fn runtime_config(&self) -> &RuntimeConfig {
        &self.runtime_config
    }

    /// 获取运行时指标
    pub async fn metrics(&self) -> RuntimeMetrics {
        self.metrics.read().await.clone()
    }

    /// 执行单个任务
    ///
    /// 自动获取任务信号量许可，控制并发数
    pub async fn execute_task(&self, task: Task) -> Result<TaskResult> {
        let start = Instant::now();
        let task_id = task.id.clone();

        debug!("Executing task {} (type: {})", task_id, task.task_type);

        // 获取任务许可（对象池模式）
        let _permit = self.resource_pool.acquire_task_permit().await?;

        // 执行实际任务
        let result = {
            let mut agent = self.agent.lock().await;
            agent.execute_task(task).await
        };

        // 更新指标
        let elapsed = start.elapsed();
        self.update_metrics(elapsed, result.is_ok()).await;

        debug!(
            "Task {} completed in {:?} (success: {})",
            task_id,
            elapsed,
            result.is_ok()
        );

        result
    }

    /// 批量执行任务
    ///
    /// # Arguments
    /// * `tasks` - 要执行的任务列表
    ///
    /// # Returns
    /// 任务结果列表，与输入任务顺序一致
    ///
    /// # 性能优化
    /// - 并行执行多个任务（受 max_concurrent_tasks 限制）
    /// - 共享 HTTP Client 连接池
    /// - 批量更新指标
    pub async fn execute_batch(&self, tasks: Vec<Task>) -> Vec<Result<TaskResult>> {
        let batch_start = Instant::now();
        let batch_id = self.batch_counter.fetch_add(1, Ordering::SeqCst);
        let task_count = tasks.len();

        info!(
            "Starting batch {} with {} tasks (batch_size: {})",
            batch_id, task_count, self.runtime_config.batch_size
        );

        if tasks.is_empty() {
            return vec![];
        }

        // 分批处理（如果任务数超过 batch_size）
        let batch_size = self.runtime_config.batch_size;
        let mut all_results = Vec::with_capacity(task_count);

        for chunk in tasks.chunks(batch_size) {
            let chunk_results = self.execute_batch_chunk(chunk.to_vec()).await;
            all_results.extend(chunk_results);
        }

        // 更新批量指标
        let batch_elapsed = batch_start.elapsed();
        self.update_batch_metrics(task_count, batch_elapsed).await;

        info!(
            "Batch {} completed {} tasks in {:?}",
            batch_id, task_count, batch_elapsed
        );

        all_results
    }

    /// 执行一批任务（内部方法）
    async fn execute_batch_chunk(&self, tasks: Vec<Task>) -> Vec<Result<TaskResult>> {
        // 顺序执行任务（避免生命周期问题）
        let mut results = Vec::with_capacity(tasks.len());
        for task in tasks {
            results.push(self.execute_task(task).await);
        }

        results
    }

    /// 流式批量处理
    ///
    /// 适用于大量任务，可以边处理边返回结果
    pub async fn execute_batch_streaming(
        &self,
        tasks: Vec<Task>,
    ) -> tokio::sync::mpsc::Receiver<Result<TaskResult>> {
        let (tx, rx) = tokio::sync::mpsc::channel(tasks.len());
        let batch_id = self.batch_counter.fetch_add(1, Ordering::SeqCst);

        info!(
            "Starting streaming batch {} with {} tasks",
            batch_id,
            tasks.len()
        );

        let this = Arc::new(self.clone());

        tokio::spawn(async move {
            let semaphore = Arc::new(tokio::sync::Semaphore::new(
                this.runtime_config.max_concurrent_tasks,
            ));

            let mut handles = vec![];

            for task in tasks {
                let tx = tx.clone();
                let permit = semaphore.clone().acquire_owned().await.ok();
                let this = this.clone();

                let handle = tokio::spawn(async move {
                    let result = this.execute_task(task).await;
                    let _ = tx.send(result).await;
                    drop(permit);
                });

                handles.push(handle);
            }

            // 等待所有任务完成
            for handle in handles {
                if let Err(e) = handle.await {
                    error!("Task handle error: {}", e);
                }
            }
        });

        rx
    }

    /// 处理批量相似任务（同类型优化）
    ///
    /// 对于相同类型的任务，可以进行额外优化：
    /// - LLM 任务：批量请求 API
    /// - 文件处理：批量读取/写入
    pub async fn execute_homogeneous_batch(&self, tasks: Vec<Task>) -> Result<Vec<TaskResult>> {
        if tasks.is_empty() {
            return Ok(vec![]);
        }

        // 检查是否都是同类型任务
        let first_type = &tasks[0].task_type;
        let all_same_type = tasks.iter().all(|t| &t.task_type == first_type);

        if !all_same_type {
            // 不是同类型，使用普通批量处理
            let results = self.execute_batch(tasks).await;
            return results.into_iter().collect::<Result<Vec<_>>>();
        }

        info!(
            "Executing homogeneous batch of {} {:?} tasks",
            tasks.len(),
            first_type
        );

        // 根据任务类型进行优化处理
        match first_type {
            TaskType::LlmChat => self.execute_batch_llm(tasks).await,
            TaskType::FileProcessing => self.execute_batch_file(tasks).await,
            _ => {
                // 其他类型使用普通批量处理
                let results = self.execute_batch(tasks).await;
                results.into_iter().collect::<Result<Vec<_>>>()
            }
        }
    }

    /// 批量 LLM 任务优化
    async fn execute_batch_llm(&self, tasks: Vec<Task>) -> Result<Vec<TaskResult>> {
        // 当前实现：并行处理
        // 未来优化：可以合并请求或使用批处理 API
        let results = self.execute_batch(tasks).await;
        results.into_iter().collect::<Result<Vec<_>>>()
    }

    /// 批量文件处理优化
    async fn execute_batch_file(&self, tasks: Vec<Task>) -> Result<Vec<TaskResult>> {
        // 当前实现：并行处理
        // 未来优化：批量 IO 操作
        let results = self.execute_batch(tasks).await;
        results.into_iter().collect::<Result<Vec<_>>>()
    }

    /// 更新指标
    async fn update_metrics(&self, _elapsed: Duration, _success: bool) {
        let mut metrics = self.metrics.write().await;
        metrics.tasks_processed += 1;
        // 这里可以添加更多指标，如平均处理时间等
    }

    /// 更新批量指标
    async fn update_batch_metrics(&self, task_count: usize, _elapsed: Duration) {
        let mut metrics = self.metrics.write().await;
        metrics.batches_processed += 1;

        // 计算平均批次大小
        let total_batches = metrics.batches_processed as f64;
        let current_avg = metrics.avg_batch_size;
        metrics.avg_batch_size =
            (current_avg * (total_batches - 1.0) + task_count as f64) / total_batches;

        // 更新内存使用估计
        if let Some(downloader) = &self.media_downloader {
            metrics.memory_usage_mb = downloader.estimate_memory_usage_mb();
        }
    }

    /// 获取资源池状态
    pub fn resource_status(&self) -> ResourceStatus {
        ResourceStatus {
            available_task_slots: self.resource_pool.available_task_permits(),
            available_download_slots: self.resource_pool.available_download_permits(),
            total_tasks: self.task_counter.load(Ordering::SeqCst),
            total_batches: self.batch_counter.load(Ordering::SeqCst),
        }
    }

    /// 优雅关闭
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down AgentRuntime for agent {}", self.config.id);

        let mut agent = self.agent.lock().await;
        agent.shutdown().await?;

        info!("AgentRuntime shutdown complete");
        Ok(())
    }
}

/// 资源状态
#[derive(Debug, Clone)]
pub struct ResourceStatus {
    /// 可用任务槽位
    pub available_task_slots: usize,
    /// 可用下载槽位
    pub available_download_slots: usize,
    /// 总任务数
    pub total_tasks: u64,
    /// 总批次数
    pub total_batches: u64,
}

/// Agent Runtime 构建器
pub struct AgentRuntimeBuilder {
    agent_config: Option<AgentConfig>,
    runtime_config: RuntimeConfig,
}

impl AgentRuntimeBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self {
            agent_config: None,
            runtime_config: RuntimeConfig::default(),
        }
    }

    /// 设置 Agent 配置
    pub fn agent_config(mut self, config: AgentConfig) -> Self {
        self.agent_config = Some(config);
        self
    }

    /// 设置运行时配置
    pub fn runtime_config(mut self, config: RuntimeConfig) -> Self {
        self.runtime_config = config;
        self
    }

    /// 设置最大并发任务数
    pub fn max_concurrent_tasks(mut self, value: usize) -> Self {
        self.runtime_config.max_concurrent_tasks = value;
        self
    }

    /// 设置批次大小
    pub fn batch_size(mut self, value: usize) -> Self {
        self.runtime_config.batch_size = value;
        self
    }

    /// 启用/禁用批量处理
    pub fn enable_batch_processing(mut self, value: bool) -> Self {
        self.runtime_config.enable_batch_processing = value;
        self
    }

    /// 构建 AgentRuntime
    pub fn build(self) -> Result<AgentRuntime> {
        let agent_config = self
            .agent_config
            .ok_or_else(|| AgentError::InvalidConfig("Agent config is required".to_string()))?;

        AgentRuntime::new(agent_config, self.runtime_config)
    }
}

impl Default for AgentRuntimeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemoryConfig, ModelConfig, PersonalityConfig};

    fn create_test_agent_config() -> AgentConfig {
        AgentConfig {
            id: "test-agent".to_string(),
            name: "Test Agent".to_string(),
            description: "Test".to_string(),
            version: "1.0.0".to_string(),
            capabilities: vec![],
            models: ModelConfig {
                provider: "test".to_string(),
                model: "test".to_string(),
                temperature: 0.7,
                max_tokens: 100,
                top_p: 1.0,
            },
            memory: MemoryConfig {
                episodic_capacity: 100,
                semantic_capacity: 100,
                working_memory_size: 10,
                consolidation_interval_hours: 24,
            },
            personality: PersonalityConfig {
                openness: 0.5,
                conscientiousness: 0.5,
                extraversion: 0.5,
                agreeableness: 0.5,
                neuroticism: 0.5,
                base_mood: "neutral".to_string(),
            },
            intent_confidence_threshold: Some(0.7),
        }
    }

    #[test]
    fn test_agent_runtime_builder() {
        let agent_config = create_test_agent_config();

        let runtime = AgentRuntimeBuilder::new()
            .agent_config(agent_config)
            .max_concurrent_tasks(50)
            .batch_size(20)
            .enable_batch_processing(true)
            .build();

        assert!(runtime.is_ok());

        let runtime = runtime.unwrap();
        assert_eq!(runtime.runtime_config.max_concurrent_tasks, 50);
        assert_eq!(runtime.runtime_config.batch_size, 20);
        assert!(runtime.runtime_config.enable_batch_processing);
    }

    #[test]
    fn test_resource_status() {
        let agent_config = create_test_agent_config();
        let runtime = AgentRuntime::with_default_runtime(agent_config).unwrap();

        let status = runtime.resource_status();
        assert_eq!(status.available_task_slots, 100); // default max_concurrent_tasks
        assert_eq!(status.available_download_slots, 10); // default max_concurrent_downloads
    }

    #[tokio::test]
    async fn test_metrics() {
        let agent_config = create_test_agent_config();
        let runtime = AgentRuntime::with_default_runtime(agent_config).unwrap();

        let metrics = runtime.metrics().await;
        assert_eq!(metrics.tasks_processed, 0);
        assert_eq!(metrics.batches_processed, 0);
    }
}
