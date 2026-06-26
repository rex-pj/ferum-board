use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::prelude::*;
use sea_orm::sea_query::{Alias, Asterisk, Expr, Func, OnConflict, PostgresQueryBuilder, Query, SimpleExpr};
use sea_orm::*;
use uuid::Uuid;

use crate::entities::{tags, thread_tags, thread_thumbnails, thread_view_dedup, threads};
use ferum_application::shared::AppError;
use ferum_domain::models::thread::{Thread, ThreadStatus};
use ferum_domain::repositories::thread_repository::{
    AdminThreadFilter, NewThread, ThreadFilter, ThreadRepository, ThreadSort, UpdateThread,
};

/// Warn when any hot query exceeds this threshold. Matches the default value of
/// Config::slow_query_ms so both the env var and this constant stay in sync.
const SLOW_QUERY_MS: u128 = 500;

pub struct PgThreadRepository {
    db: DatabaseConnection,
}

impl PgThreadRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

// Used for single-thread queries that don't need enrichment (permission checks, mutations)
fn entity_to_domain(m: threads::Model) -> Thread {
    Thread {
        id: m.id,
        category_id: m.category_id,
        category_slug: String::new(),
        category_name: None,
        author_id: m.author_id,
        author_username: None,
        author_display_name: None,
        author_avatar_url: None,
        title: m.title,
        slug: m.slug,
        status: match m.status {
            threads::ThreadStatus::Open => ThreadStatus::Open,
            threads::ThreadStatus::Locked => ThreadStatus::Locked,
            threads::ThreadStatus::Deleted => ThreadStatus::Deleted,
        },
        is_pinned: m.is_pinned,
        is_solved: m.is_solved,
        best_answer_id: m.best_answer_id,
        view_count: m.view_count,
        reply_count: m.reply_count,
        last_post_at: m.last_post_at.map(|t| t.with_timezone(&Utc)),
        created_at: m.created_at.with_timezone(&Utc),
        updated_at: m.updated_at.map(|t| t.with_timezone(&Utc)),
        deleted_at: m.deleted_at.map(|t| t.with_timezone(&Utc)),
        deleted_by_id: m.deleted_by_id,
        excerpt: None,
        thumbnail_url: None,
        tags: vec![],
    }
}

// Used for list and detail queries: carries joined author + thumbnail + excerpt data
#[derive(Debug, FromQueryResult)]
struct ThreadRow {
    id: Uuid,
    category_id: Uuid,
    author_id: Uuid,
    title: String,
    slug: String,
    status: String,
    is_pinned: bool,
    is_solved: bool,
    best_answer_id: Option<Uuid>,
    view_count: i32,
    reply_count: i32,
    last_post_at: Option<DateTime<FixedOffset>>,
    created_at: DateTime<FixedOffset>,
    author_username: String,
    author_display_name: Option<String>,
    author_avatar_url: Option<String>,
    excerpt: Option<String>,
    thumbnail_url: Option<String>,
}

fn row_to_domain(row: ThreadRow) -> Thread {
    Thread {
        id: row.id,
        category_id: row.category_id,
        category_slug: String::new(), // filled by usecase
        category_name: None,          // filled by usecase
        author_id: row.author_id,
        author_username: Some(row.author_username),
        author_display_name: row.author_display_name,
        author_avatar_url: row.author_avatar_url,
        title: row.title,
        slug: row.slug,
        status: match row.status.as_str() {
            "locked" => ThreadStatus::Locked,
            "deleted" => ThreadStatus::Deleted,
            _ => ThreadStatus::Open,
        },
        is_pinned: row.is_pinned,
        is_solved: row.is_solved,
        best_answer_id: row.best_answer_id,
        view_count: row.view_count,
        reply_count: row.reply_count,
        last_post_at: row.last_post_at.map(|t| t.with_timezone(&Utc)),
        created_at: row.created_at.with_timezone(&Utc),
        updated_at: None,
        deleted_at: None,
        deleted_by_id: None,
        excerpt: row.excerpt,
        thumbnail_url: row.thumbnail_url,
        tags: vec![],
    }
}

fn domain_status_to_entity(s: &ThreadStatus) -> threads::ThreadStatus {
    match s {
        ThreadStatus::Open => threads::ThreadStatus::Open,
        ThreadStatus::Locked => threads::ThreadStatus::Locked,
        ThreadStatus::Deleted => threads::ThreadStatus::Deleted,
    }
}

