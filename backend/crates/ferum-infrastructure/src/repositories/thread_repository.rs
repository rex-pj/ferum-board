use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::prelude::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::*;
use uuid::Uuid;

use crate::entities::{thread_thumbnails, threads};
use ferum_application::shared::AppError;
use ferum_domain::models::thread::{Thread, ThreadStatus};
use ferum_domain::repositories::thread_repository::{NewThread, ThreadRepository, UpdateThread};

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
    }
}

fn domain_status_to_entity(s: &ThreadStatus) -> threads::ThreadStatus {
    match s {
        ThreadStatus::Open => threads::ThreadStatus::Open,
        ThreadStatus::Locked => threads::ThreadStatus::Locked,
        ThreadStatus::Deleted => threads::ThreadStatus::Deleted,
    }
}

// Avatar and thumbnail are stored as relative keys; prepend /files/ in SQL (CAS).
// first_image_url from posts HTML is already a full URL and is used as-is.
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
        COALESCE('/files/' || tt.file_key, img.first_image_url) AS thumbnail_url
    FROM threads t
    JOIN users u ON u.id = t.author_id
    LEFT JOIN user_avatars ua ON ua.user_id = t.author_id
    LEFT JOIN thread_thumbnails tt ON tt.thread_id = t.id
    LEFT JOIN LATERAL (
        SELECT content_md FROM posts
        WHERE thread_id = t.id AND is_deleted = false
        ORDER BY created_at ASC LIMIT 1
    ) p ON true
    LEFT JOIN LATERAL (
        SELECT (regexp_match(content_html, '<img[^>]+src="([^"]+)"'))[1] AS first_image_url
        FROM posts
        WHERE thread_id = t.id AND is_deleted = false
        ORDER BY created_at ASC LIMIT 1
    ) img ON true
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
        let sql = format!("{ENRICHED_SELECT} WHERE t.slug = $1");
        let stmt = Statement::from_sql_and_values(DbBackend::Postgres, &sql, [slug.into()]);
        Ok(ThreadRow::find_by_statement(stmt)
            .one(&self.db)
            .await?
            .map(row_to_domain))
    }

    async fn list_by_category(
        &self,
        category_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        let offset = page.saturating_sub(1) * per_page;

        let total = threads::Entity::find()
            .filter(threads::Column::CategoryId.eq(category_id))
            .filter(threads::Column::DeletedAt.is_null())
            .count(&self.db)
            .await?;

        let sql = format!(
            "{ENRICHED_SELECT}
             WHERE t.category_id = $1 AND t.deleted_at IS NULL
             ORDER BY t.is_pinned DESC, t.last_post_at DESC NULLS LAST
             LIMIT $2 OFFSET $3"
        );
        let stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            &sql,
            [
                category_id.into(),
                (per_page as i64).into(),
                (offset as i64).into(),
            ],
        );
        let rows = ThreadRow::find_by_statement(stmt).all(&self.db).await?;
        Ok((rows.into_iter().map(row_to_domain).collect(), total))
    }

    async fn list_feed(
        &self,
        category_ids: &[Uuid],
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Thread>, u64), AppError> {
        if category_ids.is_empty() {
            return Ok((vec![], 0));
        }
        let offset = page.saturating_sub(1) * per_page;

        let total = threads::Entity::find()
            .filter(threads::Column::CategoryId.is_in(category_ids.to_vec()))
            .filter(threads::Column::DeletedAt.is_null())
            .count(&self.db)
            .await?;

        // Build $1, $2, ... placeholders for the IN clause
        let placeholders: Vec<String> = (1..=category_ids.len()).map(|i| format!("${i}")).collect();
        let in_clause = placeholders.join(", ");
        let limit_pos = category_ids.len() + 1;
        let offset_pos = category_ids.len() + 2;

        let sql = format!(
            "{ENRICHED_SELECT}
             WHERE t.category_id IN ({in_clause}) AND t.deleted_at IS NULL
             ORDER BY t.is_pinned DESC, t.last_post_at DESC NULLS LAST
             LIMIT ${limit_pos} OFFSET ${offset_pos}"
        );

        let mut values: Vec<Value> = category_ids.iter().map(|id| (*id).into()).collect();
        values.push((per_page as i64).into());
        values.push((offset as i64).into());

        let stmt = Statement::from_sql_and_values(DbBackend::Postgres, &sql, values);
        let rows = ThreadRow::find_by_statement(stmt).all(&self.db).await?;
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
        let offset = page.saturating_sub(1) * per_page;

        let total = threads::Entity::find()
            .filter(threads::Column::AuthorId.eq(author_id))
            .filter(threads::Column::CategoryId.is_in(category_ids.to_vec()))
            .filter(threads::Column::DeletedAt.is_null())
            .count(&self.db)
            .await?;

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
        let rows = ThreadRow::find_by_statement(stmt).all(&self.db).await?;
        Ok((rows.into_iter().map(row_to_domain).collect(), total))
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

    async fn increment_view_count(&self, id: Uuid) -> Result<(), AppError> {
        threads::Entity::update_many()
            .col_expr(
                threads::Column::ViewCount,
                Expr::col(threads::Column::ViewCount).add(1),
            )
            .filter(threads::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(())
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
            ..Default::default()
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
                       COALESCE('/files/' || tt.file_key, img.first_image_url) AS thumbnail_url,
                       ROW_NUMBER() OVER (
                           PARTITION BY t.category_id
                           ORDER BY t.is_pinned DESC, t.last_post_at DESC NULLS LAST
                       ) AS rn
                   FROM threads t
                   JOIN users u ON u.id = t.author_id
                   LEFT JOIN user_avatars ua ON ua.user_id = t.author_id
                   LEFT JOIN thread_thumbnails tt ON tt.thread_id = t.id
                   LEFT JOIN LATERAL (
                       SELECT content_md FROM posts
                       WHERE thread_id = t.id AND is_deleted = false
                       ORDER BY created_at ASC LIMIT 1
                   ) p ON true
                   LEFT JOIN LATERAL (
                       SELECT (regexp_match(content_html, '<img[^>]+src="([^"]+)"'))[1]
                              AS first_image_url
                       FROM posts
                       WHERE thread_id = t.id AND is_deleted = false
                       ORDER BY created_at ASC LIMIT 1
                   ) img ON true
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
