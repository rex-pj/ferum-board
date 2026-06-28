//! Integration tests for [`PgStatsRepository`].
//!
//! Exercises the parts the compiler cannot check: the `COUNT(*) FILTER`
//! activation expression, enum-literal casts, the `UNION ALL` flush upsert,
//! and the `generate_series` history join.

use ferum_domain::repositories::stats_repository::StatsRepository;
use ferum_infrastructure::repositories::PgStatsRepository;

use crate::common::TestDb;

#[tokio::test]
async fn dashboard_counts_on_empty_schema() {
    let db = TestDb::new("stats_dashboard").await;
    let repo = PgStatsRepository::new(db.conn.clone());

    let c = repo
        .dashboard_counts()
        .await
        .expect("dashboard_counts executes against the live schema");

    assert_eq!(c.users_total, 0);
    assert_eq!(c.threads_total, 0);
    assert_eq!(c.posts_total, 0);
    assert_eq!(c.reports_pending, 0);
    assert_eq!(c.dau, 0);
    assert_eq!(c.mau, 0);
    assert_eq!(c.activation_rate_pct, 0);
    assert!(c.oldest_pending_report_hours.is_none());

    db.teardown().await;
}

#[tokio::test]
async fn flush_backfill_and_history_roundtrip() {
    let db = TestDb::new("stats_flush").await;
    let repo = PgStatsRepository::new(db.conn.clone());

    repo.backfill_history()
        .await
        .expect("backfill_history runs (ON CONFLICT DO NOTHING)");
    repo.flush_daily_stats()
        .await
        .expect("flush_daily_stats runs (UNION ALL + ON CONFLICT DO UPDATE)");
    repo.flush_daily_stats()
        .await
        .expect("second flush hits the DO UPDATE branch");

    let history = repo
        .stats_history("dau", 7)
        .await
        .expect("stats_history runs the generate_series join");
    assert_eq!(history.len(), 8, "7-day window is inclusive of both ends");
    assert!(history.iter().all(|p| p.value == 0));

    db.teardown().await;
}
