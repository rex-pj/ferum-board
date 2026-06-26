//! Integration tests for [`PgStatsRepository`].
//!
//! Compiled only under `--features db-tests`. They execute every analytics query
//! against a real PostgreSQL, exercising the parts the compiler cannot check:
//! the `COUNT(*) FILTER` activation expression, the enum-literal casts, the
//! `UNION ALL` flush upsert, and the `generate_series` history join.
#![cfg(feature = "db-tests")]

mod common;

use common::TestDb;
use ferum_domain::repositories::stats_repository::StatsRepository;
use ferum_infrastructure::repositories::PgStatsRepository;

#[tokio::test]
async fn dashboard_counts_on_empty_schema() {
    let db = TestDb::new("stats_dashboard").await;
    let repo = PgStatsRepository::new(db.conn.clone());

    let c = repo
        .dashboard_counts()
        .await
        .expect("dashboard_counts executes against the live schema");

    // A freshly migrated schema has no rows: every count is zero and no pending
    // report exists. This proves the scalar subqueries + FILTER expression run.
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

    // backfill_history + flush_daily_stats must both be idempotent and must run
    // their ON CONFLICT upserts without error on an empty schema.
    repo.backfill_history()
        .await
        .expect("backfill_history runs (ON CONFLICT DO NOTHING)");
    repo.flush_daily_stats()
        .await
        .expect("flush_daily_stats runs (UNION ALL + ON CONFLICT DO UPDATE)");
    // Idempotency: a second flush hits the DO UPDATE branch.
    repo.flush_daily_stats()
        .await
        .expect("second flush re-runs cleanly");

    // generate_series yields one row per day in the window even with no data.
    let history = repo
        .stats_history("dau", 7)
        .await
        .expect("stats_history runs the generate_series join");
    assert_eq!(history.len(), 8, "7-day window is inclusive of both ends");
    assert!(history.iter().all(|p| p.value == 0));

    db.teardown().await;
}
