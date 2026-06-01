use std::time::Duration;

use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

use crate::application::ports::CacheService;
use crate::application::shared::AppError;

pub struct RedisCacheService {
    conn: ConnectionManager,
}

impl RedisCacheService {
    pub async fn new(url: &str) -> Result<Self, anyhow::Error> {
        let client = redis::Client::open(url)?;
        let conn = ConnectionManager::new(client).await?;
        Ok(Self { conn })
    }
}

#[async_trait]
impl CacheService for RedisCacheService {
    async fn get(&self, key: &str) -> Option<String> {
        let mut conn = self.conn.clone();
        conn.get::<_, Option<String>>(key).await.ok().flatten()
    }

    async fn set(&self, key: &str, value: &str, ttl: Duration) -> Result<(), AppError> {
        let mut conn = self.conn.clone();
        conn.set_ex::<_, _, ()>(key, value, ttl.as_secs())
            .await
            .map_err(|e| AppError::internal(e.to_string()))
    }

    async fn set_nx(&self, key: &str, value: &str, ttl: Duration) -> Result<bool, AppError> {
        let mut conn = self.conn.clone();
        // SET key value NX EX seconds — returns "OK" if set, nil if already existed
        let result: Option<String> = redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("NX")
            .arg("EX")
            .arg(ttl.as_secs())
            .query_async(&mut conn)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(result.is_some())
    }

    async fn del(&self, key: &str) -> Result<(), AppError> {
        let mut conn = self.conn.clone();
        conn.del::<_, ()>(key)
            .await
            .map_err(|e| AppError::internal(e.to_string()))
    }

    async fn del_prefix(&self, prefix: &str) -> Result<(), AppError> {
        let mut conn = self.conn.clone();
        let pattern = format!("{}*", prefix);
        let mut cursor: u64 = 0;
        loop {
            let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(100u64)
                .query_async(&mut conn)
                .await
                .map_err(|e| AppError::internal(e.to_string()))?;
            if !keys.is_empty() {
                conn.unlink::<_, ()>(keys)
                    .await
                    .map_err(|e| AppError::internal(e.to_string()))?;
            }
            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }
        Ok(())
    }

    async fn incr_with_ttl(&self, key: &str, ttl: Duration) -> Result<u64, AppError> {
        let mut conn = self.conn.clone();
        // INCR then set expiry only if it's a fresh key (NX = set if not exists)
        let val: u64 = conn.incr(key, 1u64)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        if val == 1 {
            conn.expire::<_, ()>(key, ttl.as_secs() as i64)
                .await
                .map_err(|e| AppError::internal(e.to_string()))?;
        }
        Ok(val)
    }

    async fn exists(&self, key: &str) -> bool {
        let mut conn = self.conn.clone();
        conn.exists::<_, bool>(key).await.unwrap_or(false)
    }
}
