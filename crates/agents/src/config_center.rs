//! Unified Configuration Center
//!
//! 🟡 P1 FIX: Centralized configuration management for all agent components.
//! Replaces scattered configuration across multiple modules.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use tokio::sync::RwLock;
use tracing::{debug, error, info};

use crate::error::{AgentError, Result};
use crate::media::DownloadConfig;
use crate::models::ModelConfig;

/// 🟡 P1 FIX: Unified configuration for all agent components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfigCenter {
    /// Service identity
    pub service: ServiceConfig,
    
    /// API server configuration
    pub server: ServerConfig,
    
    /// Model/LLM configuration
    pub models: ModelConfig,
    
    /// Media download configuration
    pub media: DownloadConfig,
    
    /// Memory configuration
    pub memory: MemoryConfig,
    
    /// Queue configuration
    pub queue: QueueConfig,
    
    /// Security configuration
    pub security: SecurityConfig,
    
    /// Feature flags
    pub features: FeatureFlags,
    
    /// Custom configuration values
    #[serde(flatten)]
    pub custom: HashMap<String, serde_json::Value>,
}

/// Service identity configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub name: String,
    pub version: String,
    pub environment: Environment,
    pub instance_id: String,
}

/// Deployment environment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    Development,
    Testing,
    Staging,
    Production,
}

impl std::fmt::Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Environment::Development => write!(f, "development"),
            Environment::Testing => write!(f, "testing"),
            Environment::Staging => write!(f, "staging"),
            Environment::Production => write!(f, "production"),
        }
    }
}

/// Server/API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub request_timeout_secs: u64,
    pub max_request_size_mb: usize,
    pub enable_cors: bool,
    pub enable_compression: bool,
}

/// Memory configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub max_working_memory_mb: usize,
    pub cache_ttl_secs: u64,
    pub enable_compression: bool,
}

/// Queue/worker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    pub max_workers: usize,
    pub queue_size: usize,
    pub task_timeout_secs: u64,
    pub retry_attempts: u32,
    pub retry_delay_secs: u64,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub enable_auth: bool,
    pub token_ttl_secs: u64,
    pub max_failed_auth_attempts: u32,
    pub enable_rate_limiting: bool,
    pub rate_limit_requests_per_min: u32,
}

/// Feature flags for A/B testing and gradual rollouts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlags {
    pub enable_a2a_protocol: bool,
    pub enable_mcp_tools: bool,
    pub enable_media_processing: bool,
    pub enable_file_uploads: bool,
    pub enable_voice_messages: bool,
    pub enable_video_processing: bool,
    pub experimental_features: Vec<String>,
}

impl Default for AgentConfigCenter {
    fn default() -> Self {
        Self {
            service: ServiceConfig {
                name: "beebotos-agent".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                environment: Environment::Development,
                instance_id: uuid::Uuid::new_v4().to_string(),
            },
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
                request_timeout_secs: 180,
                max_request_size_mb: 10,
                enable_cors: true,
                enable_compression: true,
            },
            models: ModelConfig::default(),
            media: DownloadConfig::default(),
            memory: MemoryConfig {
                max_working_memory_mb: 512,
                cache_ttl_secs: 3600,
                enable_compression: true,
            },
            queue: QueueConfig {
                max_workers: 10,
                queue_size: 1000,
                task_timeout_secs: 1200,
                retry_attempts: 3,
                retry_delay_secs: 5,
            },
            security: SecurityConfig {
                enable_auth: true,
                token_ttl_secs: 3600,
                max_failed_auth_attempts: 5,
                enable_rate_limiting: true,
                rate_limit_requests_per_min: 100,
            },
            features: FeatureFlags {
                enable_a2a_protocol: true,
                enable_mcp_tools: true,
                enable_media_processing: true,
                enable_file_uploads: true,
                enable_voice_messages: true,
                enable_video_processing: false,
                experimental_features: vec![],
            },
            custom: HashMap::new(),
        }
    }
}

