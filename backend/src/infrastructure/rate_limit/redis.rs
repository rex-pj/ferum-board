#![allow(dead_code)]

use std::time::Duration;

use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

use crate::application::ports::{RateLimitResult, RateLimiter};
use crate::application::shared::AppError;

pub struct RedisRateLimiter {
    conn: ConnectionManager,
}

impl RedisRateLimiter {
    pub async fn new(url: &str) -> Result<Self, anyhow::Error> {
        let client = redis::Client::open(url)?;
        let conn = ConnectionManager::new(client).await?;
        Ok(Self { conn })
    }
}

#[async_trait]
impl RateLimiter for RedisRateLimiter {
    async fn check(&self, key: &str, limit: u32, window: Duration) -> Result<RateLimitResult, AppError> {
        let mut conn = self.conn.clone();

        // Atomic INCR; set TTL only on first call within the window
        let count: u32 = conn.incr(key, 1u32)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;

        if count == 1 {
            conn.expire::<_, ()>(key, window.as_secs() as i64)
                .await
                .map_err(|e| AppError::internal(e.to_string()))?;
        }

        if count > limit {
            let ttl: i64 = conn.ttl(key).await.unwrap_or(0);
            let retry_after = Duration::from_secs(ttl.max(0) as u64);
            Ok(RateLimitResult::Denied { retry_after })
        } else {
            Ok(RateLimitResult::Allowed { remaining: limit.saturating_sub(count) })
        }
    }
}
