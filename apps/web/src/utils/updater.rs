//! Web application updater utility
//!
//! Checks for new versions on page load and notifies users.

use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::view;
use wasm_bindgen::JsCast;
use web_sys::window;

use beebotos_update_client::models::{UpdateCheckRequest, UpdateCheckResponse, VersionInfo};

/// Web application update manager
pub struct WebUpdater {
    current_version: String,
    server_url: String,
    registration: Option<web_sys::ServiceWorkerRegistration>,
}

impl WebUpdater {
    pub async fn new(server_url: String) -> Result<Self, String> {
        let window = window().ok_or("No window available")?;
        let navigator = window.navigator();

        let registration = {
            let sw = navigator.service_worker();
            match sw.ready() {
                Ok(promise) => {
                    wasm_bindgen_futures::JsFuture::from(promise)
                        .await
                        .ok()
                        .and_then(|r| r.dyn_into::<web_sys::ServiceWorkerRegistration>().ok())
                }
                Err(_) => None,
            }
        };

        Ok(Self {
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            server_url,
            registration,
        })
    }

    /// Check for available updates
    pub async fn check_update(&self) -> Result<Option<VersionInfo>, String> {
        let req = UpdateCheckRequest {
            app_name: "web".to_string(),
            current_version: self.current_version.clone(),
            platform: "wasm".to_string(),
            channel: "stable".to_string(),
        };

        let url = format!("{}/api/v1/updates/check", self.server_url);
        let resp = Request::post(&url)
            .json(&req)
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.ok() {
            return Err(format!("HTTP {}", resp.status()));
        }

        let check_resp: UpdateCheckResponse = resp.json().await.map_err(|e| e.to_string())?;
        Ok(check_resp.version_info)
    }

    /// Display update notification UI
    pub fn prompt_update(&self, info: &VersionInfo) -> impl IntoView {
        let (show, set_show) = signal(true);
        let version = info.version.to_string();
        let notes = info
            .release_notes
            .get("zh")
            .cloned()
            .or_else(|| info.release_notes.get("en").cloned())
            .unwrap_or_default();

        view! {
            {move || show.get().then(|| view! {
                <div class="update-notification" style="position:fixed;bottom:20px;right:20px;background:#1a1a2e;color:#fff;padding:16px 20px;border-radius:12px;box-shadow:0 8px 32px rgba(0,0,0,0.3);z-index:9999;max-width:360px;font-family:sans-serif;">
                    <div style="font-weight:600;font-size:16px;margin-bottom:8px;">
                        "New version available: " {version.clone()}
                    </div>
                    <div style="font-size:14px;color:#aaa;margin-bottom:12px;line-height:1.5;">
                        {notes.clone()}
                    </div>
                    <div style="display:flex;gap:10px;">
                        <button
                            on:click=move |_| {
                                set_show.set(false);
                                if let Some(window) = window() {
                                    let _ = window.location().reload();
                                }
                            }
                            style="flex:1;padding:8px 16px;background:#4f46e5;color:#fff;border:none;border-radius:6px;cursor:pointer;font-weight:500;"
                        >
                            "Update Now"
                        </button>
                        <button
                            on:click=move |_| set_show.set(false)
                            style="flex:1;padding:8px 16px;background:#333;color:#fff;border:none;border-radius:6px;cursor:pointer;font-weight:500;"
                        >
                            "Later"
                        </button>
                    </div>
                </div>
            })}
        }
    }

    /// Pre-cache new version via Service Worker
    pub async fn precache_update(&self, info: &VersionInfo) -> Result<(), String> {
        if let Some(reg) = &self.registration {
            if let Some(sw) = reg.active() {
                let msg = js_sys::JSON::stringify(&wasm_bindgen::JsValue::from_str(
                    &format!("{{\"action\":\"precache\",\"version\":\"{}\"}}", info.version)
                )).unwrap_or_default();
                let _ = sw.post_message(&msg);
            }
        }
        Ok(())
    }
}

/// Initialize updater on application startup
pub async fn init_updater(server_url: String) {
    let updater = match WebUpdater::new(server_url).await {
        Ok(u) => u,
        Err(e) => {
            let msg = format!("Updater init failed: {}", e);
            web_sys::console::warn_1(&msg.into());
            return;
        }
    };

    match updater.check_update().await {
        Ok(Some(info)) => {
            if info.mandatory {
                // Force update: reload immediately
                if let Some(window) = window() {
                    let _ = window.location().reload();
                }
            } else {
                // Optional update: pre-cache in background and let UI component handle display
                let _ = updater.precache_update(&info).await;
                web_sys::console::log_1(&format!("Optional update available: {}", info.version).into());
            }
        }
        Ok(None) => {}
        Err(e) => {
            let msg = format!("Update check failed: {}", e);
            web_sys::console::warn_1(&msg.into());
        }
    }
}

/// Leptos component for update notification
#[component]
pub fn UpdateNotification(server_url: String) -> impl IntoView {
    let (info, set_info) = signal(None::<VersionInfo>);
    let server_url_for_effect = server_url.clone();

    Effect::new(move |_| {
        let url = server_url_for_effect.clone();
        spawn_local(async move {
            let updater = match WebUpdater::new(url).await {
                Ok(u) => u,
                Err(_) => return,
            };
            if let Ok(Some(i)) = updater.check_update().await {
                set_info.set(Some(i));
            }
        });
    });

    move || {
        info.get().map(|version_info| {
            let updater = WebUpdater {
                current_version: env!("CARGO_PKG_VERSION").to_string(),
                server_url: server_url.clone(),
                registration: None,
            };
            updater.prompt_update(&version_info).into_view()
        })
    }
}
