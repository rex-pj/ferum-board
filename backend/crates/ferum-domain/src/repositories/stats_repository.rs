use async_trait::async_trait;

use crate::AppError;

/// Aggregate counts for the admin dashboard. Raw values straight from the
/// database; the use case derives presentation fields (e.g. dau/mau ratio).
pub struct DashboardCounts {
    pub users_total: i64,
    pub threads_total: i64,
    pub posts_total: i64,
    pub reports_pending: i64,
    pub new_users_today: i64,
    pub new_threads_today: i64,
    pub new_posts_today: i64,
    pub new_reactions_today: i64,
    pub new_views_today: i64,
    pub dau: i64,
    pub mau: i64,
    pub activation_rate_pct: i64,
    pub oldest_pending_report_hours: Option<f64>,
}

/// One day of a single metric's history.
pub struct StatHistoryPoint {
    pub date: chrono::NaiveDate,
    pub value: i64,
}

/// Persistence boundary for analytics queries. Concrete implementation lives in
/// `ferum-infrastructure` and is built against generated Sea-ORM entities, so a
/// table/column rename surfaces as a compile error rather than a runtime failure.
#[async_trait]
pub trait StatsRepository: Send + Sync {
    /// Aggregate counts for the dashboard (single round-trip).
    async fn dashboard_counts(&self) -> Result<DashboardCounts, AppError>;

    /// Snapshot today's metrics into `daily_stats` (idempotent upsert).
    async fn flush_daily_stats(&self) -> Result<(), AppError>;

    /// Backfill historical `daily_stats` rows from source tables (idempotent).
    async fn backfill_history(&self) -> Result<(), AppError>;

    /// Time-series history for one metric over the last `days` days.
    async fn stats_history(
        &self,
        metric: &str,
        days: i32,
    ) -> Result<Vec<StatHistoryPoint>, AppError>;
}