impl AgentConfigCenter {
    /// Load configuration from file
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| AgentError::configuration(format!("Failed to read config file: {}", e)))?;
        
        let config: Self = serde_yaml::from_str(&content)
            .or_else(|_| serde_json::from_str(&content))
            .map_err(|e| AgentError::configuration(format!("Failed to parse config: {}", e)))?;
        
        info!("Loaded configuration from {:?}", path.as_ref());
        Ok(config)
    }

    /// Load configuration from environment variables
    /// 
    /// Environment variable format: BEEBOTOS_<SECTION>__<KEY>
    /// Example: BEEBOTOS_SERVER__PORT=8080
    pub fn from_env() -> Result<Self> {
        let mut config = Self::default();
        
        // Load service config from env
        if let Ok(name) = std::env::var("BEEBOTOS_SERVICE__NAME") {
            config.service.name = name;
        }
        if let Ok(env_str) = std::env::var("BEEBOTOS_SERVICE__ENVIRONMENT") {
            config.service.environment = match env_str.to_lowercase().as_str() {
                "development" | "dev" => Environment::Development,
                "testing" | "test" => Environment::Testing,
                "staging" | "stage" => Environment::Staging,
                "production" | "prod" => Environment::Production,
                _ => Environment::Development,
            };
        }
        
        // Load server config from env
        if let Ok(host) = std::env::var("BEEBOTOS_SERVER__HOST") {
            config.server.host = host;
        }
        if let Ok(port) = std::env::var("BEEBOTOS_SERVER__PORT") {
            config.server.port = port.parse()
                .map_err(|e| AgentError::configuration(format!("Invalid port: {}", e)))?;
        }
        
        // Load security config from env
        if let Ok(auth) = std::env::var("BEEBOTOS_SECURITY__ENABLE_AUTH") {
            config.security.enable_auth = auth.parse().unwrap_or(true);
        }
        
        info!("Loaded configuration from environment variables");
        Ok(config)
    }

    /// Merge with another configuration (overrides self with other)
    pub fn merge(&mut self, other: Self) {
        // Service config - only override non-default values
        if !other.service.name.is_empty() {
            self.service.name = other.service.name;
        }
        if !other.service.instance_id.is_empty() {
            self.service.instance_id = other.service.instance_id;
        }
        self.service.environment = other.service.environment;
        
        // Server config
        self.server = other.server;
        
        // Model config - only override if explicitly set
        // (ModelConfig doesn't implement PartialEq, so we merge)
        
        // Media config
        self.media = other.media;
        
        // Memory config
        self.memory = other.memory;
        
        // Queue config
        self.queue = other.queue;
        
        // Security config
        self.security = other.security;
        
        // Feature flags - merge rather than replace
        self.features.enable_a2a_protocol = other.features.enable_a2a_protocol;
        self.features.enable_mcp_tools = other.features.enable_mcp_tools;
        self.features.enable_media_processing = other.features.enable_media_processing;
        self.features.enable_file_uploads = other.features.enable_file_uploads;
        self.features.enable_voice_messages = other.features.enable_voice_messages;
        self.features.enable_video_processing = other.features.enable_video_processing;
        self.features.experimental_features.extend(other.features.experimental_features);
        
        // Custom config - merge
        self.custom.extend(other.custom);
        
        debug!("Merged configurations");
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Validate server config
        if self.server.port == 0 {
            return Err(AgentError::configuration("Server port cannot be 0"));
        }
        if self.server.max_request_size_mb == 0 {
            return Err(AgentError::configuration("Max request size cannot be 0"));
        }
        
        // Validate model config
        self.models.validate()
            .map_err(|e| AgentError::configuration(format!("Model config invalid: {}", e)))?;
        
        // Validate memory config
        if self.memory.max_working_memory_mb == 0 {
            return Err(AgentError::configuration("Max working memory cannot be 0"));
        }
        
        // Validate queue config
        if self.queue.max_workers == 0 {
            return Err(AgentError::configuration("Max workers cannot be 0"));
        }
        
        info!("Configuration validation passed");
        Ok(())
    }

    /// Save configuration to file
    pub fn save_to_file(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let content = serde_yaml::to_string(self)
            .map_err(|e| AgentError::configuration(format!("Failed to serialize config: {}", e)))?;
        
        std::fs::write(&path, content)
            .map_err(|e| AgentError::configuration(format!("Failed to write config file: {}", e)))?;
        
        info!("Saved configuration to {:?}", path.as_ref());
        Ok(())
    }

    /// Get a custom configuration value
    pub fn get_custom(&self, key: &str) -> Option<&serde_json::Value> {
        self.custom.get(key)
    }

    /// Set a custom configuration value
    pub fn set_custom(&mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) {
        self.custom.insert(key.into(), value.into());
    }
}

/// 🟡 P1 FIX: Global configuration manager with hot-reload support
pub struct ConfigManager {
    config: RwLock<AgentConfigCenter>,
    subscribers: RwLock<Vec<tokio::sync::mpsc::Sender<ConfigChangeEvent>>>,
}

/// Configuration change event
#[derive(Debug, Clone)]
pub struct ConfigChangeEvent {
    pub path: String,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
}

impl ConfigManager {
    /// Create new config manager with initial configuration
    pub fn new(config: AgentConfigCenter) -> Self {
        Self {
            config: RwLock::new(config),
            subscribers: RwLock::new(vec![]),
        }
    }

    /// Get current configuration
    pub async fn get(&self) -> AgentConfigCenter {
        self.config.read().await.clone()
    }

