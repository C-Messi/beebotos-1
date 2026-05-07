//! Web updater for WASM frontend

use gloo_net::http::Request;
use js_sys::JSON;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{window, ServiceWorkerRegistration};

use crate::{
    error::UpdateError,
    models::{UpdateCheckRequest, UpdateCheckResponse, VersionInfo},
};

/// Web application updater
pub struct WebUpdater {
    current_version: String,
    server_url: String,
    registration: Option<ServiceWorkerRegistration>,
}

impl WebUpdater {
    pub async fn new(server_url: String) -> Result<Self, UpdateError> {
        let window = window().ok_or_else(|| UpdateError::Network("No window available".to_string()))?;
        let navigator = window.navigator();

        let registration = if let Ok(sw) = navigator.service_worker() {
            JsFuture::from(sw.ready())
                .await
                .ok()
                .and_then(|r| r.dyn_into::<ServiceWorkerRegistration>().ok())
        } else {
            None
        };

        Ok(Self {
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            server_url,
            registration,
        })
    }

    /// Check for available updates
    pub async fn check_update(&self) -> Result<Option<VersionInfo>, UpdateError> {
        let req = UpdateCheckRequest {
            app_name: "web".to_string(),
            current_version: self.current_version.clone(),
            platform: "wasm".to_string(),
            channel: "stable".to_string(),
        };

        let url = format!("{}/api/v1/updates/check", self.server_url);
        let resp = Request::post(&url)
            .json(&req)
            .map_err(|e| UpdateError::Serialization(e.to_string()))?
            .send()
            .await
            .map_err(|e| UpdateError::Network(e.to_string()))?;

        if !resp.ok() {
            return Err(UpdateError::Network(format!("HTTP {}", resp.status())));
        }

        let check_resp: UpdateCheckResponse = resp
            .json()
            .await
            .map_err(|e| UpdateError::Serialization(e.to_string()))?;

        Ok(check_resp.version_info)
    }

    /// Check if an update is available (simple version comparison)
    pub async fn has_update(&self) -> Result<bool, UpdateError> {
        match self.check_update().await? {
            Some(info) => {
                let latest = info.version.to_string();
                Ok(latest != self.current_version)
            }
            None => Ok(false),
        }
    }

    /// Prompt user to update (reload page)
    pub fn prompt_update(&self, _info: &VersionInfo) {
        if let Some(window) = window() {
            let _ = window.location().reload();
        }
    }

    /// Pre-cache update via Service Worker
    pub async fn precache_update(&self, info: &VersionInfo) -> Result<(), UpdateError> {
        if let Some(reg) = &self.registration {
            if let Some(sw) = reg.active() {
                let msg = JSON::stringify(&JsValue::from_str(
                    &format!("{{\"action\":\"precache\",\"version\":\"{}\"}}", info.version)
                )).unwrap_or_default();
                let _ = sw.post_message(&msg);
            }
        }
        Ok(())
    }

    /// Initialize updater on page load
    pub async fn init(&self) -> Result<(), UpdateError> {
        match self.check_update().await? {
            Some(info) => {
                if info.mandatory {
                    // Force reload for mandatory updates
                    self.prompt_update(&info);
                }
            }
            None => {}
        }
        Ok(())
    }
}
