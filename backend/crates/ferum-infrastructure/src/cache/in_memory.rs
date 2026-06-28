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

impl InMemoryCacheService {
    pub fn new() -> Self {
        Self {
            store: Arc::new(DashMap::new()),
        }
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
