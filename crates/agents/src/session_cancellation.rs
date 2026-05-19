//! Session Cancellation Registry
//!
//! A global shared registry that allows the Gateway layer to signal
//! cancellation to the Agent layer during long-running operations
//! (e.g. ReAct loops).
//!
//! Usage:
//! - Gateway calls `register(session_id, sender)` before spawning a background
//!   task. It receives a generation token that must be passed to `unregister`.
//! - Gateway calls `set_abort_handle(session_id, generation, abort_handle)`
//!   after spawning the background work task. This allows stop requests to
//!   interrupt in-flight LLM/tool futures instead of waiting for the next
//!   cooperative cancellation check.
//! - Gateway calls `cancel(session_id)` when the user sends a stop command.
//! - Agent calls `get_receiver(session_id)` inside its ReAct loop to check for
//!   cancellation.
//! - Gateway calls `unregister(session_id, generation)` when the background
//!   task completes. Only the task that owns the matching generation can remove
//!   the entry.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use once_cell::sync::Lazy;
use tokio::sync::{watch, RwLock};
use tokio::task::AbortHandle;

struct CancellationEntry {
    sender: watch::Sender<bool>,
    generation: u64,
    abort_handle: Option<AbortHandle>,
    cancelled: bool,
}

static REGISTRY: Lazy<RwLock<HashMap<String, CancellationEntry>>> =
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
    REGISTRY.write().await.insert(
        key.to_string(),
        CancellationEntry {
            sender,
            generation,
            abort_handle: None,
            cancelled: false,
        },
    );
    generation
}

/// Attach the abort handle for the task that owns a registry entry.
///
/// If the user already requested cancellation between `register` and this call,
/// the handle is aborted immediately.
pub async fn set_abort_handle(key: &str, generation: u64, abort_handle: AbortHandle) -> bool {
    let mut registry = REGISTRY.write().await;
    let Some(entry) = registry.get_mut(key) else {
        return false;
    };

    if entry.generation != generation {
        return false;
    }

    if entry.cancelled {
        abort_handle.abort();
    }
    entry.abort_handle = Some(abort_handle);
    true
}

/// Remove a session from the registry, but only if the generation matches.
/// This prevents a slow-to-finish background task from deleting a newer
/// task's cancellation sender.
pub async fn unregister(key: &str, generation: u64) {
    let mut registry = REGISTRY.write().await;
    if let Some(entry) = registry.get(key) {
        if entry.generation == generation {
            registry.remove(key);
        }
    }
}

/// Signal cancellation for a session.
/// Returns `true` if the session was found and signalled.
pub async fn cancel(key: &str) -> bool {
    let mut registry = REGISTRY.write().await;
    let Some(entry) = registry.get_mut(key) else {
        return false;
    };

    entry.cancelled = true;
    let _ = entry.sender.send(true);
    if let Some(handle) = entry.abort_handle.as_ref() {
        handle.abort();
    }
    true
}

/// Get a receiver to watch for cancellation.
pub async fn get_receiver(key: &str) -> Option<watch::Receiver<bool>> {
    REGISTRY.read().await.get(key).map(|entry| entry.sender.subscribe())
}
