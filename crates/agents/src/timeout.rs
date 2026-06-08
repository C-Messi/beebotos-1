//! Timeout Configuration
//!
//! 🟡 MEDIUM FIX: Configurable timeouts for all operations

use std::time::Duration;

/// Timeout configuration
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// Default timeout for operations
    pub default: Duration,
    /// Timeout for network requests
    pub network: Duration,
    /// Timeout for database operations
    pub database: Duration,
    /// Timeout for external API calls
    pub external_api: Duration,
}

impl TimeoutConfig {
    pub fn new(default_secs: u64) -> Self {
        Self {
            default: Duration::from_secs(default_secs),
            network: Duration::from_secs(180),
            database: Duration::from_secs(10),
            external_api: Duration::from_secs(60),
        }
    }

    pub fn with_network(mut self, secs: u64) -> Self {
        self.network = Duration::from_secs(secs);
        self
    }

    pub fn with_database(mut self, secs: u64) -> Self {
        self.database = Duration::from_secs(secs);
        self
    }

    pub fn with_external_api(mut self, secs: u64) -> Self {
        self.external_api = Duration::from_secs(secs);
        self
    }
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self::new(30)
    }
}

/// Execute a future with timeout
#[macro_export]
macro_rules! with_timeout {
    ($timeout:expr, $future:expr) => {
        tokio::time::timeout($timeout, $future)
    };
}
