//! Process sandbox execution path for foreign runtimes
//!
//! This module handles execution of Python and Node.js in isolated
//! OS processes using namespaces, seccomp, and cgroups.

pub mod cgroup;
pub mod sandbox;
pub mod seccomp;

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tracing::{debug, error, info, warn};

use crate::config::{ProcessPathConfig, SecurityConfig};
use crate::error::{ForeignRtError, Result};
use crate::metering::{ForeignGasReport, GasOracle, StandardGasOracle};
use crate::script_task::{ForeignRuntime, LogEntry, LogLevel, ScriptResult, ScriptTask};
use crate::wasm_path::WasmExecutorUtils;

/// Process sandbox executor
pub struct ProcessSandboxExecutor {
    /// Configuration
    config: ProcessPathConfig,
    /// Security configuration
    security: SecurityConfig,
    /// Gas oracle
    gas_oracle: Arc<dyn GasOracle>,
}

impl ProcessSandboxExecutor {
    /// Create a new process sandbox executor
    pub fn new(config: ProcessPathConfig, security: SecurityConfig) -> Self {
        Self {
            config,
            security,
            gas_oracle: Arc::new(StandardGasOracle::new()),
        }
    }

    /// Check if process path is available for a runtime
    pub fn is_available(&self, runtime: ForeignRuntime) -> bool {
        match runtime {
            ForeignRuntime::Python => self.config.python_rootfs.is_some(),
            ForeignRuntime::NodeJs => self.config.nodejs_rootfs.is_some(),
        }
    }

    /// Get interpreter path inside rootfs
    fn get_interpreter_path(&self, runtime: ForeignRuntime) -> PathBuf {
        match runtime {
            ForeignRuntime::Python => PathBuf::from("/opt/python/bin/python3"),
            ForeignRuntime::NodeJs => PathBuf::from("/opt/nodejs/bin/node"),
        }
    }

    /// Get rootfs path for runtime
    fn get_rootfs(&self, runtime: ForeignRuntime) -> Option<&PathBuf> {
        match runtime {
            ForeignRuntime::Python => self.config.python_rootfs.as_ref(),
            ForeignRuntime::NodeJs => self.config.nodejs_rootfs.as_ref(),
        }
    }

    /// Build the sandboxed command
    fn build_command(
        &self,
        runtime: ForeignRuntime,
        script_path: &PathBuf,
        _sandbox: &crate::script_task::SandboxRequirements,
    ) -> Result<Command> {
        let rootfs = self.get_rootfs(runtime).ok_or_else(|| {
            ForeignRtError::RuntimeNotAvailable(format!(
                "No rootfs configured for {}",
                runtime.name()
            ))
        })?;

        // For now, we use a simplified approach with direct command execution
        // In production, this would use nsjail or similar to set up namespaces
        let interpreter = self.get_interpreter_path(runtime);

        // Create a wrapper script that chroots or uses namespaces
        // TODO: Full nsjail integration with config generation
        let mut cmd = Command::new("unshare");
        cmd.arg("--fork")
            .arg("--pid")
            .arg("--mount-proc")
            .arg("--map-root-user")
            .arg(interpreter)
            .arg(script_path);

        // Set up environment
        cmd.env_clear();
        cmd.env("BEE_RUNTIME", runtime.name());
        cmd.env("BEE_TASK_ID", "unknown"); // Will be set per-task
        cmd.env("HOME", "/tmp");
        cmd.env("PATH", "/usr/local/bin:/usr/bin:/bin");

        // Resource limits via ulimit (soft limits)
        // TODO: Use cgroup v2 for proper resource control
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        debug!(
            runtime = runtime.name(),
            rootfs = ?rootfs,
            "Built sandboxed process command"
        );

        Ok(cmd)
    }

    /// Write script to a temporary file
    async fn prepare_script_file(
        &self,
        runtime: ForeignRuntime,
        task: &ScriptTask,
    ) -> Result<PathBuf> {
        let code = match &task.source {
            crate::script_task::ScriptSource::Inline { code } => code.clone(),
            crate::script_task::ScriptSource::File { path } => {
                tokio::fs::read_to_string(path)
                    .await
                    .map_err(|e| ForeignRtError::Io(format!("Failed to read script file: {}", e)))?
            }
            crate::script_task::ScriptSource::Prebuilt { module_id, .. } => {
                return Err(ForeignRtError::InvalidConfig(format!(
                    "Prebuilt modules not yet supported in process path: {}",
                    module_id
                )));
            }
        };

        let temp_dir = std::env::temp_dir().join("beebotos-foreign-rt");
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .map_err(|e| ForeignRtError::Io(format!("Failed to create temp dir: {}", e)))?;

        let ext = runtime.extension();
        let script_path = temp_dir.join(format!("{}.{}", task.task_id, ext));

        tokio::fs::write(&script_path, code)
            .await
            .map_err(|e| ForeignRtError::Io(format!("Failed to write script: {}", e)))?;

        debug!(path = ?script_path, "Prepared script file");
        Ok(script_path)
    }

