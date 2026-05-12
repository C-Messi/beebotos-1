//! Session Cancellation Registry
//!
//! A global shared registry that allows the Gateway layer to signal
//! cancellation to the Agent layer during long-running operations
//! (e.g. ReAct loops).
//!
//! Usage:
//! - Gateway calls `register(session_id, sender)` before spawning a background task.
//!   It receives a generation token that must be passed to `unregister`.
//! - Gateway calls `cancel(session_id)` when the user sends a stop command.
//! - Agent calls `get_receiver(session_id)` inside its ReAct loop to check for cancellation.
//! - Gateway calls `unregister(session_id, generation)` when the background task completes.
//!   Only the task that owns the matching generation can remove the entry.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use once_cell::sync::Lazy;
use tokio::sync::{watch, RwLock};

static REGISTRY: Lazy<RwLock<HashMap<String, (watch::Sender<bool>, u64)>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Register a cancellation sender for a session.
///
/// Returns a generation token. The caller must pass this token to
/// `unregister` to ensure only the owner can remove the entry.
/// If a previous sender exists for the same key, it is replaced
/// (the old task will no longer receive cancellation signals).
pub async fn register(key: &str, sender: watch::Sender<bool>) -> u64 {
    let generation = NEXT_GENERATION.fetch_add(1, Ordering::SeqCst);
    REGISTRY.write().await.insert(key.to_string(), (sender, generation));
    generation
}

/// Remove a session from the registry, but only if the generation matches.
/// This prevents a slow-to-finish background task from deleting a newer
/// task's cancellation sender.
pub async fn unregister(key: &str, generation: u64) {
    let mut registry = REGISTRY.write().await;
    if let Some((_, gen)) = registry.get(key) {
        if *gen == generation {
            registry.remove(key);
        }
    }
}

/// Signal cancellation for a session.
/// Returns `true` if the session was found and signalled.
pub async fn cancel(key: &str) -> bool {
    if let Some((sender, _)) = REGISTRY.read().await.get(key) {
        let _ = sender.send(true);
        true
    } else {
        false
    }
}

/// Get a receiver to watch for cancellation.
pub async fn get_receiver(key: &str) -> Option<watch::Receiver<bool>> {
    REGISTRY.read().await.get(key).map(|(s, _)| s.subscribe())
}
