//! ConfigCenter Integration (Simplified)
//!
//! Provides hot-reload for Gateway configuration using the local
//! BeeBotOSConfig TOML file as the source of truth.

use tokio::sync::RwLock;
use tracing::info;

/// Simplified configuration manager for Gateway hot-reload
pub struct GatewayConfigManager {
    /// Current configuration (protected by RwLock for safe reload)
    config: RwLock<crate::config::BeeBotOSConfig>,
    /// Path to the configuration file
    source_path: Option<std::path::PathBuf>,
}

impl GatewayConfigManager {
    /// Create from an already-loaded config
    pub fn new(config: crate::config::BeeBotOSConfig) -> Self {
        Self {
            config: RwLock::new(config),
            source_path: Some(std::path::PathBuf::from("config/beebotos.toml")),
        }
    }

    /// Get a read lock on the current config
    pub async fn config(&self) -> tokio::sync::RwLockReadGuard<'_, crate::config::BeeBotOSConfig> {
        self.config.read().await
    }

    /// Get the source configuration file path
    pub fn source_path(&self) -> Option<&std::path::PathBuf> {
        self.source_path.as_ref()
    }

    /// Check if reload is possible
    pub fn can_reload(&self) -> bool {
        self.source_path
            .as_ref()
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    /// Reload configuration from the TOML source file
    ///
    /// Returns true if the configuration was actually changed.
    pub async fn reload(&self) -> Result<bool, ConfigError> {
        let path = self
            .source_path
            .as_ref()
            .ok_or_else(|| ConfigError::NoSource)?;

        info!("Reloading configuration from {:?}...", path);

        // Use the same loading logic as startup (BeeBotOSConfig::load)
        // to ensure environment variables are merged correctly.
        // Temporarily set the working directory to the config file's parent
        // so BeeBotOSConfig::load() finds the correct file.
        let new_config = {
            // BeeBotOSConfig::load() expects the current directory to be the
            // project root (it looks for "config/beebotos.toml"). Since `path`
            // is "config/beebotos.toml", we need to switch to its grandparent
            // (i.e. the directory that contains the "config" folder).
            let config_dir = path
                .parent()
                .and_then(|p| p.parent())
                .unwrap_or_else(|| std::path::Path::new("."));
            let _guard = std::env::current_dir().ok().and_then(|cwd| {
                std::env::set_current_dir(&cwd.join(config_dir)).ok();
                Some(cwd)
            });
            let result = crate::config::BeeBotOSConfig::load()
                .map_err(|e| ConfigError::Parse(e.to_string()))?;
            if let Some(original_cwd) = _guard {
                let _ = std::env::set_current_dir(original_cwd);
            }
            result
        };

        let mut config = self.config.write().await;

        // Compare serialized forms to detect changes
        let old_json = serde_json::to_string(&*config).unwrap_or_default();
        let new_json = serde_json::to_string(&new_config).unwrap_or_default();
        let changed = old_json != new_json;

        if changed {
            *config = new_config;
            info!("✅ Configuration reloaded (changes detected)");
        } else {
            info!("Configuration unchanged");
        }

        Ok(changed)
    }

    /// Export current configuration as JSON
    pub async fn export(&self) -> Result<serde_json::Value, ConfigError> {
        let config = self.config.read().await;
        serde_json::to_value(&*config).map_err(|e| ConfigError::Serialize(e.to_string()))
    }
}

/// Configuration errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("No configuration source path set")]
    NoSource,
    #[error("IO error: {0}")]
    Io(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Serialize error: {0}")]
    Serialize(String),
}
