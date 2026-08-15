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

/// Persistence boundary for analytics queries.
///
/// **Every method takes `tz`** because a daily metric needs to say whose day.
/// Bare `CURRENT_DATE` resolves against the session zone, which `initdb` copies
/// from the host — the same rows then give different DAU on two machines, with
/// nothing surfacing the difference.
///
/// `tz` is an IANA name from `reporting_timezone`, always sent as a **bind
/// parameter**, never interpolated.
#[async_trait]
pub trait StatsRepository: Send + Sync {
    /// Aggregate counts for the dashboard (single round-trip).
    ///
    /// "Today" means the current calendar day in `tz`.
    async fn dashboard_counts(&self, tz: &str) -> Result<DashboardCounts, AppError>;

    /// Snapshot today's metrics into `daily_stats` (idempotent upsert).
    ///
    /// The row is keyed by today's date in `tz`, so changing `tz` changes which
    /// bucket subsequent flushes land in. Rows already written keep the boundary
    /// they were written under.
    async fn flush_daily_stats(&self, tz: &str) -> Result<(), AppError>;

    /// Backfill historical `daily_stats` rows from source tables (idempotent).
    ///
    /// Only fills gaps (`ON CONFLICT DO NOTHING`), so it will **not** re-bucket
    /// rows written under a different `tz` — those have to be deleted first.
    async fn backfill_history(&self, tz: &str) -> Result<(), AppError>;

    /// Time-series history for one metric over the last `days` days.
    async fn stats_history(
        &self,
        metric: &str,
        days: i32,
        tz: &str,
    ) -> Result<Vec<StatHistoryPoint>, AppError>;
}
