//! Foreign Runtime Task Executor
//!
//! Integrates beebotos-foreign-rt with the Agent runtime's TaskExecutor trait,
//! enabling Agent tasks to execute Python and Node.js scripts.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tracing::{debug, error, info, instrument, warn};

pub use beebotos_foreign_rt::{
    ForeignRuntime, ForeignRuntimeConfig, ScriptResult, ScriptSource, ScriptTask, ScriptTaskBuilder,
    SandboxRequirements,
};
use beebotos_foreign_rt::{
    DefaultForeignRuntimeManager, ForeignRuntimeManager, ForeignRuntimeManagerBuilder,
};

use crate::error::{AgentError, Result};
use crate::runtime::executor::{BatchResult, TaskExecutor};
use crate::{Artifact, Task, TaskResult, TaskType};

/// Foreign runtime task executor
///
/// Wraps the foreign runtime manager to execute Python/Node.js tasks
/// within the Agent runtime framework.
pub struct ForeignTaskExecutor {
    /// Underlying foreign runtime manager
    manager: Arc<DefaultForeignRuntimeManager>,
}

impl ForeignTaskExecutor {
    /// Create a new foreign task executor from configuration
    pub fn new(config: ForeignRuntimeConfig) -> Result<Self> {
        let manager = Arc::new(
            DefaultForeignRuntimeManager::new(config).map_err(|e| {
                AgentError::configuration(format!("Failed to create foreign runtime manager: {}", e))
            })?,
        );

        Ok(Self { manager })
    }

    /// Create a new foreign task executor with an existing manager
    pub fn with_manager(manager: Arc<DefaultForeignRuntimeManager>) -> Self {
        Self { manager }
    }

    /// Convert an Agent Task to a foreign ScriptTask
    fn convert_task(&self, task: &Task) -> Result<ScriptTask> {
        let runtime = match task.task_type {
            TaskType::ForeignPythonWasm | TaskType::ForeignPythonProcess => ForeignRuntime::Python,
            TaskType::ForeignNodeJsWasm | TaskType::ForeignNodeJsProcess => ForeignRuntime::NodeJs,
            _ => {
                return Err(AgentError::UnsupportedTaskType(
                    task.task_type.to_string(),
                ));
            }
        };

        // Extract code from input (for inline execution)
        // In production, this would come from skill metadata
        let source = ScriptSource::Inline {
            code: task.input.clone(),
        };

        let entrypoint = task
            .parameters
            .get("entrypoint")
            .cloned()
            .unwrap_or_else(|| "main".to_string());

        // Parse input as JSON if possible
        let input_value = match serde_json::from_str(&task.input) {
            Ok(val) => val,
            Err(_) => serde_json::json!({"raw_input": task.input}),
        };

        // Build sandbox requirements from parameters
        let mut sandbox = SandboxRequirements::default();
        if let Some(mem) = task.parameters.get("max_memory_mb") {
            if let Ok(mb) = mem.parse::<usize>() {
                sandbox.max_memory_mb = mb;
            }
        }
        if let Some(network) = task.parameters.get("network_allowed") {
            sandbox.network_allowed = network == "true";
        }
        if let Some(gpu) = task.parameters.get("gpu_allowed") {
            sandbox.gpu_allowed = gpu == "true";
        }

        let timeout_secs = task
            .parameters
            .get("timeout_secs")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(30);

        let mut builder = ScriptTask::builder(runtime, &task.id)
            .with_entrypoint(entrypoint)
            .with_input(input_value)
            .with_sandbox(sandbox)
            .with_timeout(Duration::from_secs(timeout_secs));

        builder = match source {
            ScriptSource::Inline { code } => builder.with_inline_code(code),
            ScriptSource::File { path } => builder.with_file(path),
            ScriptSource::Prebuilt { module_id, entrypoint: ep } => {
                // Prebuilt requires special handling; for now, treat as inline error
                return Err(AgentError::UnsupportedTaskType(format!(
                    "Prebuilt module not supported: {}", module_id
                )));
            }
        };

        builder.build()
            .map_err(|e| AgentError::Execution(format!("Failed to build script task: {}", e)))
    }

