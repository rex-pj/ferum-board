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
    async fn dashboard_counts(&self) -> Result<DashboardCounts, AppError> {
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
            Expr::col(users::Column::CreatedAt).gte(Expr::current_date()),
        );

        let new_threads_today = Query::select()
            .expr(Func::count(Expr::col(Asterisk)))
            .from(threads::Entity)
            .and_where(Expr::col(threads::Column::CreatedAt).gte(Expr::current_date()))
            .and_where(Expr::col(threads::Column::Status).ne(Expr::cust("'deleted'::thread_status")))
            .to_owned();

        let new_posts_today = Query::select()
            .expr(Func::count(Expr::col(Asterisk)))
            .from(posts::Entity)
            .and_where(Expr::col(posts::Column::CreatedAt).gte(Expr::current_date()))
            .and_where(Expr::col(posts::Column::IsDeleted).eq(false))
            .to_owned();

        let new_reactions_today = count_today(
            reactions::Entity,
            Expr::col(reactions::Column::CreatedAt).gte(Expr::current_date()),
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
            .and_where(
                Expr::col(daily_stats::Column::Date)
                    .eq(Expr::cust("CURRENT_DATE - INTERVAL '1 day'")),
            )
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

        let dau = count_today(
            users::Entity,
            Expr::col(users::Column::LastSeenAt).gte(Expr::current_date()),
        );

        let mau = count_today(
            users::Entity,
            Expr::col(users::Column::LastSeenAt).gte(Expr::cust("NOW() - INTERVAL '30 days'")),
        );

        // COUNT(*) FILTER (WHERE EXISTS (...)) — no sea-query AST node; the FROM is
        // entity-typed, the aggregate-filter body stays raw (column names inside
        // are not table/column identifiers sea-query can bind).
        let activation_rate_pct = Query::select()
            .expr(Expr::cust(
                "CASE WHEN COUNT(*) = 0 THEN 0 \
                 ELSE ROUND(100.0 * COUNT(*) FILTER (\
                     WHERE EXISTS (\
                         SELECT 1 FROM posts p \
                         WHERE p.author_id = u.id \
                           AND p.created_at >= CURRENT_DATE \
                           AND p.is_deleted = false\
                     )\
                 ) / COUNT(*))::BIGINT END",
            ))
            .from_as(users::Entity, Alias::new("u"))
            .and_where(Expr::cust("u.created_at >= CURRENT_DATE - INTERVAL '1 day'"))
            .to_owned();

        let oldest_pending_report_hours = Query::select()
            .expr(Expr::cust(
                "(EXTRACT(EPOCH FROM (NOW() - MIN(created_at))) / 3600.0)::FLOAT8",
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

    async fn flush_daily_stats(&self) -> Result<(), AppError> {
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
                .expr(Expr::cust("CURRENT_DATE - INTERVAL '1 day'"))
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
            seed_ins.select_from(seed_row).expect("3 columns");
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
            .and_where(
                Expr::col(daily_stats::Column::Date).eq(Expr::cust("CURRENT_DATE - INTERVAL '1 day'")),
            )
            .and_where(Expr::col(daily_stats::Column::CategoryId).is_null())
            .to_owned();

        let dau = scalar(count_today(
            users::Entity,
            Expr::col(users::Column::LastSeenAt).gte(Expr::current_date()),
        ));
        let new_users = scalar(count_today(
            users::Entity,
            Expr::col(users::Column::CreatedAt).gte(Expr::current_date()),
        ));
        let new_threads = scalar(
            Query::select()
                .expr(Func::count(Expr::col(Asterisk)))
                .from(threads::Entity)
                .and_where(Expr::col(threads::Column::CreatedAt).gte(Expr::current_date()))
                .and_where(
                    Expr::col(threads::Column::Status).ne(Expr::cust("'deleted'::thread_status")),
                )
                .to_owned(),
        );
        let new_posts = scalar(
            Query::select()
                .expr(Func::count(Expr::col(Asterisk)))
                .from(posts::Entity)
                .and_where(Expr::col(posts::Column::CreatedAt).gte(Expr::current_date()))
                .and_where(Expr::col(posts::Column::IsDeleted).eq(false))
                .to_owned(),
        );
        let new_reactions = scalar(count_today(
            reactions::Entity,
            Expr::col(reactions::Column::CreatedAt).gte(Expr::current_date()),
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
        let activation = scalar(
            Query::select()
                .expr(Expr::cust(
                    "CASE WHEN COUNT(*) = 0 THEN 0 \
                     ELSE ROUND(100.0 * COUNT(*) FILTER (\
                         WHERE EXISTS (\
                             SELECT 1 FROM posts p \
                             WHERE p.author_id = u.id \
                               AND p.created_at >= CURRENT_DATE \
                               AND p.is_deleted = false\
                         )\
                     ) / COUNT(*))::BIGINT END",
                ))
                .from_as(users::Entity, Alias::new("u"))
                .and_where(Expr::cust("u.created_at >= CURRENT_DATE - INTERVAL '1 day'"))
                .to_owned(),
        );

        // One UNION ALL arm per metric: SELECT CURRENT_DATE, '<metric>', <value>.
        let arm = |metric: &str, value: SimpleExpr| {
            Query::select()
                .expr(Expr::current_date())
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
        ins.select_from(rows).expect("3 columns selected");
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

    async fn backfill_history(&self) -> Result<(), AppError> {
        // INSERT created_at::date, '<metric>', COUNT(*) … ON CONFLICT DO NOTHING.
        let build = |day_select: SelectStatement| -> (String, Values) {
            let mut ins = Query::insert();
            ins.into_table(daily_stats::Entity).columns([
                daily_stats::Column::Date,
                daily_stats::Column::Metric,
                daily_stats::Column::Value,
            ]);
            ins.select_from(day_select).expect("3 columns selected");
            ins.on_conflict(
                OnConflict::columns([daily_stats::Column::Date, daily_stats::Column::Metric])
                    .target_and_where(Expr::col(daily_stats::Column::CategoryId).is_null())
                    .do_nothing()
                    .to_owned(),
            );
            ins.build(PostgresQueryBuilder)
        };

        // CAST("created_at" AS date) — reused in SELECT, WHERE and GROUP BY.
        let users_stmt = {
            let day = Expr::col(users::Column::CreatedAt).cast_as(Alias::new("date"));
            build(
                Query::select()
                    .expr(day.clone())
                    .expr(Expr::val("new_users"))
                    .expr(Func::count(Expr::col(Asterisk)))
                    .from(users::Entity)
                    .and_where(Expr::expr(day.clone()).lt(Expr::current_date()))
                    .add_group_by([day])
                    .to_owned(),
            )
        };
        let threads_stmt = {
            let day = Expr::col(threads::Column::CreatedAt).cast_as(Alias::new("date"));
            build(
                Query::select()
                    .expr(day.clone())
                    .expr(Expr::val("new_threads"))
                    .expr(Func::count(Expr::col(Asterisk)))
                    .from(threads::Entity)
                    .and_where(Expr::expr(day.clone()).lt(Expr::current_date()))
                    .and_where(
                        Expr::col(threads::Column::Status).ne(Expr::cust("'deleted'::thread_status")),
                    )
                    .add_group_by([day])
                    .to_owned(),
            )
        };
        let posts_stmt = {
            let day = Expr::col(posts::Column::CreatedAt).cast_as(Alias::new("date"));
            build(
                Query::select()
                    .expr(day.clone())
                    .expr(Expr::val("new_posts"))
                    .expr(Func::count(Expr::col(Asterisk)))
                    .from(posts::Entity)
                    .and_where(Expr::expr(day.clone()).lt(Expr::current_date()))
                    .and_where(Expr::col(posts::Column::IsDeleted).eq(false))
                    .add_group_by([day])
                    .to_owned(),
            )
        };
        let reactions_stmt = {
            let day = Expr::col(reactions::Column::CreatedAt).cast_as(Alias::new("date"));
            build(
                Query::select()
                    .expr(day.clone())
                    .expr(Expr::val("new_reactions"))
                    .expr(Func::count(Expr::col(Asterisk)))
                    .from(reactions::Entity)
                    .and_where(Expr::expr(day.clone()).lt(Expr::current_date()))
                    .add_group_by([day])
                    .to_owned(),
            )
        };

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
            .from_function(
                Func::cust(Alias::new("generate_series")).args([
                    Expr::cust_with_values(
                        "(CURRENT_DATE - ($1::int * INTERVAL '1 day'))::timestamp",
                        [days],
                    ),
                    Expr::cust("CURRENT_DATE::timestamp"),
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
