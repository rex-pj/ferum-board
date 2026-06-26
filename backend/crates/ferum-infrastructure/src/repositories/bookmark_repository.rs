use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::prelude::*;
use sea_orm::*;
use uuid::Uuid;

use crate::entities::bookmarks;
use ferum_application::shared::AppError;
use ferum_domain::models::bookmark::Bookmark;
use ferum_domain::models::thread::{Thread, ThreadStatus};
use ferum_domain::repositories::bookmark_repository::BookmarkRepository;

pub struct PgBookmarkRepository {
    db: DatabaseConnection,
}

impl PgBookmarkRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn bookmark_to_domain(m: bookmarks::Model) -> Bookmark {
    Bookmark {
        id: m.id,
        user_id: m.user_id,
        thread_id: m.thread_id,
        created_at: m.created_at.with_timezone(&Utc),
    }
}

#[derive(Debug, FromQueryResult)]
struct BookmarkThreadRow {
    bookmark_id: Uuid,
    bookmark_user_id: Uuid,
    bookmark_thread_id: Uuid,
    bookmark_created_at: DateTime<FixedOffset>,
    id: Uuid,
    category_id: Uuid,
    category_slug: String,
    category_name: String,
    author_id: Uuid,
    author_username: String,
    author_display_name: Option<String>,
    author_avatar_url: Option<String>,
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
    thumbnail_url: Option<String>,
}

fn row_to_pair(row: BookmarkThreadRow) -> (Bookmark, Thread) {
    let bookmark = Bookmark {
        id: row.bookmark_id,
        user_id: row.bookmark_user_id,
        thread_id: row.bookmark_thread_id,
        created_at: row.bookmark_created_at.with_timezone(&Utc),
    };
    let thread = Thread {
        id: row.id,
        category_id: row.category_id,
        category_slug: row.category_slug,
        category_name: Some(row.category_name),
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
        thumbnail_url: row.thumbnail_url,
        excerpt: None,
        tags: vec![],
    };
    (bookmark, thread)
}

#[async_trait]
impl BookmarkRepository for PgBookmarkRepository {
    async fn find(&self, user_id: Uuid, thread_id: Uuid) -> Result<Option<Bookmark>, AppError> {
        Ok(bookmarks::Entity::find()
            .filter(bookmarks::Column::UserId.eq(user_id))
            .filter(bookmarks::Column::ThreadId.eq(thread_id))
            .one(&self.db)
            .await?
            .map(bookmark_to_domain))
    }

    async fn list_for_user(
        &self,
        user_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<(Bookmark, Thread)>, u64), AppError> {
        let offset = page.saturating_sub(1) * per_page;

        let data_stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"
            SELECT
                b.id            AS bookmark_id,
                b.user_id       AS bookmark_user_id,
                b.thread_id     AS bookmark_thread_id,
                b.created_at    AS bookmark_created_at,
                t.id,
                t.category_id,
                c.slug          AS category_slug,
                c.name          AS category_name,
                t.author_id,
                u.username      AS author_username,
                u.display_name  AS author_display_name,
                CASE WHEN ua.file_key IS NOT NULL
                     THEN '/files/' || ua.file_key ELSE NULL END AS author_avatar_url,
                t.title,
                t.slug,
                t.status::text  AS status,
                t.is_pinned,
                t.is_solved,
                t.best_answer_id,
                t.view_count,
                t.reply_count,
                t.last_post_at,
                t.created_at,
                CASE WHEN tt.file_key IS NOT NULL
                     THEN '/files/' || tt.file_key ELSE NULL END AS thumbnail_url
            FROM bookmarks b
            INNER JOIN threads t  ON t.id  = b.thread_id
            INNER JOIN categories c ON c.id = t.category_id
            INNER JOIN users u    ON u.id  = t.author_id
            LEFT  JOIN user_avatars ua     ON ua.user_id  = t.author_id
            LEFT  JOIN thread_thumbnails tt ON tt.thread_id = t.id
            WHERE b.user_id = $1
              AND t.status != 'deleted'::thread_status
            ORDER BY b.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            [
                user_id.into(),
                (per_page as i64).into(),
                (offset as i64).into(),
            ],
        );

        let (total, rows) = tokio::try_join!(
            bookmarks::Entity::find()
                .filter(bookmarks::Column::UserId.eq(user_id))
                .count(&self.db),
            BookmarkThreadRow::find_by_statement(data_stmt).all(&self.db),
        )?;

        Ok((rows.into_iter().map(row_to_pair).collect(), total))
    }

    async fn add(&self, user_id: Uuid, thread_id: Uuid) -> Result<Bookmark, AppError> {
        let model = bookmarks::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user_id),
            thread_id: Set(thread_id),
            ..Default::default()
        };
        Ok(bookmark_to_domain(model.insert(&self.db).await?))
    }

    async fn remove(&self, user_id: Uuid, thread_id: Uuid) -> Result<(), AppError> {
        bookmarks::Entity::delete_many()
            .filter(bookmarks::Column::UserId.eq(user_id))
            .filter(bookmarks::Column::ThreadId.eq(thread_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }
}
