//! Dashboard counts and historical series.
//!
//! Day buckets use `(ts AT TIME ZONE <reporting_timezone>)::date`, never bare
//! `CURRENT_DATE` — the boundary must be named, not inherited from the session.

use async_trait::async_trait;
use sea_orm::sea_query::{
    Alias, Asterisk, Condition, Expr, ExprTrait, Func, JoinType, OnConflict, Order,
    PostgresQueryBuilder, Query, SelectStatement, SimpleExpr, SubQueryStatement, UnionType, Values,
};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, FromQueryResult, Statement};

use crate::entities::{daily_stats, posts, reactions, reports, threads, users};
use ferum_application::shared::AppError;
use ferum_domain::repositories::stats_repository::{
    DashboardCounts, StatHistoryPoint, StatsRepository,
};

/// Sea-ORM-backed analytics queries. Table and column identifiers are taken from
/// generated entities (`users::Column::CreatedAt`, …), so a schema rename breaks
/// compilation here. Only genuinely non-expressible PostgreSQL constructs stay as
/// `Expr::cust`: enum-literal casts (`'deleted'::thread_status`), interval
/// arithmetic, `COUNT(*) FILTER`, `EXTRACT`, and `generate_series` step.
pub struct PgStatsRepository {
    db: DatabaseConnection,
}

impl PgStatsRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

/// Wrap a SELECT as a scalar subquery expression.
fn scalar(stmt: SelectStatement) -> SimpleExpr {
    SimpleExpr::SubQuery(None, Box::new(SubQueryStatement::SelectStatement(stmt)))
}

// ─── Reporting day boundaries ────────────────────────────────────────────────
//
// Never bare `CURRENT_DATE` — it resolves against the session zone, so the day
// boundary would come from whatever host ran `initdb`, and even UTC splits one
// local day across two dates for a single-region community.
//
// The zone is BOUND as a parameter, so a hostile config value cannot become SQL.
// The round trip is the standard idiom:
//   ts AT TIME ZONE tz → timestamp; date_trunc('day', …) → local midnight;
//   … AT TIME ZONE tz  → timestamptz (the instant that midnight happened)

/// Today's date **in the reporting zone**, as a `DATE`. Use for `daily_stats.date`.
fn today(tz: &str) -> SimpleExpr {
    Expr::cust_with_values("(now() AT TIME ZONE $1)::date", [tz])
}

/// The instant at which the current reporting day began, as a `timestamptz`.
///
/// This is what a `created_at >= …` comparison needs: comparing a `timestamptz`
/// against a bare `DATE` makes Postgres cast the date using the *session* zone,
/// which is the dependency being removed.
fn start_of_today(tz: &str) -> SimpleExpr {
    // Two numbered placeholders and two copies of the same value. Reusing `$1`
    // twice also works — sea-query resolves a numbered placeholder by index
    // (`values[num - 1]`) and emits a fresh bind each time it appears — but
    // spelling out `$1`/`$2` keeps the mapping obvious at the call site, and a
    // mistake here would not fail loudly: it would silently move the day
    // boundary.
    Expr::cust_with_values(
        "(date_trunc('day', now() AT TIME ZONE $1) AT TIME ZONE $2)",
        [tz, tz],
    )
}

/// Yesterday's date in the reporting zone, as a `DATE`.
///
/// Subtracting the interval from the *reporting* date rather than from
/// `CURRENT_DATE` is what keeps the snapshot lookup aligned with the row
/// `flush_daily_stats` actually wrote.
fn yesterday(tz: &str) -> SimpleExpr {
    Expr::cust_with_values(
        "((now() AT TIME ZONE $1)::date - INTERVAL '1 day')::date",
        [tz],
    )
}

/// Bucket a `timestamptz` column into its reporting-zone calendar date.
///
/// `col` is a SQL fragment, always a literal from a call site in this file —
/// never anything reaching the request. Only `tz` is bound, because only `tz`
/// comes from configuration.
fn day_bucket(col: &str, tz: &str) -> SimpleExpr {
    Expr::cust_with_values(format!("({col} AT TIME ZONE $1)::date"), [tz])
}

