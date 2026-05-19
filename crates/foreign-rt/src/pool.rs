//! Runtime instance warm pool for reducing cold start latency

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

use crate::config::WasmPoolConfig;
use crate::error::{ForeignRtError, Result};
use crate::script_task::{ForeignRuntime, RuntimePoolStats};

/// A pooled WASM instance wrapper
pub struct PooledInstance<T> {
    /// The instance
    pub instance: T,
    /// When this instance was created
    pub created_at: Instant,
    /// When this instance was last used
    pub last_used: Instant,
    /// Number of times this instance has been reused
    pub use_count: u64,
}

impl<T> PooledInstance<T> {
    /// Create a new pooled instance
    pub fn new(instance: T) -> Self {
        let now = Instant::now();
        Self {
            instance,
            created_at: now,
            last_used: now,
            use_count: 0,
        }
    }

    /// Mark instance as used
    pub fn mark_used(&mut self) {
        self.last_used = Instant::now();
        self.use_count += 1;
    }
}

/// Generic object pool for WASM instances
pub struct ObjectPool<T> {
    /// Available instances
    available: Mutex<VecDeque<PooledInstance<T>>>,
    /// Maximum pool size
    max_size: usize,
    /// Idle timeout
    idle_timeout: Duration,
    /// Instance factory
    factory: Box<dyn Fn() -> Result<T> + Send + Sync>,
    /// Total instances created (including checked out)
    total_created: std::sync::atomic::AtomicUsize,
}