/// Returns (extra_where_fragment, order_by_clause) for the given ThreadFilter.
/// `extra_where_fragment` is empty or starts with " AND ".
fn sort_clauses(filter: &ThreadFilter) -> (&'static str, &'static str) {
    match filter.sort {
        ThreadSort::Latest => (
            "",
            "ORDER BY t.is_pinned DESC, t.last_post_at DESC NULLS LAST",
        ),
        ThreadSort::Newest => (
            "",
            "ORDER BY t.is_pinned DESC, t.created_at DESC",
        ),
        ThreadSort::Hottest => (
            "",
            "ORDER BY t.is_pinned DESC, t.reply_count DESC, t.view_count DESC",
        ),
        ThreadSort::Unanswered => (
            " AND t.reply_count = 0 AND t.status = 'open'",
            "ORDER BY t.is_pinned DESC, t.created_at DESC",
        ),
        ThreadSort::Solved => (
            " AND t.is_solved = true",
            "ORDER BY t.is_pinned DESC, t.last_post_at DESC NULLS LAST",
        ),
    }
}

/// Returns the extra WHERE condition for count queries from the given ThreadFilter.
/// Mirrors the `extra_where_fragment` half of `sort_clauses()`, but as a typed sea_query
/// expression so the COUNT queries don't need raw SQL string concatenation.
fn sort_condition(filter: &ThreadFilter) -> Option<SimpleExpr> {
    match filter.sort {
        ThreadSort::Unanswered => Some(
            Expr::col(threads::Column::ReplyCount)
                .eq(0i32)
                .and(Expr::col(threads::Column::Status).eq(Expr::cust("'open'::thread_status"))),
        ),
        ThreadSort::Solved => Some(Expr::col(threads::Column::IsSolved).eq(true)),
        _ => None,
    }
}

// Avatar and thumbnail are stored as relative keys; prepend /files/ in SQL (CAS).
// first_image_url is a STORED generated column on posts (migration 006 create_posts), computed once
// at write time — the LATERAL just reads it instead of running a regex per row/request.
// A single LATERAL fetches both excerpt and first_image_url from the same post row,
// halving the index scans on posts compared to two separate LATERAL subqueries.
const ENRICHED_SELECT: &str = r#"
    SELECT
        t.id, t.category_id, t.author_id, t.title, t.slug,
        t.status::text AS status, t.is_pinned, t.is_solved,
        t.best_answer_id, t.view_count, t.reply_count,
        t.last_post_at, t.created_at,
        u.username     AS author_username,
        u.display_name AS author_display_name,
        CASE WHEN ua.file_key IS NOT NULL THEN '/files/' || ua.file_key ELSE NULL END AS author_avatar_url,
        LEFT(p.content_md, 160) AS excerpt,
        COALESCE('/files/' || tt.file_key, p.first_image_url) AS thumbnail_url
    FROM threads t
    JOIN users u ON u.id = t.author_id
    LEFT JOIN user_avatars ua ON ua.user_id = t.author_id
    LEFT JOIN thread_thumbnails tt ON tt.thread_id = t.id
    LEFT JOIN LATERAL (
        SELECT content_md, first_image_url
        FROM posts
        WHERE thread_id = t.id AND is_deleted = false
        ORDER BY created_at ASC LIMIT 1
    ) p ON true
"#;

#[async_trait]
impl ThreadRepository for PgThreadRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Thread>, AppError> {
        Ok(threads::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .map(entity_to_domain))
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<Thread>, AppError> {
        let t0 = std::time::Instant::now();
        let sql = format!("{ENRICHED_SELECT} WHERE t.slug = $1");
        let stmt = Statement::from_sql_and_values(DbBackend::Postgres, &sql, [slug.into()]);
        let result = ThreadRow::find_by_statement(stmt).one(&self.db).await?.map(row_to_domain);
        let elapsed = t0.elapsed();
        if elapsed.as_millis() > SLOW_QUERY_MS {
            tracing::warn!(elapsed_ms = elapsed.as_millis(), slug = %slug, "slow_query: find_by_slug");
        }
        Ok(result)
    }

