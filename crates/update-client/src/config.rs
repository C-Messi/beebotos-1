//! Update client configuration

/// Update configuration
#[derive(Debug, Clone)]
pub struct UpdateConfig {
    pub server_url: String,
    pub channel: String,
    pub app_name: String,
    pub current_version: String,
    pub platform: String,
    pub device_id: String,
    pub public_key_b64: Option<String>,
    pub auto_download: bool,
    pub auto_install: bool,
    pub check_cron: Option<String>,
    pub allow_downgrade: bool,
    pub min_supported_version: Option<String>,
    pub http_proxy: Option<String>,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            server_url: "https://beeweb.beeagentos.ai".to_string(),
            channel: "stable".to_string(),
            app_name: "unknown".to_string(),
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            platform: default_platform(),
            device_id: uuid::Uuid::new_v4().to_string(),
            public_key_b64: None,
            auto_download: true,
            auto_install: false,
            check_cron: Some("0 0 3 * * *".to_string()),
            allow_downgrade: false,
            min_supported_version: None,
            http_proxy: None,
        }
    }
}

impl UpdateConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(url) = std::env::var("BEEBOTOS_UPDATE_SERVER") {
            config.server_url = url;
        }
        if let Ok(channel) = std::env::var("BEEBOTOS_UPDATE_CHANNEL") {
            config.channel = channel;
        }
        if std::env::var("BEEBOTOS_UPDATE_DISABLED").is_ok() {
            // Would be handled by caller
        }
        if let Ok(key) = std::env::var("BEEBOTOS_UPDATE_PUBLIC_KEY") {
            config.public_key_b64 = Some(key);
        }
        if let Ok(proxy) = std::env::var("HTTP_PROXY") {
            config.http_proxy = Some(proxy);
        }
        config
    }

    pub fn with_app_name(mut self, name: &str) -> Self {
        self.app_name = name.to_string();
        self
    }

    pub fn with_server_url(mut self, url: &str) -> Self {
        self.server_url = url.to_string();
        self
    }
}

/// Detect default platform
pub fn default_platform() -> String {
    #[cfg(target_os = "windows")]
    return "windows".to_string();
    #[cfg(target_os = "macos")]
    return "macos".to_string();
    #[cfg(target_os = "linux")]
    return "linux".to_string();
    #[cfg(target_arch = "wasm32")]
    return "wasm".to_string();
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux",
        target_arch = "wasm32"
    )))]
    return "unknown".to_string();
}
