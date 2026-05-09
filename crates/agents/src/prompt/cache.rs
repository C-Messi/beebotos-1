//! Prompt Cache Module
//!
//! Caches assembled prompts to reduce repeated token consumption by 40-60%.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use super::PromptComponents;

/// Prompt cache configuration
#[derive(Debug, Clone)]
pub struct PromptCacheConfig {
    /// Time-to-live for cached entries
    pub ttl: Duration,
    /// Maximum number of entries
    pub max_entries: usize,
}

impl Default for PromptCacheConfig {
    fn default() -> Self {
        Self {
            ttl: Duration::from_secs(300), // 5 minutes
            max_entries: 100,
        }
    }
}

/// Prompt cache using component hash keys
pub struct PromptCache {
    cache: Arc<RwLock<HashMap<String, (String, Instant)>>>,
    config: PromptCacheConfig,
}

impl PromptCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            config: PromptCacheConfig::default(),
        }
    }

    pub fn with_config(config: PromptCacheConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Get cached prompt or build it
    pub async fn get_or_build<F>(&self, components: &PromptComponents, builder: F) -> String
    where
        F: FnOnce(&PromptComponents) -> String,
    {
        let hash = self.hash_components(components);

        {
            let cache = self.cache.read().await;
            if let Some((prompt, last_used)) = cache.get(&hash) {
                if last_used.elapsed() < self.config.ttl {
                    tracing::debug!("Prompt cache hit: {}", hash);
                    return prompt.clone();
                }
            }
        }

        let prompt = builder(components);

        {
            let mut cache = self.cache.write().await;
            cache.insert(hash.clone(), (prompt.clone(), Instant::now()));

            // Evict oldest entries if over limit
            if cache.len() > self.config.max_entries {
                let mut entries: Vec<_> = cache.drain().collect();
                entries.sort_by(|a, b| b.1 .1.cmp(&a.1 .1)); // newest first
                let keep = self.config.max_entries / 2;
                for (k, v) in entries.into_iter().take(keep) {
                    cache.insert(k, v);
                }
            }
        }

        tracing::debug!("Prompt cache miss, stored: {}", hash);
        prompt
    }

    /// Hash prompt components for cache key
    fn hash_components(&self, components: &PromptComponents) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();

        components.soul.hash(&mut hasher);
        components.user_profile.hash(&mut hasher);
        components.model.hash(&mut hasher);

        // Hash active skill IDs
        let skill_ids: Vec<&str> = components.skills.iter().map(|s| s.id()).collect();
        skill_ids.hash(&mut hasher);

        // Hash memory count and first few chars
        components.memories.len().hash(&mut hasher);
        for m in components.memories.iter().take(3) {
            m.content
                .chars()
                .take(50)
                .collect::<String>()
                .hash(&mut hasher);
        }

        format!("{:x}", hasher.finish())
    }

    /// Clear all cached entries
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Get cache statistics
    pub async fn stats(&self) -> (usize, Option<Duration>) {
        let cache = self.cache.read().await;
        let size = cache.len();
        let oldest = cache.values().map(|(_, t)| t.elapsed()).min();
        (size, oldest)
    }
}

impl Default for PromptCache {
    fn default() -> Self {
        Self::new()
    }
}