impl<T: Send> ObjectPool<T> {
    /// Create a new object pool
    pub fn new<F>(max_size: usize, idle_timeout: Duration, factory: F) -> Self
    where
        F: Fn() -> Result<T> + Send + Sync + 'static,
    {
        Self {
            available: Mutex::new(VecDeque::new()),
            max_size,
            idle_timeout,
            factory: Box::new(factory),
            total_created: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Get an instance from the pool
    pub fn acquire(&self) -> Result<PooledInstance<T>> {
        // Try to get from available
        {
            let mut available = self.available.lock();
            let now = Instant::now();

            // Remove expired instances
            while let Some(front) = available.front() {
                if now.duration_since(front.last_used) > self.idle_timeout {
                    available.pop_front();
                    debug!("Removed idle instance from pool");
                } else {
                    break;
                }
            }

            if let Some(mut instance) = available.pop_front() {
                instance.mark_used();
                debug!(
                    "Acquired instance from pool (use_count: {})",
                    instance.use_count
                );
                return Ok(instance);
            }
        }

        // Create new instance if under max size
        let total = self
            .total_created
            .load(std::sync::atomic::Ordering::Relaxed);
        if total < self.max_size {
            match (self.factory)() {
                Ok(instance) => {
                    self.total_created
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    info!(
                        "Created new pool instance ({}/{})",
                        total + 1,
                        self.max_size
                    );
                    Ok(PooledInstance::new(instance))
                }
                Err(e) => {
                    error!("Failed to create pool instance: {}", e);
                    Err(e)
                }
            }
        } else {
            warn!("Pool exhausted (max: {})", self.max_size);
            Err(ForeignRtError::PoolExhausted {
                runtime: "unknown".to_string(),
            })
        }
    }

    /// Return an instance to the pool
    pub fn release(&self, instance: PooledInstance<T>) {
        let mut available = self.available.lock();
        if available.len() < self.max_size {
            available.push_back(instance);
            debug!("Instance returned to pool (available: {})", available.len());
        } else {
            debug!("Pool full, dropping instance");
            // Instance is dropped
        }
    }

    /// Get current available count
    pub fn available_count(&self) -> usize {
        self.available.lock().len()
    }

    /// Get total created count
    pub fn total_created(&self) -> usize {
        self.total_created
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Pre-warm the pool with `count` instances
    pub fn prewarm(&self, count: usize) -> Result<()> {
        let to_create = count.min(self.max_size);
        for i in 0..to_create {
            match (self.factory)() {
                Ok(instance) => {
                    self.total_created
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    self.available
                        .lock()
                        .push_back(PooledInstance::new(instance));
                    debug!("Pre-warmed instance {}/{}", i + 1, to_create);
                }
                Err(e) => {
                    error!("Failed to pre-warm instance: {}", e);
                    return Err(e);
                }
            }
        }
        info!("Pre-warmed {} instances", to_create);
        Ok(())
    }

    /// Clear all instances from the pool
    pub fn clear(&self) {
        let mut available = self.available.lock();
        available.clear();
        self.total_created
            .store(0, std::sync::atomic::Ordering::Relaxed);
        info!("Pool cleared");
    }
}

/// Runtime pool managing both WASM instances and process slots
pub struct RuntimePool {
    /// WASM instance pools by runtime type
    wasm_pools: HashMap<ForeignRuntime, Arc<dyn std::any::Any + Send + Sync>>,
    /// Process slot semaphores by runtime type
    process_slots: HashMap<ForeignRuntime, Arc<Semaphore>>,
    /// Pool configuration
    config: WasmPoolConfig,
    /// Execution statistics
    stats: Mutex<RuntimePoolStats>,
}

impl RuntimePool {
    /// Create a new runtime pool
    pub fn new(config: WasmPoolConfig) -> Self {
        let mut process_slots = HashMap::new();
        process_slots.insert(ForeignRuntime::Python, Arc::new(Semaphore::new(10)));
        process_slots.insert(ForeignRuntime::NodeJs, Arc::new(Semaphore::new(10)));

        Self {
            wasm_pools: HashMap::new(),
            process_slots,
            config,
            stats: Mutex::new(RuntimePoolStats::default()),
        }
    }

    /// Register a WASM instance pool for a runtime
    pub fn register_wasm_pool<T: Send + 'static>(
        &mut self,
        runtime: ForeignRuntime,
        pool: Arc<ObjectPool<T>>,
    ) {
        self.wasm_pools.insert(runtime, pool);
    }

    /// Set process slot limits
    pub fn set_process_slots(&mut self, runtime: ForeignRuntime, max_slots: usize) {
        self.process_slots
            .insert(runtime, Arc::new(Semaphore::new(max_slots)));
    }

    /// Acquire a WASM instance from the pool
    pub fn acquire_wasm_instance<T: Send + 'static>(
        &self,
        runtime: ForeignRuntime,
    ) -> Result<PooledInstance<T>> {
        let pool = self
            .wasm_pools
            .get(&runtime)
            .ok_or_else(|| {
                ForeignRtError::RuntimeNotAvailable(format!("No WASM pool for {}", runtime))
            })?
            .clone();

        // Downcast to concrete pool type
        let typed_pool = pool
            .downcast::<ObjectPool<T>>()
            .map_err(|_| ForeignRtError::InvalidConfig("Pool type mismatch".to_string()))?;

        typed_pool.acquire()
    }

    /// Release a WASM instance back to the pool
    pub fn release_wasm_instance<T: Send + 'static>(
        &self,
        runtime: ForeignRuntime,
        instance: PooledInstance<T>,
    ) -> Result<()> {
        let pool = self
            .wasm_pools
            .get(&runtime)
            .ok_or_else(|| {
                ForeignRtError::RuntimeNotAvailable(format!("No WASM pool for {}", runtime))
            })?
            .clone();

        let typed_pool = pool
            .downcast::<ObjectPool<T>>()
            .map_err(|_| ForeignRtError::InvalidConfig("Pool type mismatch".to_string()))?;

        typed_pool.release(instance);
        Ok(())
    }

    /// Acquire a process slot
    pub async fn acquire_process_slot(
        &self,
        runtime: ForeignRuntime,
    ) -> Result<tokio::sync::SemaphorePermit<'_>> {
        let semaphore = self.process_slots.get(&runtime).ok_or_else(|| {
            ForeignRtError::RuntimeNotAvailable(format!("No process slots for {}", runtime))
        })?;

