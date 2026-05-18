//! Cgroup v2 resource control for process sandbox

use std::path::PathBuf;

use tracing::{debug, info, warn};

use crate::error::{ForeignRtError, Result};

#[cfg(target_os = "linux")]
fn kill_process(pid: i32) {
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
}

#[cfg(not(target_os = "linux"))]
fn kill_process(_pid: i32) {}

/// Cgroup controller for managing process resource limits
pub struct CgroupController {
    /// Base cgroup path
    base_path: PathBuf,
    /// Whether cgroup v2 is available
    available: bool,
}

impl CgroupController {
    /// Create a new cgroup controller
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        let base_path = base_path.into();
        let available = Self::check_cgroup_v2_available();

        if available {
            info!(path = ?base_path, "Cgroup v2 controller initialized");
        } else {
            warn!("Cgroup v2 not available, resource limits will use fallback mechanisms");
        }

        Self {
            base_path,
            available,
        }
    }

    /// Check if cgroup v2 is available on this system
    fn check_cgroup_v2_available() -> bool {
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
        #[cfg(target_os = "linux")]
        {
            std::path::Path::new("/sys/fs/cgroup/cgroup.controllers").exists()
        }
    }

    /// Check if cgroup v2 is available
    pub fn is_available(&self) -> bool {
        self.available
    }

    /// Create a cgroup for a task
    pub async fn create_cgroup(&self, task_id: &str) -> Result<CgroupHandle> {
        if !self.available {
            return Ok(CgroupHandle::noop(task_id));
        }

        let cgroup_path = self.base_path.join(format!("task-{}", task_id));

        // Create cgroup directory
        tokio::fs::create_dir_all(&cgroup_path).await.map_err(|e| {
            ForeignRtError::ProcessSandbox(format!("Failed to create cgroup directory: {}", e))
        })?;

        debug!(path = ?cgroup_path, "Created cgroup");

        Ok(CgroupHandle {
            path: cgroup_path,
            task_id: task_id.to_string(),
            active: true,
        })
    }

    /// Get the base path
    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }
}

/// Handle to a created cgroup
pub struct CgroupHandle {
    /// Cgroup filesystem path
    path: PathBuf,
    /// Task ID
    task_id: String,
    /// Whether the cgroup is still active
    active: bool,
}

impl CgroupHandle {
    /// Create a no-op handle (when cgroups unavailable)
    pub fn noop(task_id: &str) -> Self {
        Self {
            path: PathBuf::new(),
            task_id: task_id.to_string(),
            active: false,
        }
    }

    /// Set memory limit (memory.max)
    pub async fn set_memory_limit(&self, bytes: u64) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        let limit_path = self.path.join("memory.max");
        tokio::fs::write(&limit_path, bytes.to_string())
            .await
            .map_err(|e| {
                ForeignRtError::ProcessSandbox(format!("Failed to set memory limit: {}", e))
            })?;

        debug!(bytes, "Set cgroup memory limit");
        Ok(())
    }

    /// Set memory high watermark (memory.high)
    pub async fn set_memory_high(&self, bytes: u64) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        let high_path = self.path.join("memory.high");
        tokio::fs::write(&high_path, bytes.to_string())
            .await
            .map_err(|e| {
                ForeignRtError::ProcessSandbox(format!("Failed to set memory high: {}", e))
            })?;

        debug!(bytes, "Set cgroup memory high");
        Ok(())
    }

    /// Set CPU weight (cpu.weight)
    pub async fn set_cpu_weight(&self, weight: u32) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        let weight_path = self.path.join("cpu.weight");
        tokio::fs::write(&weight_path, weight.to_string())
            .await
            .map_err(|e| {
                ForeignRtError::ProcessSandbox(format!("Failed to set CPU weight: {}", e))
            })?;

        debug!(weight, "Set cgroup CPU weight");
        Ok(())
    }

    /// Set PID limit (pids.max)
    pub async fn set_pid_limit(&self, max_pids: u32) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        let pids_path = self.path.join("pids.max");
        tokio::fs::write(&pids_path, max_pids.to_string())
            .await
            .map_err(|e| {
                ForeignRtError::ProcessSandbox(format!("Failed to set PID limit: {}", e))
            })?;

        debug!(max_pids, "Set cgroup PID limit");
        Ok(())
    }

    /// Add a process to this cgroup
    pub async fn add_process(&self, pid: u32) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        let procs_path = self.path.join("cgroup.procs");
        tokio::fs::write(&procs_path, pid.to_string())
            .await
            .map_err(|e| {
                ForeignRtError::ProcessSandbox(format!("Failed to add process to cgroup: {}", e))
            })?;

        debug!(pid, "Added process to cgroup");
        Ok(())
    }

    /// Read memory peak usage
    pub async fn read_memory_peak(&self) -> Result<u64> {
        if !self.active {
            return Ok(0);
        }

        let peak_path = self.path.join("memory.peak");
        match tokio::fs::read_to_string(&peak_path).await {
            Ok(content) => content.trim().parse::<u64>().map_err(|e| {
                ForeignRtError::ProcessSandbox(format!("Failed to parse memory.peak: {}", e))
            }),
            Err(_) => Ok(0),
        }
    }

    /// Read CPU usage in microseconds
    pub async fn read_cpu_usage_usec(&self) -> Result<u64> {
        if !self.active {
            return Ok(0);
        }

        let stat_path = self.path.join("cpu.stat");
        match tokio::fs::read_to_string(&stat_path).await {
            Ok(content) => {
                for line in content.lines() {
                    if line.starts_with("usage_usec ") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            return parts[1].parse::<u64>().map_err(|e| {
                                ForeignRtError::ProcessSandbox(format!(
                                    "Failed to parse cpu.stat: {}",
                                    e
                                ))
                            });
                        }
                    }
                }
                Ok(0)
            }
            Err(_) => Ok(0),
        }
    }

    /// Destroy the cgroup
    pub async fn destroy(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        // Kill all processes in the cgroup first
        let procs_path = self.path.join("cgroup.procs");
        if let Ok(content) = tokio::fs::read_to_string(&procs_path).await {
            for line in content.lines() {
                if let Ok(pid) = line.parse::<i32>() {
                    kill_process(pid);
                }
            }
        }

        // Remove cgroup directory
        match tokio::fs::remove_dir(&self.path).await {
            Ok(_) => {
                debug!("Destroyed cgroup");
            }
            Err(e) => {
                warn!("Failed to remove cgroup directory: {}", e);
            }
        }

        self.active = false;
        Ok(())
    }
}

impl Drop for CgroupHandle {
    fn drop(&mut self) {
        if self.active {
            // Best-effort cleanup in blocking context
            let procs_path = self.path.join("cgroup.procs");
            if let Ok(content) = std::fs::read_to_string(&procs_path) {
                for line in content.lines() {
                    if let Ok(pid) = line.parse::<i32>() {
                        kill_process(pid);
                    }
                }
            }
            let _ = std::fs::remove_dir(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cgroup_availability() {
        let controller = CgroupController::new("/sys/fs/cgroup/beebotos-test");
        // Just verify it doesn't panic
        assert!(!controller.base_path().as_os_str().is_empty());
    }

    #[test]
    fn test_noop_handle() {
        let handle = CgroupHandle::noop("test");
        assert!(!handle.active);
    }
}