/// `GROUP BY 1` — by ordinal, not style. The bucket expression holds a bind
/// parameter and `cust_with_values` emits a fresh placeholder each time, so
/// SELECT gets `$1` and GROUP BY `$3`. Postgres matches GROUP BY syntactically
/// and rejects the query outright; an ordinal sidesteps the comparison.
fn group_by_first_column() -> SimpleExpr {
    Expr::cust("1")
}

/// Percentage of users who registered *yesterday* and have posted *today*.
///
/// Extracted because `dashboard_counts` and `flush_daily_stats` both need it and
/// held byte-identical copies — which is how the live dashboard and the stored
/// snapshot would have drifted apart the first time one was edited.
///
/// `COUNT(*) FILTER (WHERE EXISTS (…))` has no sea-query AST node, so the
/// aggregate body stays raw; the FROM is still entity-typed.
fn activation_rate(tz: &str) -> SelectStatement {
    Query::select()
        .expr(Expr::cust_with_values(
            "CASE WHEN COUNT(*) = 0 THEN 0 \
             ELSE ROUND(100.0 * COUNT(*) FILTER (\
                 WHERE EXISTS (\
                     SELECT 1 FROM posts p \
                     WHERE p.author_id = u.id \
                       AND p.created_at >= (date_trunc('day', now() AT TIME ZONE $1) \
                                            AT TIME ZONE $2) \
                       AND p.is_deleted = false\
                 )\
             ) / COUNT(*))::BIGINT END",
            [tz, tz],
        ))
        .from_as(users::Entity, Alias::new("u"))
        .and_where(Expr::cust_with_values(
            "u.created_at >= (date_trunc('day', now() AT TIME ZONE $1) AT TIME ZONE $2) \
             - INTERVAL '1 day'",
            [tz, tz],
        ))
        .to_owned()
}

/// `COUNT(*)` over an entity table with an optional extra predicate, as a scalar
/// subquery — the shared shape behind most dashboard/flush metrics.
fn count_today(
    from: impl sea_orm::sea_query::IntoTableRef,
    when: SimpleExpr,
) -> SelectStatement {
    Query::select()
        .expr(Func::count(Expr::col(Asterisk)))
        .from(from)
        .and_where(when)
        .to_owned()
}

