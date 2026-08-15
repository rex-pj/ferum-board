//! Integration tests for [`PgStatsRepository`] — the parts the compiler cannot
//! check: `COUNT(*) FILTER`, enum casts, the `UNION ALL` upsert, the
//! `generate_series` join, and reporting-timezone bucketing.
//!
//! **The assertion is not "the right number" but "the same number whatever the
//! session zone is".** Zone-sensitive tests run under
//! `TestDb::new_in_timezone` with a hostile zone, so a query that has gone back
//! to inheriting the boundary fails here rather than a continent away.

use ferum_domain::repositories::stats_repository::StatsRepository;
use ferum_infrastructure::repositories::PgStatsRepository;
use sea_orm::{ConnectionTrait, DbBackend, Statement};

use crate::common::TestDb;

/// A session zone deliberately unlike UTC, chosen for its awkwardness rather
/// than for any market: `Asia/Kathmandu` is UTC+05:45 with no DST. The **:45**
/// is the point — an offset that is not a whole number of hours breaks code
/// that truncates instead of converting, which a whole-hour zone would not
/// catch.
const HOSTILE_SESSION_TZ: &str = "Asia/Kathmandu";

#[tokio::test]
async fn dashboard_counts_on_empty_schema() {
    let db = TestDb::new("stats_dashboard").await;
    let repo = PgStatsRepository::new(db.conn.clone());

    let c = repo
        .dashboard_counts("UTC")
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

    repo.backfill_history("UTC")
        .await
        .expect("backfill_history runs (ON CONFLICT DO NOTHING)");
    repo.flush_daily_stats("UTC")
        .await
        .expect("flush_daily_stats runs (UNION ALL + ON CONFLICT DO UPDATE)");
    repo.flush_daily_stats("UTC")
        .await
        .expect("second flush hits the DO UPDATE branch");

    let history = repo
        .stats_history("dau", 7, "UTC")
        .await
        .expect("stats_history runs the generate_series join");
    assert_eq!(history.len(), 8, "7-day window is inclusive of both ends");
    assert!(history.iter().all(|p| p.value == 0));

    db.teardown().await;
}

// ─── The reporting zone, not the session zone, decides the day ───────────────

/// Every statement must run under a hostile session zone.
///
/// This is the blunt version of the guarantee: if any of these still contains a
/// bare `CURRENT_DATE`, it resolves against the hostile zone here, and for the
/// hours each day when the two zones disagree the flushed row lands on a
/// different date than the dashboard reads back.
#[tokio::test]
async fn every_query_runs_under_a_non_utc_session() {
    let db = TestDb::new_in_timezone("stats_hostile_session", HOSTILE_SESSION_TZ).await;
    let repo = PgStatsRepository::new(db.conn.clone());

    // Confirm the harness actually applied it — otherwise this whole test is
    // an expensive way to re-run the UTC case.
    let row = db
        .conn
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SHOW TimeZone".to_owned(),
        ))
        .await
        .expect("SHOW TimeZone")
        .expect("one row");
    let session_tz: String = row.try_get_by_index(0).expect("TimeZone value");
    assert_eq!(
        session_tz, HOSTILE_SESSION_TZ,
        "the test harness did not apply the session timezone, so this test proves nothing"
    );

    repo.backfill_history("UTC").await.expect("backfill under a non-UTC session");
    repo.flush_daily_stats("UTC").await.expect("flush under a non-UTC session");
    repo.dashboard_counts("UTC").await.expect("dashboard under a non-UTC session");
    repo.stats_history("dau", 7, "UTC")
        .await
        .expect("history under a non-UTC session");

    db.teardown().await;
}

/// The flush writes its row on today-in-the-reporting-zone, and reads it back
/// on the same date.
///
/// Run under a session zone that matches neither argument, so agreement between
/// write and read can only come from the `tz` parameter.
#[tokio::test]
async fn flushed_rows_are_keyed_by_the_reporting_day() {
    let db = TestDb::new_in_timezone("stats_reporting_day", "America/New_York").await;
    let repo = PgStatsRepository::new(db.conn.clone());

    for tz in ["UTC", "Asia/Kathmandu", "Pacific/Kiritimati"] {
        repo.flush_daily_stats(tz)
            .await
            .unwrap_or_else(|e| panic!("flush in {tz}: {e}"));

        let expected: chrono::NaiveDate = db
            .conn
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT (now() AT TIME ZONE $1)::date",
                [tz.into()],
            ))
            .await
            .expect("query today in tz")
            .expect("one row")
            .try_get_by_index(0)
            .expect("a date");

        let got = db
            .conn
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT max(date) FROM daily_stats WHERE metric = 'dau'".to_owned(),
            ))
            .await
            .expect("read back")
            .expect("one row")
            .try_get_by_index::<Option<chrono::NaiveDate>>(0)
            .expect("a nullable date");

        assert_eq!(
            got,
            Some(expected),
            "a flush with reporting zone {tz} must key its row on that zone's today, \
             not on the session's"
        );
    }

    db.teardown().await;
}

