use lru::LruCache;
use parking_lot::RwLock;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::LazyLock;

use crate::prompts::system::SystemPromptReport;

/// Maximum cache size per shard. With N shards the total capacity is
/// N * MAX_SHARD_SIZE, but entries are distributed by key so the effective
/// capacity is approximately MAX_SHARD_SIZE per project.
const MAX_SHARD_SIZE: usize = 32;
const NUM_SHARDS: usize = 16;
const SHARD_MASK: usize = NUM_SHARDS - 1;

/// Task categories for prompt generation
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TaskType {
    System,
    Lightweight,
    Specialized,
}

/// Providers can expose a cache key describing the prompt variant.
pub trait PromptProvider {
    fn cache_key(&self) -> String;
    fn task_type(&self) -> TaskType;
}

/// Sharded in-memory prompt cache.
///
/// Splits the cache across N `RwLock<LruCache>` shards so that reads on
/// different shards proceed concurrently without contending on a single
/// global mutex. Shard selection is by Fx hash of the key.
///
/// The fast path uses `LruCache::peek()` under a read-lock — no LRU
/// promotion on reads, but fully concurrent. The slow path (insert) uses
/// a write-lock on the affected shard only.
pub struct SystemPromptCache<V: Clone> {
    shards: [RwLock<LruCache<String, V>>; NUM_SHARDS],
}

impl<V: Clone> Default for SystemPromptCache<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Clone> SystemPromptCache<V> {
    pub fn new() -> Self {
        let shard_size = NonZeroUsize::new(MAX_SHARD_SIZE).unwrap_or(NonZeroUsize::MIN);
        let shard = || RwLock::new(LruCache::new(shard_size));
        Self { shards: [(); NUM_SHARDS].map(|_| shard()) }
    }

    #[inline]
    fn shard_index(key: &str) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) & SHARD_MASK
    }

    /// Get cached prompt or build it if missing.
    /// Fast path: read-lock + peek (concurrent, no LRU promotion).
    /// Slow path: write-lock + double-check + insert.
    pub fn get_or_insert_with<F>(&self, key: &str, builder: F) -> V
    where
        F: FnOnce() -> V,
    {
        let idx = Self::shard_index(key);

        // Fast path: read-lock allows N concurrent readers on different shards
        {
            let shard = self.shards[idx].read();
            if let Some(value) = shard.peek(key) {
                return value.clone();
            }
        }

        // Slow path: build + insert under write-lock
        let value = builder();
        let mut shard = self.shards[idx].write();
        // Double-check: another caller may have inserted while we were building
        if let Some(value) = shard.get(key) {
            return value.clone();
        }
        shard.put(key.to_string(), value.clone());
        value
    }

    /// Get cached value, returning None on miss.
    pub fn get(&self, key: &str) -> Option<V> {
        let shard = self.shards[Self::shard_index(key)].read();
        shard.peek(key).cloned()
    }

    /// Insert a value into the cache.
    pub fn insert(&self, key: String, value: V) {
        let idx = Self::shard_index(&key);
        let mut shard = self.shards[idx].write();
        shard.put(key, value);
    }

    /// Clear all shards.
    pub fn clear(&self) {
        for shard in &self.shards {
            shard.write().clear();
        }
    }
}

/// Global prompt cache shared across runs. Caches the composed prompt string
/// together with its [`SystemPromptReport`] so cache hits surface the same
/// token-budget report a cache miss would have computed.
///
/// Sharded into 16 `RwLock<LruCache>` shards to eliminate mutex contention
/// in multi-project workflows.
pub static PROMPT_CACHE: LazyLock<SystemPromptCache<(String, SystemPromptReport)>> =
    LazyLock::new(SystemPromptCache::new);