    async fn list_by_category(
        &self,
        category_id: Uuid,
        filter: &ThreadFilter,
        page: u64,
        per_page: u64,
        cached_total: Option<u64>,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        let t0 = std::time::Instant::now();
        let offset = page.saturating_sub(1) * per_page;
        let (extra_where, order_by) = sort_clauses(filter);

        let data_sql = format!(
            "{ENRICHED_SELECT}
             WHERE t.category_id = $1 AND t.deleted_at IS NULL{extra_where}
             {order_by}
             LIMIT $2 OFFSET $3"
        );
        let data_stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            &data_sql,
            [
                category_id.into(),
                (per_page as i64).into(),
                (offset as i64).into(),
            ],
        );

        // Skip the COUNT round-trip when the use-case layer supplies a cached total.
        let (total, rows) = match cached_total {
            Some(total) => {
                let rows = ThreadRow::find_by_statement(data_stmt).all(&self.db).await?;
                (total, rows)
            }
            None => {
                let mut count_q = Query::select()
                    .expr_as(Func::count(Expr::col(Asterisk)), Alias::new("count"))
                    .from(threads::Entity)
                    .and_where(Expr::col(threads::Column::CategoryId).eq(category_id))
                    .and_where(Expr::col(threads::Column::DeletedAt).is_null())
                    .to_owned();
                if let Some(cond) = sort_condition(filter) {
                    count_q.and_where(cond);
                }
                let (count_sql, count_vals) = count_q.build(PostgresQueryBuilder);
                let count_stmt = Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    count_sql,
                    count_vals,
                );
                let (count_result, rows) = tokio::try_join!(
                    self.db.query_one(count_stmt),
                    ThreadRow::find_by_statement(data_stmt).all(&self.db),
                )?;
                let total = count_result
                    .map(|r| r.try_get::<i64>("", "count").unwrap_or(0) as u64)
                    .unwrap_or(0);
                (total, rows)
            }
        };
        let elapsed = t0.elapsed();
        if elapsed.as_millis() > SLOW_QUERY_MS {
            tracing::warn!(elapsed_ms = elapsed.as_millis(), %category_id, page, "slow_query: list_by_category");
        }
        Ok((rows.into_iter().map(row_to_domain).collect(), total))
    }

    async fn list_feed(
        &self,
        category_ids: &[Uuid],
        filter: &ThreadFilter,
        page: u64,
        per_page: u64,
        cached_total: Option<u64>,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        if category_ids.is_empty() {
            return Ok((vec![], 0));
        }
        let t0 = std::time::Instant::now();
        let offset = page.saturating_sub(1) * per_page;
        let (extra_where, order_by) = sort_clauses(filter);

        // Build $1, $2, ... placeholders for the IN clause
        let placeholders: Vec<String> = (1..=category_ids.len()).map(|i| format!("${i}")).collect();
        let in_clause = placeholders.join(", ");
        let limit_pos = category_ids.len() + 1;
        let offset_pos = category_ids.len() + 2;

        let base_values: Vec<Value> = category_ids.iter().map(|id| (*id).into()).collect();

        let data_sql = format!(
            "{ENRICHED_SELECT}
             WHERE t.category_id IN ({in_clause}) AND t.deleted_at IS NULL{extra_where}
             {order_by}
             LIMIT ${limit_pos} OFFSET ${offset_pos}"
        );
        let mut data_values = base_values;
        data_values.push((per_page as i64).into());
        data_values.push((offset as i64).into());
        let data_stmt =
            Statement::from_sql_and_values(DbBackend::Postgres, &data_sql, data_values);

        // Skip the COUNT round-trip when the use-case layer supplies a cached total.
        let (total, rows) = match cached_total {
            Some(total) => {
                let rows = ThreadRow::find_by_statement(data_stmt).all(&self.db).await?;
                (total, rows)
            }
            None => {
                let mut count_q = Query::select()
                    .expr_as(Func::count(Expr::col(Asterisk)), Alias::new("count"))
                    .from(threads::Entity)
                    .and_where(
                        Expr::col(threads::Column::CategoryId)
                            .is_in(category_ids.to_vec()),
                    )
                    .and_where(Expr::col(threads::Column::DeletedAt).is_null())
                    .to_owned();
                if let Some(cond) = sort_condition(filter) {
                    count_q.and_where(cond);
                }
                let (count_sql, count_vals) = count_q.build(PostgresQueryBuilder);
                let count_stmt = Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    count_sql,
                    count_vals,
                );
                let (count_result, rows) = tokio::try_join!(
                    self.db.query_one(count_stmt),
                    ThreadRow::find_by_statement(data_stmt).all(&self.db),
                )?;
                let total = count_result
                    .map(|r| r.try_get::<i64>("", "count").unwrap_or(0) as u64)
                    .unwrap_or(0);
                (total, rows)
            }
        };
        let elapsed = t0.elapsed();
        if elapsed.as_millis() > SLOW_QUERY_MS {
            tracing::warn!(elapsed_ms = elapsed.as_millis(), page, "slow_query: list_feed");
        }
        Ok((rows.into_iter().map(row_to_domain).collect(), total))
    }

    async fn list_by_author(
        &self,
        author_id: Uuid,
        category_ids: &[Uuid],
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        if category_ids.is_empty() {
            return Ok((vec![], 0));
        }
        let t0 = std::time::Instant::now();
        let offset = page.saturating_sub(1) * per_page;

        let placeholders: Vec<String> = (2..=category_ids.len() + 1)
            .map(|i| format!("${i}"))
            .collect();
        let in_clause = placeholders.join(", ");
        let limit_pos = category_ids.len() + 2;
        let offset_pos = category_ids.len() + 3;

        let sql = format!(
            "{ENRICHED_SELECT}
             WHERE t.author_id = $1 AND t.category_id IN ({in_clause}) AND t.deleted_at IS NULL
             ORDER BY t.created_at DESC
             LIMIT ${limit_pos} OFFSET ${offset_pos}"
        );

        let mut values: Vec<Value> = vec![author_id.into()];
        values.extend(category_ids.iter().map(|id| Value::from(*id)));
        values.push((per_page as i64).into());
        values.push((offset as i64).into());

        let stmt = Statement::from_sql_and_values(DbBackend::Postgres, &sql, values);
        let (total, rows) = tokio::try_join!(
            threads::Entity::find()
                .filter(threads::Column::AuthorId.eq(author_id))
                .filter(threads::Column::CategoryId.is_in(category_ids.to_vec()))
                .filter(threads::Column::DeletedAt.is_null())
                .count(&self.db),
            ThreadRow::find_by_statement(stmt).all(&self.db),
        )?;
        let elapsed = t0.elapsed();
        if elapsed.as_millis() > SLOW_QUERY_MS {
            tracing::warn!(elapsed_ms = elapsed.as_millis(), %author_id, page, "slow_query: list_by_author");
        }
        Ok((rows.into_iter().map(row_to_domain).collect(), total))
    }

    async fn list_by_tag(
        &self,
        tag_slug: &str,
        category_ids: &[Uuid],
        filter: &ThreadFilter,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        let t0 = std::time::Instant::now();
        let offset = page.saturating_sub(1) * per_page;
        let (extra_where, order_by) = sort_clauses(filter);

        let cat_filter = if category_ids.is_empty() {
            String::new()
        } else {
            let placeholders: Vec<String> = (2..=category_ids.len() + 1)
                .map(|i| format!("${i}"))
                .collect();
            format!("AND t.category_id IN ({})", placeholders.join(", "))
        };
        let limit_pos = category_ids.len() + 2;
        let offset_pos = category_ids.len() + 3;

        let mut count_q = Query::select()
            .expr_as(Func::count(Expr::col(Asterisk)), Alias::new("count"))
            .from(threads::Entity)
            .inner_join(
                thread_tags::Entity,
                Expr::col((thread_tags::Entity, thread_tags::Column::ThreadId))
                    .equals((threads::Entity, threads::Column::Id)),
            )
            .inner_join(
                tags::Entity,
                Expr::col((tags::Entity, tags::Column::Id))
                    .equals((thread_tags::Entity, thread_tags::Column::TagId)),
            )
            .and_where(Expr::col((tags::Entity, tags::Column::Slug)).eq(tag_slug))
            .and_where(Expr::col(threads::Column::DeletedAt).is_null())
            .to_owned();
        if !category_ids.is_empty() {
            count_q.and_where(
                Expr::col(threads::Column::CategoryId).is_in(category_ids.to_vec()),
            );
        }
        if let Some(cond) = sort_condition(filter) {
            count_q.and_where(cond);
        }
        let (count_sql, count_vals) = count_q.build(PostgresQueryBuilder);
        let count_result = self
            .db
            .query_one(Statement::from_sql_and_values(
                DbBackend::Postgres,
                count_sql,
                count_vals,
            ))
            .await?;
        let total: i64 = count_result
            .map(|r| r.try_get::<i64>("", "count").unwrap_or(0))
            .unwrap_or(0);

        let sql = format!(
            "{ENRICHED_SELECT}
             JOIN thread_tags ttg ON ttg.thread_id = t.id
             JOIN tags tg ON tg.id = ttg.tag_id
             WHERE tg.slug = $1 AND t.deleted_at IS NULL {cat_filter}{extra_where}
             {order_by}
             LIMIT ${limit_pos} OFFSET ${offset_pos}"
        );

        let mut values: Vec<Value> = vec![tag_slug.into()];
        values.extend(category_ids.iter().map(|id| Value::from(*id)));
        values.push((per_page as i64).into());
        values.push((offset as i64).into());

        let stmt = Statement::from_sql_and_values(DbBackend::Postgres, &sql, values);
        let rows = ThreadRow::find_by_statement(stmt).all(&self.db).await?;
        let elapsed = t0.elapsed();
        if elapsed.as_millis() > SLOW_QUERY_MS {
            tracing::warn!(elapsed_ms = elapsed.as_millis(), tag_slug, page, "slow_query: list_by_tag");
        }
        Ok((rows.into_iter().map(row_to_domain).collect(), total as u64))
    }

    async fn create(&self, cmd: NewThread) -> Result<Thread, AppError> {
        let model = threads::ActiveModel {
            id: Set(cmd.id),
            category_id: Set(cmd.category_id),
            author_id: Set(cmd.author_id),
            title: Set(cmd.title),
            slug: Set(cmd.slug),
            ..Default::default()
        };
        let inserted = model.insert(&self.db).await?;
        Ok(entity_to_domain(inserted))
    }

    async fn update(&self, id: Uuid, patch: UpdateThread) -> Result<Thread, AppError> {
        let model = threads::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(AppError::NotFound)?;

        let mut active: threads::ActiveModel = model.into();

        if let Some(v) = patch.title {
            active.title = Set(v);
        }
        if let Some(v) = patch.status {
            active.status = Set(domain_status_to_entity(&v));
        }
        if let Some(v) = patch.is_pinned {
            active.is_pinned = Set(v);
        }
        if let Some(v) = patch.is_solved {
            active.is_solved = Set(v);
        }
        if let Some(v) = patch.best_answer_id {
            active.best_answer_id = Set(v);
        }
        if let Some(v) = patch.category_id {
            active.category_id = Set(v);
        }
        if let Some(v) = patch.deleted_by_id {
            active.deleted_by_id = Set(Some(v));
        }
        if let Some(v) = patch.deleted_at {
            active.deleted_at = Set(Some(v.fixed_offset()));
        }

        let updated = active.update(&self.db).await?;
        Ok(entity_to_domain(updated))
    }

    async fn add_view_count(&self, id: Uuid, delta: i32) -> Result<(), AppError> {
        threads::Entity::update_many()
            .col_expr(
                threads::Column::ViewCount,
                Expr::col(threads::Column::ViewCount).add(delta),
            )
            .filter(threads::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn try_record_view(
        &self,
        thread_id: Uuid,
        viewer_key: &str,
        viewer_type: &str,
    ) -> Result<bool, AppError> {
        let now = Utc::now().fixed_offset();
        let today = Utc::now().date_naive();

        // Bước 1: INSERT mới (viewer lần đầu tiên).
        let insert_stmt = thread_view_dedup::Entity::insert(thread_view_dedup::ActiveModel {
            thread_id: Set(thread_id),
            viewer_key: Set(viewer_key.to_owned()),
            viewer_type: Set(viewer_type.to_owned()),
            last_viewed_date: Set(today),
            total_views: Set(1),
            first_viewed_at: Set(now),
            last_viewed_at: Set(now),
        })
        .on_conflict(
            OnConflict::columns([
                thread_view_dedup::Column::ThreadId,
                thread_view_dedup::Column::ViewerKey,
            ])
            .do_nothing()
            .to_owned(),
        )
        .build(DbBackend::Postgres);

        let inserted = self.db.execute(insert_stmt).await?;

        if inserted.rows_affected() > 0 {
            return Ok(true); // viewer hoàn toàn mới
        }

        // Bước 2: viewer đã tồn tại — chỉ tính view nếu ngày hôm nay chưa được ghi.
        let updated = thread_view_dedup::Entity::update_many()
            .col_expr(thread_view_dedup::Column::LastViewedDate, Expr::value(today))
            .col_expr(thread_view_dedup::Column::LastViewedAt, Expr::value(now))
            .col_expr(
                thread_view_dedup::Column::TotalViews,
                Expr::col(thread_view_dedup::Column::TotalViews).add(1i32),
            )
            .filter(thread_view_dedup::Column::ThreadId.eq(thread_id))
            .filter(thread_view_dedup::Column::ViewerKey.eq(viewer_key))
            .filter(thread_view_dedup::Column::LastViewedDate.lt(today))
            .exec(&self.db)
            .await?;

        Ok(updated.rows_affected > 0)
    }

    async fn update_reply_stats(
        &self,
        id: Uuid,
        reply_count_delta: i32,
        last_post_at: DateTime<Utc>,
    ) -> Result<(), AppError> {
        threads::Entity::update_many()
            .col_expr(
                threads::Column::ReplyCount,
                Expr::col(threads::Column::ReplyCount).add(reply_count_delta),
            )
            .col_expr(
                threads::Column::LastPostAt,
                Expr::value(last_post_at.fixed_offset()),
            )
            .filter(threads::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn set_thumbnail(&self, thread_id: Uuid, file_key: String) -> Result<(), AppError> {
        let model = thread_thumbnails::ActiveModel {
            thread_id: Set(thread_id),
            file_key: Set(file_key),
            updated_at: Set(chrono::Utc::now().into()),
        };
        thread_thumbnails::Entity::insert(model)
            .on_conflict(
                OnConflict::column(thread_thumbnails::Column::ThreadId)
                    .update_columns([
                        thread_thumbnails::Column::FileKey,
                        thread_thumbnails::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn remove_thumbnail(&self, thread_id: Uuid) -> Result<(), AppError> {
        thread_thumbnails::Entity::delete_by_id(thread_id)
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn find_thumbnail_key(&self, thread_id: Uuid) -> Result<Option<String>, AppError> {
        Ok(thread_thumbnails::Entity::find_by_id(thread_id)
            .one(&self.db)
            .await?
            .map(|t| t.file_key))
    }

    async fn list_admin_threads(
        &self,
        filter: &AdminThreadFilter,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        let offset = page.saturating_sub(1) * per_page;

        // Build dynamic WHERE clauses and bind values. Placeholders ($N) are
        // assigned sequentially as conditions are appended, so the bind order in
        // `values` always matches the SQL.
        let mut where_parts: Vec<String> = Vec::new();
        let mut values: Vec<Value> = Vec::new();
        let mut pos: usize = 1;

        if let Some(q) = filter.search.as_deref().filter(|s| !s.is_empty()) {
            where_parts.push(format!("to_tsvector('simple', t.title) @@ plainto_tsquery('simple', ${pos})"));
            values.push(q.into());
            pos += 1;
        }

        match filter.status.as_deref() {
            Some("deleted") => {
                where_parts.push("t.deleted_at IS NOT NULL".to_string());
            }
            Some("locked") => {
                where_parts.push("t.deleted_at IS NULL".to_string());
                where_parts.push("t.status = 'locked'".to_string());
            }
            Some("open") => {
                where_parts.push("t.deleted_at IS NULL".to_string());
                where_parts.push("t.status = 'open'".to_string());
            }
            _ => {
                // Default: show all non-deleted threads.
                where_parts.push("t.deleted_at IS NULL".to_string());
            }
        }

        if let Some(category_id) = filter.category_id {
            where_parts.push(format!("t.category_id = ${pos}"));
            values.push(category_id.into());
            pos += 1;
        }
        if let Some(author_id) = filter.author_id {
            where_parts.push(format!("t.author_id = ${pos}"));
            values.push(author_id.into());
            pos += 1;
        }
        if let Some(from) = filter.created_from {
            where_parts.push(format!("t.created_at >= ${pos}"));
            values.push(from.into());
            pos += 1;
        }
        if let Some(to) = filter.created_to {
            where_parts.push(format!("t.created_at <= ${pos}"));
            values.push(to.into());
            pos += 1;
        }

        let where_clause = if where_parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_parts.join(" AND "))
        };

        let order_by = match filter.sort {
            ThreadSort::Newest => "ORDER BY t.is_pinned DESC, t.created_at DESC",
            ThreadSort::Hottest => "ORDER BY t.is_pinned DESC, t.reply_count DESC, t.view_count DESC",
            _ => "ORDER BY t.is_pinned DESC, t.last_post_at DESC NULLS LAST",
        };

        let limit_pos = pos;
        let offset_pos = pos + 1;

        let count_sql = format!("SELECT COUNT(*) FROM threads t {where_clause}");
        let count_stmt =
            Statement::from_sql_and_values(DbBackend::Postgres, &count_sql, values.clone());

        let mut data_values = values;
        data_values.push((per_page as i64).into());
        data_values.push((offset as i64).into());
        let data_sql = format!(
            "{ENRICHED_SELECT}
             {where_clause}
             {order_by}
             LIMIT ${limit_pos} OFFSET ${offset_pos}"
        );
        let data_stmt =
            Statement::from_sql_and_values(DbBackend::Postgres, &data_sql, data_values);

        // Run COUNT and data fetch concurrently — each uses a separate pool connection
        // so neither blocks the other, halving the wall-clock time for this query pair.
        let (count_result, rows) = tokio::try_join!(
            self.db.query_one(count_stmt),
            ThreadRow::find_by_statement(data_stmt).all(&self.db),
        )?;

        let total = count_result
            .map(|r| r.try_get::<i64>("", "count").unwrap_or(0) as u64)
            .unwrap_or(0);

        Ok((rows.into_iter().map(row_to_domain).collect(), total))
    }

    async fn list_recent_by_categories(
        &self,
        category_ids: &[Uuid],
        limit_per_category: u64,
    ) -> Result<Vec<Thread>, AppError> {
        if category_ids.is_empty() {
            return Ok(vec![]);
        }

        let placeholders: Vec<String> = (1..=category_ids.len()).map(|i| format!("${i}")).collect();
        let in_clause = placeholders.join(", ");
        let limit_pos = category_ids.len() + 1;

        // Window function ranks threads within each category by pinned-first then
        // last_post_at desc. The outer query keeps only the top N per partition.
        let sql = format!(
            r#"SELECT
                   id, category_id, author_id, title, slug,
                   status, is_pinned, is_solved, best_answer_id, view_count, reply_count,
                   last_post_at, created_at,
                   author_username, author_display_name, author_avatar_url,
                   excerpt, thumbnail_url
               FROM (
                   SELECT
                       t.id, t.category_id, t.author_id, t.title, t.slug,
                       t.status::text AS status, t.is_pinned, t.is_solved,
                       t.best_answer_id, t.view_count, t.reply_count,
                       t.last_post_at, t.created_at,
                       u.username     AS author_username,
                       u.display_name AS author_display_name,
                       CASE WHEN ua.file_key IS NOT NULL
                            THEN '/files/' || ua.file_key ELSE NULL END AS author_avatar_url,
                       LEFT(p.content_md, 160) AS excerpt,
                       COALESCE('/files/' || tt.file_key, p.first_image_url) AS thumbnail_url,
                       ROW_NUMBER() OVER (
                           PARTITION BY t.category_id
                           ORDER BY t.is_pinned DESC, t.last_post_at DESC NULLS LAST
                       ) AS rn
                   FROM threads t
                   JOIN users u ON u.id = t.author_id
                   LEFT JOIN user_avatars ua ON ua.user_id = t.author_id
                   LEFT JOIN thread_thumbnails tt ON tt.thread_id = t.id
                   LEFT JOIN LATERAL (
                       SELECT content_md, first_image_url
                       FROM posts
                       WHERE thread_id = t.id AND is_deleted = false
                       ORDER BY created_at ASC LIMIT 1
                   ) p ON true
                   WHERE t.category_id IN ({in_clause}) AND t.deleted_at IS NULL
               ) ranked
               WHERE rn <= ${limit_pos}
               ORDER BY category_id, is_pinned DESC, last_post_at DESC NULLS LAST"#
        );

        let mut values: Vec<Value> = category_ids.iter().map(|id| (*id).into()).collect();
        values.push((limit_per_category as i64).into());

        let stmt = Statement::from_sql_and_values(DbBackend::Postgres, &sql, values);
        let rows = ThreadRow::find_by_statement(stmt).all(&self.db).await?;
        Ok(rows.into_iter().map(row_to_domain).collect())
    }
}