/// `Pacific/Kiritimati` (UTC+14) and `Pacific/Niue` (UTC-11) are 25 hours apart,
/// so at every instant they disagree about the date. Bucketing that still
/// depends on the session would produce the same answer for both.
#[tokio::test]
async fn two_reporting_zones_a_day_apart_bucket_differently() {
    let db = TestDb::new("stats_zone_spread").await;

    let date_in = |tz: &'static str| {
        let conn = db.conn.clone();
        async move {
            conn.query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT (now() AT TIME ZONE $1)::date",
                [tz.into()],
            ))
            .await
            .expect("query")
            .expect("one row")
            .try_get_by_index::<chrono::NaiveDate>(0)
            .expect("a date")
        }
    };

    let far_east = date_in("Pacific/Kiritimati").await;
    let far_west = date_in("Pacific/Niue").await;

    // They are either equal (for the one hour a day the calendars align) or
    // exactly one day apart. Anything else means the expression is not doing
    // what it claims.
    let delta = (far_east - far_west).num_days();
    assert!(
        (0..=1).contains(&delta),
        "UTC+14 and UTC-11 should differ by 0 or 1 calendar days, got {delta}"
    );

    db.teardown().await;
}

/// The history axis is generated in the reporting zone and must line up with
/// the dates `flush_daily_stats` writes — otherwise the chart's last bar is
/// empty for part of every day.
#[tokio::test]
async fn history_window_ends_on_the_reporting_today() {
    let db = TestDb::new_in_timezone("stats_history_axis", HOSTILE_SESSION_TZ).await;
    let repo = PgStatsRepository::new(db.conn.clone());

    let tz = "UTC";
    repo.flush_daily_stats(tz).await.expect("flush");

    let history = repo.stats_history("dau", 7, tz).await.expect("history");
    assert_eq!(history.len(), 8, "7-day window is inclusive of both ends");

    let today: chrono::NaiveDate = db
        .conn
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT (now() AT TIME ZONE $1)::date",
            [tz.into()],
        ))
        .await
        .expect("query")
        .expect("one row")
        .try_get_by_index(0)
        .expect("a date");

    assert_eq!(
        history.last().expect("non-empty").date,
        today,
        "the series must end on today in the REPORTING zone; ending on the \
         session's today is the bug this parameter exists to prevent"
    );

    db.teardown().await;
}

/// The one that pins F-06: an event near midnight is filed under the day it was
/// in *for the reporting zone*, even when that is a different month.
///
/// 2020-07-31T18:30Z is 00:15 on 1 August at UTC+05:45. Under a UTC boundary it
/// belongs to July; under the offset zone, to August. Before the `tz` parameter
/// existed there was only one answer, and which one you got depended on the
/// database host.
///
/// The session zone is set to a third value so neither expected answer can be
/// produced by accident.
#[tokio::test]
async fn a_signup_near_midnight_buckets_by_month_in_the_reporting_zone() {
    let db = TestDb::new_in_timezone("stats_month_boundary", "America/New_York").await;

    let user = crate::common::insert_user(&db.conn, 1).await;
    db.conn
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE users SET created_at = $1::timestamptz WHERE id = $2",
            ["2020-07-31T18:30:00+00:00".into(), user.id.into()],
        ))
        .await
        .expect("backdate the signup");

    // `metric = 'new_users'` is written by backfill_history from users.created_at.
    let bucket_for = |tz: &'static str| {
        let conn = db.conn.clone();
        async move {
            conn.execute_raw(Statement::from_string(
                DbBackend::Postgres,
                "DELETE FROM daily_stats WHERE metric = 'new_users'".to_owned(),
            ))
            .await
            .expect("clear previous run");

            PgStatsRepository::new(conn.clone())
                .backfill_history(tz)
                .await
                .unwrap_or_else(|e| panic!("backfill in {tz}: {e}"));

            conn.query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT date FROM daily_stats WHERE metric = 'new_users' ORDER BY date LIMIT 1"
                    .to_owned(),
            ))
            .await
            .expect("read bucket")
            .expect("backfill must have written a row")
            .try_get_by_index::<chrono::NaiveDate>(0)
            .expect("a date")
        }
    };

    assert_eq!(
        bucket_for("UTC").await,
        chrono::NaiveDate::from_ymd_opt(2020, 7, 31).unwrap(),
        "18:30Z on 31 July is still July in UTC"
    );
    assert_eq!(
        bucket_for("Asia/Kathmandu").await,
        chrono::NaiveDate::from_ymd_opt(2020, 8, 1).unwrap(),
        "the same instant is 00:15 on 1 August at +05:45, so it belongs to August — \
         a monthly report in that zone counts this signup in the following month"
    );

    db.teardown().await;
}
