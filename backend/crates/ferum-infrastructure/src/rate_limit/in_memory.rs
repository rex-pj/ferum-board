
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dashmap::DashMap;

use ferum_application::ports::{RateLimitResult, RateLimiter};
use ferum_application::shared::AppError;

struct Window {
    count: u32,
    reset_at: Instant,
}

pub struct InMemoryRateLimiter {
    store: Arc<DashMap<String, Window>>,
}

impl Default for InMemoryRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryRateLimiter {
    pub fn new() -> Self {
        let store: Arc<DashMap<String, Window>> = Arc::new(DashMap::new());

        // Evict expired windows every 5 minutes. Without this the map grows
        // unboundedly because entries are only reset (not removed) on re-use,
        // so IPs that never return accumulate forever.
        let store_weak = Arc::downgrade(&store);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let Some(map) = store_weak.upgrade() else { break };
                let now = Instant::now();
                map.retain(|_, w| w.reset_at > now);
            }
        });

        Self { store }
    }
}

#[async_trait]
impl RateLimiter for InMemoryRateLimiter {
    async fn check(
        &self,
        key: &str,
        limit: u32,
        window: Duration,
    ) -> Result<RateLimitResult, AppError> {
        let now = Instant::now();
        let reset_at = now + window;

        let mut entry = self
            .store
            .entry(key.to_string())
            .or_insert(Window { count: 0, reset_at });

        if now > entry.reset_at {
            entry.count = 0;
            entry.reset_at = reset_at;
        }

        entry.count += 1;

        if entry.count > limit {
            let retry_after = entry.reset_at.saturating_duration_since(now);
            Ok(RateLimitResult::Denied { retry_after })
        } else {
            let remaining = limit.saturating_sub(entry.count);
            Ok(RateLimitResult::Allowed { remaining })
        }
    }
}

/// Allows everything. Selected when `RATE_LIMIT_ENABLED=false`, which is the
/// dev profile — never a production one.
pub struct NullRateLimiter;

#[async_trait]
impl RateLimiter for NullRateLimiter {
    async fn check(
        &self,
        _key: &str,
        limit: u32,
        _window: Duration,
    ) -> Result<RateLimitResult, AppError> {
        Ok(RateLimitResult::Allowed { remaining: limit })
    }
}