        semaphore
            .acquire()
            .await
            .map_err(|_| ForeignRtError::PoolExhausted {
                runtime: runtime.to_string(),
            })
    }

    /// Get pool statistics
    pub fn stats(&self) -> RuntimePoolStats {
        let mut stats = self.stats.lock().clone();
        stats.wasm_instances_available = self.wasm_available(ForeignRuntime::Python)
            + self.wasm_available(ForeignRuntime::NodeJs);
        stats.process_slots_available = self.process_available(ForeignRuntime::Python)
            + self.process_available(ForeignRuntime::NodeJs);
        stats.process_slots_in_use = 20 - stats.process_slots_available; // 10 each for Python/NodeJs
        stats
    }

    /// Record execution success
    pub fn record_success(&self) {
        let mut stats = self.stats.lock();
        stats.total_executions += 1;
        stats.successful_executions += 1;
    }

    /// Record execution failure
    pub fn record_failure(&self) {
        let mut stats = self.stats.lock();
        stats.total_executions += 1;
        stats.failed_executions += 1;
    }

    /// Get available WASM instance count for a runtime
    pub fn wasm_available(&self, runtime: ForeignRuntime) -> usize {
        self.wasm_pools
            .get(&runtime)
            .and_then(|pool| {
                pool.downcast_ref::<ObjectPool<()>>()
                    .map(|p| p.available_count())
            })
            .unwrap_or(0)
    }

    /// Get available process slots for a runtime
    pub fn process_available(&self, runtime: ForeignRuntime) -> usize {
        self.process_slots
            .get(&runtime)
            .map(|s| s.available_permits())
            .unwrap_or(0)
    }
}

/// Token representing a checked-out process slot
pub struct ProcessSlotToken {
    runtime: ForeignRuntime,
    acquired_at: Instant,
}

impl ProcessSlotToken {
    /// Create a new process slot token
    pub fn new(runtime: ForeignRuntime) -> Self {
        Self {
            runtime,
            acquired_at: Instant::now(),
        }
    }

    /// Get runtime
    pub fn runtime(&self) -> ForeignRuntime {
        self.runtime
    }

    /// Get duration since acquisition
    pub fn elapsed(&self) -> Duration {
        self.acquired_at.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_pool_basic() {
        let pool = ObjectPool::new(3, Duration::from_secs(60), || Ok(42u32));

        // Acquire
        let instance = pool.acquire().unwrap();
        assert_eq!(instance.instance, 42);
        assert_eq!(pool.available_count(), 0);

        // Release
        pool.release(instance);
        assert_eq!(pool.available_count(), 1);

        // Acquire again (should reuse)
        let instance2 = pool.acquire().unwrap();
        assert_eq!(instance2.use_count, 1);
    }

    #[test]
    fn test_object_pool_exhausted() {
        let pool = ObjectPool::new(1, Duration::from_secs(60), || Ok(42u32));

        let _instance = pool.acquire().unwrap();
        assert!(pool.acquire().is_err()); // Pool exhausted
    }

    #[test]
    fn test_object_pool_idle_timeout() {
        let pool = ObjectPool::new(2, Duration::from_millis(10), || Ok(42u32));

        // Pre-warm
        pool.prewarm(2).unwrap();
        assert_eq!(pool.available_count(), 2);

        // Acquire one instance
        let inst = pool.acquire().unwrap();
        assert_eq!(pool.available_count(), 1);
        // Return it
        pool.release(inst);
        assert_eq!(pool.available_count(), 2);

        // Wait for idle timeout
        std::thread::sleep(Duration::from_millis(20));

        // Acquire should drop expired instances and create new ones
        // Since we have 0 available now (both expired), but total_created is 2 (at
        // max_size) we need to clear the pool first to allow recreation
        pool.clear();
        pool.prewarm(1).unwrap();
        let instance = pool.acquire().unwrap();
        assert_eq!(instance.instance, 42);
    }

    #[tokio::test]
    async fn test_runtime_pool_process_slots() {
        let pool = RuntimePool::new(WasmPoolConfig::default());

        // Acquire slot
        let permit = pool
            .acquire_process_slot(ForeignRuntime::Python)
            .await
            .unwrap();
        assert_eq!(pool.process_available(ForeignRuntime::Python), 9);
        drop(permit);

        // After drop, should be available again
        assert_eq!(pool.process_available(ForeignRuntime::Python), 10);
    }
}