#[async_trait]
impl StatsRepository for PgStatsRepository {
    async fn dashboard_counts(&self, tz: &str) -> Result<DashboardCounts, AppError> {
        #[derive(FromQueryResult)]
        struct Row {
            users_total: i64,
            threads_total: i64,
            posts_total: i64,
            reports_pending: i64,
            new_users_today: i64,
            new_threads_today: i64,
            new_posts_today: i64,
            new_reactions_today: i64,
            new_views_today: i64,
            dau: i64,
            mau: i64,
            activation_rate_pct: i64,
            oldest_pending_report_hours: Option<f64>,
        }

        let users_total = Query::select()
            .expr(Func::count(Expr::col(Asterisk)))
            .from(users::Entity)
            .to_owned();

        let threads_total = Query::select()
            .expr(Func::count(Expr::col(Asterisk)))
            .from(threads::Entity)
            .and_where(Expr::col(threads::Column::Status).ne(Expr::cust("'deleted'::thread_status")))
            .to_owned();

        let posts_total = Query::select()
            .expr(Func::count(Expr::col(Asterisk)))
            .from(posts::Entity)
            .and_where(Expr::col(posts::Column::IsDeleted).eq(false))
            .to_owned();

        let reports_pending = Query::select()
            .expr(Func::count(Expr::col(Asterisk)))
            .from(reports::Entity)
            .and_where(Expr::col(reports::Column::Status).eq(Expr::cust("'pending'::report_status")))
            .to_owned();

        let new_users_today = count_today(
            users::Entity,
            Expr::col(users::Column::CreatedAt).gte(start_of_today(tz)),
        );

        let new_threads_today = Query::select()
            .expr(Func::count(Expr::col(Asterisk)))
            .from(threads::Entity)
            .and_where(Expr::col(threads::Column::CreatedAt).gte(start_of_today(tz)))
            .and_where(Expr::col(threads::Column::Status).ne(Expr::cust("'deleted'::thread_status")))
            .to_owned();

        let new_posts_today = Query::select()
            .expr(Func::count(Expr::col(Asterisk)))
            .from(posts::Entity)
            .and_where(Expr::col(posts::Column::CreatedAt).gte(start_of_today(tz)))
            .and_where(Expr::col(posts::Column::IsDeleted).eq(false))
            .to_owned();

        let new_reactions_today = count_today(
            reactions::Entity,
            Expr::col(reactions::Column::CreatedAt).gte(start_of_today(tz)),
        );

        // Compute new_views_today live — same formula as flush_daily_stats — so the
        // dashboard reflects views within the 60s buffer flush cycle rather than
        // waiting for the hourly daily_stats snapshot.
        //
        // Fallback: when no yesterday snapshot exists (fresh install or seed data),
        // COALESCE to view_total so the delta is 0 instead of the full cumulative sum.
        let view_total_live = || {
            Query::select()
                .expr(Func::coalesce([
                    // sea-query 1.0 unified `SimpleExpr` into `Expr`; a bare
                    // `.into()` no longer resolves to a unique target type.
                    Expr::from(Func::sum(Expr::col(threads::Column::ViewCount))),
                    Expr::val(0i64),
                ]))
                .from(threads::Entity)
                .and_where(
                    Expr::col(threads::Column::Status).ne(Expr::cust("'deleted'::thread_status")),
                )
                .to_owned()
        };

        let view_yesterday_snapshot = Query::select()
            .expr(Expr::col(daily_stats::Column::Value))
            .from(daily_stats::Entity)
            .and_where(Expr::col(daily_stats::Column::Metric).eq("view_count_snapshot"))
            .and_where(Expr::col(daily_stats::Column::Date).eq(yesterday(tz)))
            .and_where(Expr::col(daily_stats::Column::CategoryId).is_null())
            .to_owned();

        let new_views_today: SimpleExpr = Func::cust(Alias::new("GREATEST"))
            .args([
                Expr::val(0i64),
                Expr::expr(scalar(view_total_live())).sub(Func::coalesce([
                    scalar(view_yesterday_snapshot),
                    scalar(view_total_live()), // no snapshot → delta = 0
                ])),
            ])
            .into();

        // DAU and MAU deliberately use two different clocks, and it is worth
        // saying so because they sit next to each other and used to look like an
        // inconsistency:
        //   DAU — "active during today", a CALENDAR DAY in the reporting zone.
        //   MAU — "active in the last 30 days", a ROLLING WINDOW, which is a
        //         duration and has no day boundary to get wrong.
        let dau = count_today(
            users::Entity,
            Expr::col(users::Column::LastSeenAt).gte(start_of_today(tz)),
        );

        let mau = count_today(
            users::Entity,
            Expr::col(users::Column::LastSeenAt).gte(Expr::cust("now() - INTERVAL '30 days'")),
        );

        let activation_rate_pct = activation_rate(tz);

        let oldest_pending_report_hours = Query::select()
            // A rolling age, not a day boundary — `now()` is the right clock and
            // needs no zone.
            .expr(Expr::cust(
                "(EXTRACT(EPOCH FROM (now() - MIN(created_at))) / 3600.0)::FLOAT8",
            ))
            .from(reports::Entity)
            .and_where(Expr::col(reports::Column::Status).eq(Expr::cust("'pending'::report_status")))
            .to_owned();

        let (sql, values) = Query::select()
            .expr_as(scalar(users_total), Alias::new("users_total"))
            .expr_as(scalar(threads_total), Alias::new("threads_total"))
            .expr_as(scalar(posts_total), Alias::new("posts_total"))
            .expr_as(scalar(reports_pending), Alias::new("reports_pending"))
            .expr_as(scalar(new_users_today), Alias::new("new_users_today"))
            .expr_as(scalar(new_threads_today), Alias::new("new_threads_today"))
            .expr_as(scalar(new_posts_today), Alias::new("new_posts_today"))
            .expr_as(scalar(new_reactions_today), Alias::new("new_reactions_today"))
            .expr_as(new_views_today, Alias::new("new_views_today"))
            .expr_as(scalar(dau), Alias::new("dau"))
            .expr_as(scalar(mau), Alias::new("mau"))
            .expr_as(scalar(activation_rate_pct), Alias::new("activation_rate_pct"))
            .expr_as(
                scalar(oldest_pending_report_hours),
                Alias::new("oldest_pending_report_hours"),
            )
            .build(PostgresQueryBuilder);

        let row = Row::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
        .one(&self.db)
        .await?
        .ok_or_else(|| AppError::internal("stats query returned no row"))?;

        Ok(DashboardCounts {
            users_total: row.users_total,
            threads_total: row.threads_total,
            posts_total: row.posts_total,
            reports_pending: row.reports_pending,
            new_users_today: row.new_users_today,
            new_threads_today: row.new_threads_today,
            new_posts_today: row.new_posts_today,
            new_reactions_today: row.new_reactions_today,
            new_views_today: row.new_views_today,
            dau: row.dau,
            mau: row.mau,
            activation_rate_pct: row.activation_rate_pct,
            oldest_pending_report_hours: row.oldest_pending_report_hours,
        })
    }

