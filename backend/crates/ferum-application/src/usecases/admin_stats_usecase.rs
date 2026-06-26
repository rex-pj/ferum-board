use std::sync::Arc;
use std::time::Duration;

use crate::dto::{DashboardStats, StatPoint};
use crate::permission::PermissionChecker;
use crate::ports::CacheService;
use crate::shared::AppError;
use ferum_domain::repositories::stats_repository::StatsRepository;
use ferum_domain::AuthUser;

const DASHBOARD_CACHE_KEY: &str = "stats:dashboard";
const DASHBOARD_CACHE_TTL: Duration = Duration::from_secs(60);

/// Admin analytics use case. Owns permissioning, caching and presentation
/// mapping; all database access is delegated to `StatsRepository` so the query
/// SQL lives behind entity types in the infrastructure layer.
pub struct AdminStatsUseCase {
    repo: Arc<dyn StatsRepository>,
    cache: Option<Arc<dyn CacheService>>,
}

impl AdminStatsUseCase {
    pub fn new(repo: Arc<dyn StatsRepository>) -> Self {
        Self { repo, cache: None }
    }

    pub fn with_cache(mut self, cache: Arc<dyn CacheService>) -> Self {
        self.cache = Some(cache);
        self
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id))]
    pub async fn dashboard(&self, actor: &AuthUser) -> Result<DashboardStats, AppError> {
        PermissionChecker::can_manage_users(actor)?;

        if let Some(cache) = &self.cache {
            if let Some(cached) = cache.get(DASHBOARD_CACHE_KEY).await {
                if let Ok(stats) = serde_json::from_str::<DashboardStats>(&cached) {
                    return Ok(stats);
                }
            }
        }

        let c = self.repo.dashboard_counts().await?;

        let dau_mau_ratio = if c.mau > 0 {
            (c.dau as f64 / c.mau as f64 * 100.0 * 10.0).round() / 10.0
        } else {
            0.0
        };

        let stats = DashboardStats {
            users_total: c.users_total as u64,
            threads_total: c.threads_total as u64,
            posts_total: c.posts_total as u64,
            reports_pending: c.reports_pending as u64,
            new_users_today: c.new_users_today as u64,
            new_threads_today: c.new_threads_today as u64,
            new_posts_today: c.new_posts_today as u64,
            new_reactions_today: c.new_reactions_today as u64,
            new_views_today: c.new_views_today as u64,
            dau: c.dau as u64,
            mau: c.mau as u64,
            dau_mau_ratio,
            activation_rate_pct: c.activation_rate_pct as u64,
            oldest_pending_report_hours: c.oldest_pending_report_hours.map(|h| h as u64),
        };

        if let Some(cache) = &self.cache {
            if let Ok(json) = serde_json::to_string(&stats) {
                let _ = cache
                    .set(DASHBOARD_CACHE_KEY, &json, DASHBOARD_CACHE_TTL)
                    .await;
            }
        }

        Ok(stats)
    }

    /// Writes today's metric snapshot to `daily_stats`. Called from a background
    /// task every hour — no AuthUser required.
    pub async fn flush_daily_stats(&self) -> Result<(), AppError> {
        self.repo.flush_daily_stats().await?;

        if let Some(cache) = &self.cache {
            let _ = cache.del(DASHBOARD_CACHE_KEY).await;
        }

        Ok(())
    }

    /// Backfills historical `daily_stats` from source-table timestamps for all
    /// dates before today. Safe to call on every startup — fully idempotent.
    pub async fn backfill_history(&self) -> Result<(), AppError> {
        self.repo.backfill_history().await
    }

    /// Returns the history of one metric for the last N days (capped at 90).
    pub async fn stats_history(
        &self,
        actor: &AuthUser,
        metric: &str,
        days: u32,
    ) -> Result<Vec<StatPoint>, AppError> {
        PermissionChecker::can_manage_users(actor)?;

        let days = days.clamp(1, 90) as i32;
        let allowed = [
            "dau",
            "new_users",
            "new_threads",
            "new_posts",
            "new_reactions",
            "new_views",
            "activation_rate_pct",
        ];
        if !allowed.contains(&metric) {
            return Err(AppError::UnprocessableEntity(format!(
                "unknown metric '{metric}'"
            )));
        }

        let points = self.repo.stats_history(metric, days).await?;

        Ok(points
            .into_iter()
            .map(|p| StatPoint {
                date: p.date.to_string(),
                value: p.value as u64,
            })
            .collect())
    }
}

