//! Unified resource metering (Gas system) for foreign runtimes
//!
//! All resource consumption across WASM and process paths is converted
//! into BeeBotOS Gas units for consistent accounting.

use serde::{Deserialize, Serialize};

/// Gas report for foreign runtime execution
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ForeignGasReport {
    /// Compute gas (CPU instructions / time)
    pub compute_gas: u64,
    /// Memory gas (memory bytes × time)
    pub memory_gas: u64,
    /// IO gas (disk read/write bytes)
    pub io_gas: u64,
    /// Network gas (outbound bytes)
    pub network_gas: u64,
    /// Storage gas (KV operations)
    pub storage_gas: u64,
}

impl ForeignGasReport {
    /// Create a new empty gas report
    pub fn new() -> Self {
        Self::default()
    }

    /// Total gas consumed
    pub fn total(&self) -> u64 {
        self.compute_gas
            .saturating_add(self.memory_gas)
            .saturating_add(self.io_gas)
            .saturating_add(self.network_gas)
            .saturating_add(self.storage_gas)
    }

    /// Add compute gas
    pub fn add_compute(&mut self, amount: u64) {
        self.compute_gas = self.compute_gas.saturating_add(amount);
    }

    /// Add memory gas
    pub fn add_memory(&mut self, amount: u64) {
        self.memory_gas = self.memory_gas.saturating_add(amount);
    }

    /// Add IO gas
    pub fn add_io(&mut self, amount: u64) {
        self.io_gas = self.io_gas.saturating_add(amount);
    }

    /// Add network gas
    pub fn add_network(&mut self, amount: u64) {
        self.network_gas = self.network_gas.saturating_add(amount);
    }

    /// Add storage gas
    pub fn add_storage(&mut self, amount: u64) {
        self.storage_gas = self.storage_gas.saturating_add(amount);
    }

    /// Merge another gas report into this one
    pub fn merge(&mut self, other: &ForeignGasReport) {
        self.compute_gas = self.compute_gas.saturating_add(other.compute_gas);
        self.memory_gas = self.memory_gas.saturating_add(other.memory_gas);
        self.io_gas = self.io_gas.saturating_add(other.io_gas);
        self.network_gas = self.network_gas.saturating_add(other.network_gas);
        self.storage_gas = self.storage_gas.saturating_add(other.storage_gas);
    }
}

/// Gas oracle for converting resource usage to gas units
pub trait GasOracle: Send + Sync {
    /// Convert CPU microseconds to compute gas
    fn cpu_time_to_gas(&self, cpu_usec: u64) -> u64;
    /// Convert memory bytes × milliseconds to memory gas
    fn memory_time_to_gas(&self, bytes: u64, millis: u64) -> u64;
    /// Convert IO bytes to IO gas
    fn io_bytes_to_gas(&self, bytes: u64) -> u64;
    /// Convert network bytes to network gas
    fn network_bytes_to_gas(&self, bytes: u64) -> u64;
    /// Convert storage operations to storage gas
    fn storage_ops_to_gas(&self, ops: u64) -> u64;
}

/// Standard gas oracle with fixed conversion rates
pub struct StandardGasOracle {
    /// Compute gas per CPU microsecond
    compute_rate: u64,
    /// Memory gas per byte-millisecond
    memory_rate: u64,
    /// IO gas per byte
    io_rate: u64,
    /// Network gas per byte
    network_rate: u64,
    /// Storage gas per operation
    storage_rate: u64,
}

impl StandardGasOracle {
    /// Create a new standard gas oracle with default rates
    pub fn new() -> Self {
        Self {
            // 1 gas per 10us CPU time
            compute_rate: 100,
            // 1 gas per 1MB * 1s = 1 gas per 1GB*ms, scaled
            memory_rate: 1,
            // 1 gas per 1KB IO
            io_rate: 1,
            // 1 gas per 100B network
            network_rate: 10,
            // 100 gas per storage op
            storage_rate: 100,
        }
    }

    /// Create with custom rates
    pub fn with_rates(
        compute_rate: u64,
        memory_rate: u64,
        io_rate: u64,
        network_rate: u64,
        storage_rate: u64,
    ) -> Self {
        Self {
            compute_rate,
            memory_rate,
            io_rate,
            network_rate,
            storage_rate,
        }
    }
}