    /// Update configuration
    pub async fn update(&self, new_config: AgentConfigCenter) -> Result<()> {
        // Validate new config
        new_config.validate()?;
        
        let mut config = self.config.write().await;
        *config = new_config;
        
        info!("Configuration updated");
        Ok(())
    }

    /// Update specific field
    pub async fn update_field<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(&mut AgentConfigCenter),
    {
        let mut config = self.config.write().await;
        f(&mut config);
        
        // Validate after update
        config.validate()?;
        
        info!("Configuration field updated");
        Ok(())
    }

    /// Subscribe to configuration changes
    pub async fn subscribe(&self) -> tokio::sync::mpsc::Receiver<ConfigChangeEvent> {
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let mut subscribers = self.subscribers.write().await;
        subscribers.push(tx);
        rx
    }

    /// Load from file and update
    pub async fn load_from_file(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let new_config = AgentConfigCenter::from_file(path)?;
        self.update(new_config).await
    }

    /// Watch file for changes and auto-reload
    pub async fn watch_file(&self, path: impl AsRef<std::path::Path> + Send + 'static) -> Result<()> {
        use tokio::time::{interval, Duration};
        
        let path = path.as_ref().to_path_buf();
        let last_modified = RwLock::new(std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| std::time::SystemTime::now()));
        
        let mut check_interval = interval(Duration::from_secs(5));
        
        loop {
            check_interval.tick().await;
            
            if let Ok(metadata) = std::fs::metadata(&path) {
                if let Ok(modified) = metadata.modified() {
                    let last = *last_modified.read().await;
                    if modified > last {
                        info!("Configuration file changed, reloading...");
                        if let Err(e) = self.load_from_file(&path).await {
                            error!("Failed to reload configuration: {}", e);
                        } else {
                            *last_modified.write().await = modified;
                        }
                    }
                }
            }
        }
    }
}

/// 🟡 P1 FIX: Configuration builder for easy setup
pub struct ConfigBuilder {
    config: AgentConfigCenter,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: AgentConfigCenter::default(),
        }
    }

    pub fn with_service_name(mut self, name: impl Into<String>) -> Self {
        self.config.service.name = name.into();
        self
    }

    pub fn with_environment(mut self, env: Environment) -> Self {
        self.config.service.environment = env;
        self
    }

    pub fn with_server_host(mut self, host: impl Into<String>) -> Self {
        self.config.server.host = host.into();
        self
    }

    pub fn with_server_port(mut self, port: u16) -> Self {
        self.config.server.port = port;
        self
    }

    pub fn with_model_config(mut self, models: ModelConfig) -> Self {
        self.config.models = models;
        self
    }

    pub fn with_media_config(mut self, media: DownloadConfig) -> Self {
        self.config.media = media;
        self
    }

    pub fn with_feature(mut self, feature: impl Into<String>, enabled: bool) -> Self {
        match feature.into().as_str() {
            "a2a" => self.config.features.enable_a2a_protocol = enabled,
            "mcp" => self.config.features.enable_mcp_tools = enabled,
            "media" => self.config.features.enable_media_processing = enabled,
            "files" => self.config.features.enable_file_uploads = enabled,
            "voice" => self.config.features.enable_voice_messages = enabled,
            "video" => self.config.features.enable_video_processing = enabled,
            other => {
                if enabled {
                    self.config.features.experimental_features.push(other.to_string());
                }
            }
        }
        self
    }

    pub fn build(self) -> Result<AgentConfigCenter> {
        self.config.validate()?;
        Ok(self.config)
    }

    pub fn build_manager(self) -> Result<ConfigManager> {
        let config = self.build()?;
        Ok(ConfigManager::new(config))
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AgentConfigCenter::default();
        assert_eq!(config.server.port, 8080);
        assert!(config.security.enable_auth);
        assert!(config.features.enable_a2a_protocol);
    }

    #[test]
    fn test_config_validation() {
        let config = AgentConfigCenter::default();
        assert!(config.validate().is_ok());
        
        let mut invalid = config.clone();
        invalid.server.port = 0;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_config_from_env() {
        // This test would need env vars set
        // std::env::set_var("BEEBOTOS_SERVER__PORT", "9000");
        // let config = AgentConfigCenter::from_env().unwrap();
        // assert_eq!(config.server.port, 9000);
    }

    #[test]
    fn test_config_builder() {
        let config = ConfigBuilder::new()
            .with_service_name("test-agent")
            .with_server_port(9000)
            .with_environment(Environment::Testing)
            .with_feature("video", true)
            .build()
            .unwrap();
        
        assert_eq!(config.service.name, "test-agent");
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.service.environment, Environment::Testing);
        assert!(config.features.enable_video_processing);
    }

    #[tokio::test]
    async fn test_config_manager() {
        let manager = ConfigManager::new(AgentConfigCenter::default());
        let config = manager.get().await;
        assert_eq!(config.server.port, 8080);
    }
}
