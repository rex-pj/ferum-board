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
/// Persistence boundary for analytics queries.
///
/// ## Why every method takes `tz`
///
/// A daily metric is only meaningful once you say *whose* day. These queries
/// used bare `CURRENT_DATE`, which resolves against the database session's
/// `TimeZone` — a value `initdb` copies from whatever host the cluster was
/// created on. The day boundary was therefore an accident of deployment: the
/// same code and the same rows produced different DAU figures on two machines,
/// and nothing surfaced the difference.
///
/// Passing the zone in makes the boundary an explicit, testable argument. `tz`
/// is an IANA name (`"UTC"`, `"America/Sao_Paulo"`, …) from the
/// `reporting_timezone` site-config key, and it is always sent to Postgres as a
/// **bind parameter**, never interpolated.
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