impl Default for StandardGasOracle {
    fn default() -> Self {
        Self::new()
    }
}

impl GasOracle for StandardGasOracle {
    fn cpu_time_to_gas(&self, cpu_usec: u64) -> u64 {
        cpu_usec.saturating_mul(self.compute_rate)
    }

    fn memory_time_to_gas(&self, bytes: u64, millis: u64) -> u64 {
        // Scale: bytes * millis / 1GB*ms
        let gb_ms = bytes.saturating_mul(millis) / (1024 * 1024 * 1024);
        gb_ms.saturating_mul(self.memory_rate)
    }

    fn io_bytes_to_gas(&self, bytes: u64) -> u64 {
        bytes.saturating_mul(self.io_rate) / 1024
    }

    fn network_bytes_to_gas(&self, bytes: u64) -> u64 {
        bytes.saturating_mul(self.network_rate) / 100
    }

    fn storage_ops_to_gas(&self, ops: u64) -> u64 {
        ops.saturating_mul(self.storage_rate)
    }
}

/// Gas limit configuration
#[derive(Debug, Clone, Copy)]
pub struct GasLimit {
    /// Maximum total gas allowed
    pub max_total: u64,
    /// Maximum compute gas
    pub max_compute: u64,
    /// Maximum memory gas
    pub max_memory: u64,
    /// Maximum network gas
    pub max_network: u64,
}

impl GasLimit {
    /// Create a new gas limit
    pub fn new(max_total: u64) -> Self {
        Self {
            max_total,
            max_compute: u64::MAX,
            max_memory: u64::MAX,
            max_network: u64::MAX,
        }
    }

    /// With compute limit
    pub fn with_compute_limit(mut self, limit: u64) -> Self {
        self.max_compute = limit;
        self
    }

    /// With memory limit
    pub fn with_memory_limit(mut self, limit: u64) -> Self {
        self.max_memory = limit;
        self
    }

    /// With network limit
    pub fn with_network_limit(mut self, limit: u64) -> Self {
        self.max_network = limit;
        self
    }

    /// Check if a gas report exceeds limits
    pub fn check(&self, report: &ForeignGasReport) -> Result<(), crate::error::ForeignRtError> {
        if report.total() > self.max_total {
            return Err(crate::error::ForeignRtError::ResourceLimitExceeded {
                limit: "total_gas".to_string(),
                used: report.total(),
                max: self.max_total,
            });
        }
        if report.compute_gas > self.max_compute {
            return Err(crate::error::ForeignRtError::ResourceLimitExceeded {
                limit: "compute_gas".to_string(),
                used: report.compute_gas,
                max: self.max_compute,
            });
        }
        if report.memory_gas > self.max_memory {
            return Err(crate::error::ForeignRtError::ResourceLimitExceeded {
                limit: "memory_gas".to_string(),
                used: report.memory_gas,
                max: self.max_memory,
            });
        }
        if report.network_gas > self.max_network {
            return Err(crate::error::ForeignRtError::ResourceLimitExceeded {
                limit: "network_gas".to_string(),
                used: report.network_gas,
                max: self.max_network,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gas_report_total() {
        let mut report = ForeignGasReport::new();
        report.add_compute(100);
        report.add_memory(50);
        report.add_io(25);

        assert_eq!(report.total(), 175);
    }

    #[test]
    fn test_gas_oracle_conversion() {
        let oracle = StandardGasOracle::new();

        // 1ms CPU = 1000us * 100 = 100000 gas
        assert_eq!(oracle.cpu_time_to_gas(1000), 100_000);

        // 1KB IO = 1024 bytes * 1 / 1024 = 1 gas
        assert_eq!(oracle.io_bytes_to_gas(1024), 1);

        // 100B network = 100 * 10 / 100 = 10 gas
        assert_eq!(oracle.network_bytes_to_gas(100), 10);
    }

    #[test]
    fn test_gas_limit_check() {
        let limit = GasLimit::new(1000).with_compute_limit(500);

        let mut report = ForeignGasReport::new();
        report.add_compute(400);
        assert!(limit.check(&report).is_ok());

        report.add_compute(200);
        assert!(limit.check(&report).is_err());
    }
}