    /// Execute a task in a sandboxed process
    pub async fn execute(&self, task: &ScriptTask) -> Result<ScriptResult> {
        let start = Instant::now();

        if !self.is_available(task.runtime) {
            return Err(ForeignRtError::RuntimeNotAvailable(format!(
                "Process path not available for {}",
                task.runtime
            )));
        }

        info!(
            task_id = %task.task_id,
            runtime = task.runtime.name(),
            "Executing script via process sandbox"
        );

        // Prepare script file
        let script_path = self.prepare_script_file(task.runtime, task).await?;

        // Build and spawn command
        let mut cmd = self.build_command(task.runtime, &script_path, &task.sandbox)?;
        cmd.env("BEE_TASK_ID", &task.task_id);
        if let Some(ref agent_id) = task.agent_id {
            cmd.env("BEE_AGENT_ID", agent_id);
        }

        // Set timeout
        let timeout = task.timeout;

        debug!(task_id = %task.task_id, "Spawning sandboxed process");

        let mut child = cmd.spawn().map_err(|e| {
            ForeignRtError::ProcessSandbox(format!("Failed to spawn process: {}", e))
        })?;

        // Write input to stdin if needed (for future stdin-based input)
        if let Some(mut stdin) = child.stdin.take() {
            let input_json = serde_json::to_string(&task.input).unwrap_or_default();
            let _ = stdin.write_all(input_json.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }

        // Wait for completion with timeout
        let result = tokio::time::timeout(timeout, child.wait()).await;

        let mut stdout = String::new();
        let mut stderr = String::new();

        if let Ok(Ok(status)) = result {
            // Read stdout/stderr
            if let Some(mut out) = child.stdout.take() {
                let _ = out.read_to_string(&mut stdout).await;
            }
            if let Some(mut err) = child.stderr.take() {
                let _ = err.read_to_string(&mut stderr).await;
            }

            // Clean up temp file
            let _ = tokio::fs::remove_file(&script_path).await;

            let execution_time = start.elapsed();
            let logs = WasmExecutorUtils::parse_logs(&stderr);

            if status.success() {
                let output = WasmExecutorUtils::parse_output(&stdout)?;
                let mut gas_report = ForeignGasReport::new();
                // TODO: Read actual cgroup stats for CPU/memory usage
                gas_report.add_compute(execution_time.as_millis() as u64 * 100);

                info!(
                    task_id = %task.task_id,
                    duration_ms = execution_time.as_millis(),
                    "Process sandbox execution completed"
                );

                Ok(ScriptResult::success(&task.task_id, output, execution_time)
                    .with_logs(logs)
                    .with_gas_report(gas_report))
            } else {
                let error_msg = format!(
                    "Process exited with code {:?}. stderr: {}",
                    status.code(),
                    stderr.trim()
                );
                warn!(
                    task_id = %task.task_id,
                    error = %error_msg,
                    "Process sandbox execution failed"
                );

                Ok(ScriptResult::failure(&task.task_id, error_msg, execution_time).with_logs(logs))
            }
        } else {
            // Timeout or spawn error
            let _ = child.start_kill();
            let _ = tokio::fs::remove_file(&script_path).await;

            let execution_time = start.elapsed();
            let logs = WasmExecutorUtils::parse_logs(&stderr);

            if result.is_err() {
                Err(ForeignRtError::Timeout(timeout))
            } else {
                Ok(ScriptResult::failure(
                    &task.task_id,
                    "Process execution failed",
                    execution_time,
                )
                .with_logs(logs))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script_task::{ScriptSource, SandboxRequirements};

    #[test]
    fn test_is_available() {
        let config = ProcessPathConfig {
            python_rootfs: Some("/var/lib/python".into()),
            nodejs_rootfs: None,
            ..Default::default()
        };
        let executor = ProcessSandboxExecutor::new(config, SecurityConfig::default());

        assert!(executor.is_available(ForeignRuntime::Python));
        assert!(!executor.is_available(ForeignRuntime::NodeJs));
    }

    #[tokio::test]
    async fn test_prepare_script_file() {
        let config = ProcessPathConfig::default();
        let executor = ProcessSandboxExecutor::new(config, SecurityConfig::default());

        let task = ScriptTask {
            task_id: "test-script-1".to_string(),
            runtime: ForeignRuntime::Python,
            source: ScriptSource::Inline {
                code: "print('hello')".to_string(),
            },
            entrypoint: "main".to_string(),
            input: serde_json::Value::Null,
            sandbox: SandboxRequirements::default(),
            permissions: vec![],
            timeout: Duration::from_secs(30),
            agent_id: None,
        };

        let path = executor.prepare_script_file(ForeignRuntime::Python, &task).await.unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().ends_with(".py"));

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "print('hello')");

        // Cleanup
        let _ = tokio::fs::remove_file(&path).await;
    }
}