    /// Convert ScriptResult to Agent TaskResult
    fn convert_result(&self, task_id: &str, result: ScriptResult) -> TaskResult {
        let artifacts = result
            .artifacts
            .into_iter()
            .map(|a| Artifact {
                id: a.id,
                artifact_type: a.name,
                content: a.content,
                mime_type: a.mime_type,
            })
            .collect();

        TaskResult {
            task_id: task_id.to_string(),
            success: result.success,
            output: if result.success {
                result.output.to_string()
            } else {
                result.error.unwrap_or_else(|| "Unknown error".to_string())
            },
            artifacts,
            execution_time_ms: result.execution_time.as_millis() as u64,
        }
    }
}

#[async_trait]
impl TaskExecutor for ForeignTaskExecutor {
    #[instrument(skip(self, task), fields(task_id = %task.id))]
    async fn execute(&self, task: Task) -> Result<TaskResult> {
        let start = Instant::now();
        debug!(task_type = %task.task_type, "Executing foreign runtime task");

        // Convert to script task
        let script_task = self.convert_task(&task)?;

        // Execute via foreign runtime manager
        match self.manager.execute(script_task).await {
            Ok(result) => {
                info!(
                    task_id = %task.id,
                    success = result.success,
                    duration_ms = result.execution_time.as_millis(),
                    "Foreign runtime task completed"
                );
                Ok(self.convert_result(&task.id, result))
            }
            Err(e) => {
                error!(task_id = %task.id, error = %e, "Foreign runtime task failed");
                Ok(TaskResult {
                    task_id: task.id,
                    success: false,
                    output: format!("Foreign runtime error: {}", e),
                    artifacts: vec![],
                    execution_time_ms: start.elapsed().as_millis() as u64,
                })
            }
        }
    }

    async fn execute_batch(&self, tasks: Vec<Task>) -> BatchResult {
        let start = Instant::now();
        let mut result = BatchResult::new();

        for task in tasks {
            let task_id = task.id.clone();
            match self.execute(task).await {
                Ok(task_result) => result.add_success(task_result),
                Err(e) => result.add_failure(task_id, e),
            }
        }

        result.total_duration = start.elapsed();
        result
    }
}

/// Builder for ForeignTaskExecutor
pub struct ForeignTaskExecutorBuilder {
    config: ForeignRuntimeConfig,
}

impl ForeignTaskExecutorBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: ForeignRuntimeConfig::default(),
        }
    }

    /// Set foreign runtime configuration
    pub fn with_config(mut self, config: ForeignRuntimeConfig) -> Self {
        self.config = config;
        self
    }

    /// Build the executor
    pub fn build(self) -> Result<ForeignTaskExecutor> {
        ForeignTaskExecutor::new(self.config)
    }
}

impl Default for ForeignTaskExecutorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_convert_task_python() {
        let executor = ForeignTaskExecutorBuilder::new().build().unwrap();

        let task = Task {
            id: "test-1".to_string(),
            task_type: TaskType::ForeignPythonWasm,
            input: "print('hello')".to_string(),
            parameters: HashMap::new(),
            stream_tx: None,
        };

        let script_task = executor.convert_task(&task).unwrap();
        assert_eq!(script_task.runtime, ForeignRuntime::Python);
        assert_eq!(script_task.entrypoint, "main");
    }

    #[test]
    fn test_convert_task_nodejs() {
        let executor = ForeignTaskExecutorBuilder::new().build().unwrap();

        let mut params = HashMap::new();
        params.insert("entrypoint".to_string(), "handler".to_string());
        params.insert("max_memory_mb".to_string(), "512".to_string());

        let task = Task {
            id: "test-2".to_string(),
            task_type: TaskType::ForeignNodeJsProcess,
            input: "console.log('hello')".to_string(),
            parameters: params,
            stream_tx: None,
        };

        let script_task = executor.convert_task(&task).unwrap();
        assert_eq!(script_task.runtime, ForeignRuntime::NodeJs);
        assert_eq!(script_task.entrypoint, "handler");
        assert_eq!(script_task.sandbox.max_memory_mb, 512);
    }

    #[test]
    fn test_convert_task_unsupported() {
        let executor = ForeignTaskExecutorBuilder::new().build().unwrap();

        let task = Task {
            id: "test-3".to_string(),
            task_type: TaskType::LlmChat,
            input: "hello".to_string(),
            parameters: HashMap::new(),
            stream_tx: None,
        };

        assert!(executor.convert_task(&task).is_err());
    }
}
