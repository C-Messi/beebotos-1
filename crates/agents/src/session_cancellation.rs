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
//! - Agent calls `get_receiver_for_generation(session_id, generation)` inside
//!   its ReAct loop to check for cancellation.
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
    abort_handles: Vec<AbortHandle>,
    cancelled: bool,
}

static REGISTRY: Lazy<RwLock<HashMap<String, HashMap<u64, CancellationEntry>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Register a cancellation sender for a session.
///
/// Returns a generation token. The caller must pass this token to
/// `unregister` to ensure only the owner can remove the entry.
/// Multiple generations may coexist for the same session so a stale
/// long-running task remains cancellable after a newer user message starts.
pub async fn register(key: &str, sender: watch::Sender<bool>) -> u64 {
    let generation = NEXT_GENERATION.fetch_add(1, Ordering::SeqCst);
    REGISTRY
        .write()
        .await
        .entry(key.to_string())
        .or_default()
        .insert(
            generation,
            CancellationEntry {
                sender,
                generation,
                abort_handles: Vec::new(),
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
    let Some(entries) = registry.get_mut(key) else {
        return false;
    };
    let Some(entry) = entries.get_mut(&generation) else {
        return false;
    };

    if entry.generation != generation {
        return false;
    }

    if entry.cancelled {
        abort_handle.abort();
    }
    entry.abort_handles.push(abort_handle);
    true
}

/// Remove a session from the registry, but only if the generation matches.
/// This prevents a slow-to-finish background task from deleting a newer
/// task's cancellation sender.
pub async fn unregister(key: &str, generation: u64) {
    let mut registry = REGISTRY.write().await;
    if let Some(entries) = registry.get_mut(key) {
        if matches!(entries.get(&generation), Some(entry) if entry.generation == generation) {
            entries.remove(&generation);
        }
        if entries.is_empty() {
            registry.remove(key);
        }
    }
}

/// Signal cancellation for a session.
/// Returns `true` if at least one active generation was found and signalled.
pub async fn cancel(key: &str) -> bool {
    let mut registry = REGISTRY.write().await;
    let Some(entries) = registry.get_mut(key) else {
        return false;
    };

    for entry in entries.values_mut() {
        entry.cancelled = true;
        let _ = entry.sender.send(true);
        for handle in &entry.abort_handles {
            handle.abort();
        }
    }
    true
}

/// Get a receiver to watch for the newest generation's cancellation.
///
/// Prefer `get_receiver_for_generation` when a task owns a generation token.
pub async fn get_receiver(key: &str) -> Option<watch::Receiver<bool>> {
    let registry = REGISTRY.read().await;
    let entries = registry.get(key)?;
    entries
        .values()
        .max_by_key(|entry| entry.generation)
        .map(|entry| entry.sender.subscribe())
}

/// Get a receiver to watch cancellation for one specific task generation.
pub async fn get_receiver_for_generation(
    key: &str,
    generation: u64,
) -> Option<watch::Receiver<bool>> {
    REGISTRY
        .read()
        .await
        .get(key)
        .and_then(|entries| entries.get(&generation))
        .map(|entry| entry.sender.subscribe())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    fn unique_key(name: &str) -> String {
        format!(
            "test-{}-{}",
            name,
            NEXT_GENERATION.fetch_add(1, Ordering::SeqCst)
        )
    }

    #[tokio::test]
    async fn cancel_signals_all_generations_for_session() {
        let key = unique_key("cancel-all");
        let (tx1, _rx1) = watch::channel(false);
        let (tx2, _rx2) = watch::channel(false);

        let gen1 = register(&key, tx1).await;
        let gen2 = register(&key, tx2).await;

        let rx1 = get_receiver_for_generation(&key, gen1)
            .await
            .expect("first generation receiver");
        let rx2 = get_receiver_for_generation(&key, gen2)
            .await
            .expect("second generation receiver");

        assert!(cancel(&key).await);
        assert!(*rx1.borrow());
        assert!(*rx2.borrow());

        unregister(&key, gen1).await;
        unregister(&key, gen2).await;
    }

    #[tokio::test]
    async fn unregister_only_removes_matching_generation() {
        let key = unique_key("unregister-one");
        let (tx1, _rx1) = watch::channel(false);
        let (tx2, _rx2) = watch::channel(false);

        let gen1 = register(&key, tx1).await;
        let gen2 = register(&key, tx2).await;

        unregister(&key, gen1).await;

        assert!(get_receiver_for_generation(&key, gen1).await.is_none());
        assert!(get_receiver_for_generation(&key, gen2).await.is_some());

        unregister(&key, gen2).await;
    }
}