    async fn flush_daily_stats(&self, tz: &str) -> Result<(), AppError> {
        // ── Bootstrap yesterday's snapshot on first run ─────────────────────
        // When no view_count_snapshot exists for yesterday (fresh install or the
        // server missed a day), seed it with the current cumulative total so the
        // next delta computation starts from a meaningful baseline instead of 0.
        // ON CONFLICT DO NOTHING makes this idempotent across every 5-minute tick.
        {
            let view_now = Query::select()
                .expr(Func::coalesce([
                    // sea-query 1.0 unified `SimpleExpr` into `Expr`; a bare
                    // `.into()` no longer resolves to a unique target type.
                    Expr::from(Func::sum(Expr::col(threads::Column::ViewCount))),
                    Expr::val(0i64),
                ]))
                .from(threads::Entity)
                .and_where(
                    Expr::col(threads::Column::Status)
                        .ne(Expr::cust("'deleted'::thread_status")),
                )
                .to_owned();

            let seed_row = Query::select()
                .expr(yesterday(tz))
                .expr(Expr::val("view_count_snapshot"))
                .expr(scalar(view_now))
                .to_owned();

            let mut seed_ins = Query::insert();
            seed_ins
                .into_table(daily_stats::Entity)
                .columns([
                    daily_stats::Column::Date,
                    daily_stats::Column::Metric,
                    daily_stats::Column::Value,
                ]);
            seed_ins.select_from(seed_row)
                .map_err(|e| AppError::internal(format!("daily_stats INSERT ... SELECT column mismatch: {e}")))?;
            seed_ins.on_conflict(
                OnConflict::columns([daily_stats::Column::Date, daily_stats::Column::Metric])
                    .target_and_where(Expr::col(daily_stats::Column::CategoryId).is_null())
                    .do_nothing()
                    .to_owned(),
            );
            let (sql, values) = seed_ins.build(PostgresQueryBuilder);
            self.db
                .execute_raw(Statement::from_sql_and_values(DbBackend::Postgres, sql, values))
                .await?;
        }

        // SUM(view_count) over non-deleted threads — needed by both the snapshot
        // and the new_views delta, so built fresh on each use.
        let view_total = || {
            Query::select()
                .expr(Func::coalesce([
                    // sea-query 1.0 unified `SimpleExpr` into `Expr`; a bare
                    // `.into()` no longer resolves to a unique target type.
                    Expr::from(Func::sum(Expr::col(threads::Column::ViewCount))),
                    Expr::val(0i64),
                ]))
                .from(threads::Entity)
                .and_where(
                    Expr::col(threads::Column::Status).ne(Expr::cust("'deleted'::thread_status")),
                )
                .to_owned()
        };

        let view_yesterday = Query::select()
            .expr(Expr::col(daily_stats::Column::Value))
            .from(daily_stats::Entity)
            .and_where(Expr::col(daily_stats::Column::Metric).eq("view_count_snapshot"))
            .and_where(Expr::col(daily_stats::Column::Date).eq(yesterday(tz)))
            .and_where(Expr::col(daily_stats::Column::CategoryId).is_null())
            .to_owned();

        let dau = scalar(count_today(
            users::Entity,
            Expr::col(users::Column::LastSeenAt).gte(start_of_today(tz)),
        ));
        let new_users = scalar(count_today(
            users::Entity,
            Expr::col(users::Column::CreatedAt).gte(start_of_today(tz)),
        ));
        let new_threads = scalar(
            Query::select()
                .expr(Func::count(Expr::col(Asterisk)))
                .from(threads::Entity)
                .and_where(Expr::col(threads::Column::CreatedAt).gte(start_of_today(tz)))
                .and_where(
                    Expr::col(threads::Column::Status).ne(Expr::cust("'deleted'::thread_status")),
                )
                .to_owned(),
        );
        let new_posts = scalar(
            Query::select()
                .expr(Func::count(Expr::col(Asterisk)))
                .from(posts::Entity)
                .and_where(Expr::col(posts::Column::CreatedAt).gte(start_of_today(tz)))
                .and_where(Expr::col(posts::Column::IsDeleted).eq(false))
                .to_owned(),
        );
        let new_reactions = scalar(count_today(
            reactions::Entity,
            Expr::col(reactions::Column::CreatedAt).gte(start_of_today(tz)),
        ));
        let snapshot = scalar(view_total());
        // GREATEST(0, view_total - view_yesterday); when no yesterday snapshot exists
        // (fresh install), fall back to view_total so the delta is 0 rather than
        // the full cumulative sum appearing as a single day's views.
        let view_yesterday_expr =
            Func::coalesce([scalar(view_yesterday), scalar(view_total())]);
        let new_views: SimpleExpr = Func::cust(Alias::new("GREATEST"))
            .args([
                Expr::val(0i64),
                Expr::expr(scalar(view_total())).sub(view_yesterday_expr),
            ])
            .into();
        let activation = scalar(activation_rate(tz));

        // One UNION ALL arm per metric: SELECT <today in tz>, '<metric>', <value>.
        let arm = |metric: &str, value: SimpleExpr| {
            Query::select()
                .expr(today(tz))
                .expr(Expr::val(metric))
                .expr(value)
                .to_owned()
        };

        let mut rows = arm("dau", dau);
        rows.union(UnionType::All, arm("new_users", new_users));
        rows.union(UnionType::All, arm("new_threads", new_threads));
        rows.union(UnionType::All, arm("new_posts", new_posts));
        rows.union(UnionType::All, arm("new_reactions", new_reactions));
        rows.union(UnionType::All, arm("new_views", new_views));
        rows.union(UnionType::All, arm("view_count_snapshot", snapshot));
        rows.union(UnionType::All, arm("activation_rate_pct", activation));

        let mut ins = Query::insert();
        ins.into_table(daily_stats::Entity).columns([
            daily_stats::Column::Date,
            daily_stats::Column::Metric,
            daily_stats::Column::Value,
        ]);
        ins.select_from(rows)
            .map_err(|e| AppError::internal(format!("daily_stats INSERT ... SELECT column mismatch: {e}")))?;
        ins.on_conflict(
            OnConflict::columns([daily_stats::Column::Date, daily_stats::Column::Metric])
                .target_and_where(Expr::col(daily_stats::Column::CategoryId).is_null())
                .update_column(daily_stats::Column::Value)
                .to_owned(),
        );
        let (sql, values) = ins.build(PostgresQueryBuilder);

        self.db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                sql,
                values,
            ))
            .await?;

        Ok(())
    }

    async fn backfill_history(&self, tz: &str) -> Result<(), AppError> {
        // `day_bucket` names the zone; never `CAST(created_at AS date)`, which
        // casts a timestamptz through the SESSION zone and lets a backfill
        // disagree with the dashboard it was meant to reconstruct.
        //
        // ON CONFLICT DO NOTHING means this only FILLS GAPS — re-running after a
        // zone change leaves the old rows as they were.
        let build = |day_select: SelectStatement| -> Result<(String, Values), AppError> {
            let mut ins = Query::insert();
            ins.into_table(daily_stats::Entity).columns([
                daily_stats::Column::Date,
                daily_stats::Column::Metric,
                daily_stats::Column::Value,
            ]);
            ins.select_from(day_select)
                .map_err(|e| AppError::internal(format!("daily_stats INSERT ... SELECT column mismatch: {e}")))?;
            ins.on_conflict(
                OnConflict::columns([daily_stats::Column::Date, daily_stats::Column::Metric])
                    .target_and_where(Expr::col(daily_stats::Column::CategoryId).is_null())
                    .do_nothing()
                    .to_owned(),
            );
            Ok(ins.build(PostgresQueryBuilder))
        };

        // The bucket expression is reused in SELECT, WHERE and GROUP BY, and
        // must be spelled identically in all three or Postgres rejects the
        // grouping. Only today is excluded — it is still accumulating, and
        // `flush_daily_stats` owns it.
        let day = day_bucket("created_at", tz);

        let users_stmt = build(
            Query::select()
                .expr(day.clone())
                .expr(Expr::val("new_users"))
                .expr(Func::count(Expr::col(Asterisk)))
                .from(users::Entity)
                .and_where(Expr::expr(day.clone()).lt(today(tz)))
                .add_group_by([group_by_first_column()])
                .to_owned(),
        )?;
        let threads_stmt = build(
            Query::select()
                .expr(day.clone())
                .expr(Expr::val("new_threads"))
                .expr(Func::count(Expr::col(Asterisk)))
                .from(threads::Entity)
                .and_where(Expr::expr(day.clone()).lt(today(tz)))
                .and_where(
                    Expr::col(threads::Column::Status).ne(Expr::cust("'deleted'::thread_status")),
                )
                .add_group_by([group_by_first_column()])
                .to_owned(),
        )?;
        let posts_stmt = build(
            Query::select()
                .expr(day.clone())
                .expr(Expr::val("new_posts"))
                .expr(Func::count(Expr::col(Asterisk)))
                .from(posts::Entity)
                .and_where(Expr::expr(day.clone()).lt(today(tz)))
                .and_where(Expr::col(posts::Column::IsDeleted).eq(false))
                .add_group_by([group_by_first_column()])
                .to_owned(),
        )?;
        let reactions_stmt = build(
            Query::select()
                .expr(day.clone())
                .expr(Expr::val("new_reactions"))
                .expr(Func::count(Expr::col(Asterisk)))
                .from(reactions::Entity)
                .and_where(Expr::expr(day.clone()).lt(today(tz)))
                .add_group_by([group_by_first_column()])
                .to_owned(),
        )?;

        for (sql, values) in [users_stmt, threads_stmt, posts_stmt, reactions_stmt] {
            self.db
                .execute_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    sql,
                    values,
                ))
                .await?;
        }

        Ok(())
    }

    async fn stats_history(
        &self,
        metric: &str,
        days: i32,
        tz: &str,
    ) -> Result<Vec<StatHistoryPoint>, AppError> {
        #[derive(FromQueryResult)]
        struct Row {
            date: chrono::NaiveDate,
            value: i64,
        }

        let s = Alias::new("s");
        let d = Alias::new("d");

        let query = Query::select()
            .expr_as(Expr::cust("d::date"), Alias::new("date"))
            .expr_as(
                Func::coalesce([
                    Expr::col((s.clone(), daily_stats::Column::Value)),
                    Expr::val(0i64),
                ]),
                Alias::new("value"),
            )
            // The axis is generated from today *in the reporting zone*, so the
            // series lines up with the dates `flush_daily_stats` writes. Built
            // from a plain `timestamp` (no zone) on purpose: this is a sequence
            // of calendar labels to join on, not a sequence of instants, so a
            // DST transition inside the range must not shift the steps.
            .from_function(
                Func::cust(Alias::new("generate_series")).args([
                    Expr::cust_with_values(
                        "((now() AT TIME ZONE $1)::date - ($2::int * INTERVAL '1 day'))::timestamp",
                        [sea_orm::Value::from(tz), sea_orm::Value::from(days)],
                    ),
                    Expr::cust_with_values("(now() AT TIME ZONE $1)::date::timestamp", [tz]),
                    Expr::cust("INTERVAL '1 day'"),
                ]),
                d.clone(),
            )
            .join_as(
                JoinType::LeftJoin,
                daily_stats::Entity,
                s.clone(),
                Condition::all()
                    .add(Expr::col((s.clone(), daily_stats::Column::Date)).eq(Expr::cust("d::date")))
                    .add(Expr::col((s.clone(), daily_stats::Column::Metric)).eq(metric))
                    .add(Expr::col((s.clone(), daily_stats::Column::CategoryId)).is_null()),
            )
            .order_by_expr(Expr::col(d), Order::Asc)
            .to_owned();

        let (sql, values) = query.build(PostgresQueryBuilder);

        let rows = Row::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
        .all(&self.db)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| StatHistoryPoint {
                date: r.date,
                value: r.value,
            })
            .collect())
    }
}
