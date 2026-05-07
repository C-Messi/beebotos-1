//! BeeBotOS Update Client
//!
//! Core library for remote software updates across Gateway, CLI, and Web applications.

pub mod config;
pub mod error;
pub mod models;
pub mod utils;

#[cfg(not(target_arch = "wasm32"))]
pub mod client;
#[cfg(not(target_arch = "wasm32"))]
pub mod verify;

#[cfg(target_arch = "wasm32")]
pub mod web_client;

// Re-exports
pub use config::{UpdateConfig, default_platform};
pub use error::UpdateError;
pub use models::*;
pub use utils::{matches_platform, select_package};

#[cfg(not(target_arch = "wasm32"))]
pub use client::{ConsoleProgress, DownloadProgress, NativeUpdateClient, UpdateClient};

#[cfg(target_arch = "wasm32")]
pub use web_client::WebUpdater;
