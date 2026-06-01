#![allow(dead_code)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dashmap::DashMap;

use crate::application::ports::{RateLimitResult, RateLimiter};
use crate::application::shared::AppError;

struct Window {
    count: u32,
    reset_at: Instant,
}

pub struct InMemoryRateLimiter {
    store: Arc<DashMap<String, Window>>,
}

impl InMemoryRateLimiter {
    pub fn new() -> Self {
        Self { store: Arc::new(DashMap::new()) }
    }
}

#[async_trait]
impl RateLimiter for InMemoryRateLimiter {
    async fn check(&self, key: &str, limit: u32, window: Duration) -> Result<RateLimitResult, AppError> {
        let now = Instant::now();
        let reset_at = now + window;

        let mut entry = self.store.entry(key.to_string()).or_insert(Window {
            count: 0,
            reset_at,
        });

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

// ─── NullRateLimiter — disabled for dev ───────────────────────────────────────

pub struct NullRateLimiter;

#[async_trait]
impl RateLimiter for NullRateLimiter {
    async fn check(&self, _key: &str, limit: u32, _window: Duration) -> Result<RateLimitResult, AppError> {
        Ok(RateLimitResult::Allowed { remaining: limit })
    }
}
