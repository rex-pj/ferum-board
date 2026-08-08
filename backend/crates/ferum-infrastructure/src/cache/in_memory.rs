use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dashmap::DashMap;

use ferum_application::ports::CacheService;
use ferum_application::shared::AppError;

struct CacheEntry {
    value: String,
    expires_at: Instant,
}

pub struct InMemoryCacheService {
    store: Arc<DashMap<String, CacheEntry>>,
}

impl Default for InMemoryCacheService {
    fn default() -> Self {
        Self::new()
    }
}

/// How often expired entries are swept. Matches `InMemoryRateLimiter`'s cadence;
/// the cost is one `retain` pass over the map, and entries are still evicted
/// lazily on read in between, so this only has to catch what is never read again.
const EVICT_INTERVAL: Duration = Duration::from_secs(300);

impl InMemoryCacheService {
    pub fn new() -> Self {
        let store: Arc<DashMap<String, CacheEntry>> = Arc::new(DashMap::new());

        // Sweep expired entries periodically.
        //
        // Without this the map only ever shrinks when the *same key* is read
        // again after expiring — and most keys here are never read again. They
        // are per-user and per-session: `user:roles:{uuid}`, `user:banned:{uuid}`,
        // the session-epoch key, cached preferences. Every visitor who ever
        // authenticates leaves entries behind that nothing revisits once their
        // TTL passes, so the map grew for the life of the process.
        //
        // This is the fallback cache — it is what runs whenever `REDIS_URL` is
        // unset, which is the documented dev profile and every single-instance
        // install. Redis expires its own keys, so this concerns only the
        // in-process path.
        //
        // `Weak` + `break`: the task must not keep the map alive by holding a
        // strong `Arc`, or the eviction loop becomes its own leak. Same shape as
        // `InMemoryRateLimiter::new`, which already did this correctly.
        let store_weak = Arc::downgrade(&store);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(EVICT_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let Some(map) = store_weak.upgrade() else { break };
                let removed = Self::sweep(&map);
                if removed > 0 {
                    tracing::debug!(removed, remaining = map.len(), "in-memory cache swept");
                }
            }
        });

        Self { store }
    }

    /// Drops every expired entry, returning how many were removed.
    ///
    /// Split out from the interval task so it can be driven directly: the task
    /// itself fires every five minutes, which no test is going to wait for, and
    /// the eviction *rule* is the part worth pinning.
    fn sweep(store: &DashMap<String, CacheEntry>) -> usize {
        let before = store.len();
        store.retain(|_, entry| !Self::is_expired(entry));
        before - store.len()
    }

    /// Drops every expired entry, returning how many were removed.
    ///
    /// `pub` for the test that asserts entries which are never read again are
    /// still reclaimed — the exact case the periodic sweep exists for, and the
    /// one that lazy eviction on read cannot cover.
    pub fn evict_expired(&self) -> usize {
        Self::sweep(&self.store)
    }

    fn is_expired(entry: &CacheEntry) -> bool {
        Instant::now() > entry.expires_at
    }
}

#[async_trait]
impl CacheService for InMemoryCacheService {
    async fn get(&self, key: &str) -> Option<String> {
        let entry = self.store.get(key)?;
        if Self::is_expired(&entry) {
            drop(entry);
            self.store.remove(key);
            None
        } else {
            Some(entry.value.clone())
        }
    }

    async fn set<'a>(&self, key: &'a str, value: &'a str, ttl: Duration) -> Result<(), AppError> {
        self.store.insert(
            key.to_string(),
            CacheEntry {
                value: value.to_string(),
                expires_at: Instant::now() + ttl,
            },
        );
        Ok(())
    }

    async fn set_nx<'a>(&self, key: &'a str, value: &'a str, ttl: Duration) -> Result<bool, AppError> {
        use dashmap::mapref::entry::Entry;
        let now = Instant::now();
        match self.store.entry(key.to_string()) {
            Entry::Vacant(e) => {
                e.insert(CacheEntry {
                    value: value.to_string(),
                    expires_at: now + ttl,
                });
                Ok(true)
            }
            Entry::Occupied(mut e) => {
                if Self::is_expired(e.get()) {
                    e.insert(CacheEntry {
                        value: value.to_string(),
                        expires_at: now + ttl,
                    });
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
        }
    }

    async fn del(&self, key: &str) -> Result<(), AppError> {
        self.store.remove(key);
        Ok(())
    }

    async fn del_prefix(&self, prefix: &str) -> Result<(), AppError> {
        self.store.retain(|k, _| !k.starts_with(prefix));
        Ok(())
    }

    async fn incr_with_ttl(&self, key: &str, ttl: Duration) -> Result<u64, AppError> {
        let expires_at = Instant::now() + ttl;
        let mut new_val = 1u64;
        self.store
            .entry(key.to_string())
            .and_modify(|e| {
                if Self::is_expired(e) {
                    e.value = "1".to_string();
                    e.expires_at = expires_at;
                } else {
                    let v: u64 = e.value.parse().unwrap_or(0);
                    new_val = v + 1;
                    e.value = new_val.to_string();
                }
            })
            .or_insert_with(|| CacheEntry {
                value: "1".to_string(),
                expires_at,
            });
        Ok(new_val)
    }

    async fn exists(&self, key: &str) -> bool {
        self.get(key).await.is_some()
    }
}
