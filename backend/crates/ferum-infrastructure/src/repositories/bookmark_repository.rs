use async_trait::async_trait;
use chrono::Utc;
use sea_orm::prelude::*;
use sea_orm::*;
use std::collections::HashMap;
use uuid::Uuid;

use crate::entities::{bookmarks, threads};
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

fn thread_to_domain(m: threads::Model) -> Thread {
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
        updated_at: None,
        deleted_at: None,
        deleted_by_id: None,
        thumbnail_url: None,
        excerpt: None,
        tags: vec![],
    }
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
        let query = bookmarks::Entity::find()
            .filter(bookmarks::Column::UserId.eq(user_id))
            .order_by_desc(bookmarks::Column::CreatedAt);

        let total = query.clone().count(&self.db).await?;
        let bookmark_rows = query.limit(per_page).offset(offset).all(&self.db).await?;

        if bookmark_rows.is_empty() {
            return Ok((vec![], total));
        }

        let thread_ids: Vec<Uuid> = bookmark_rows.iter().map(|b| b.thread_id).collect();
        let thread_map: HashMap<Uuid, Thread> = threads::Entity::find()
            .filter(threads::Column::Id.is_in(thread_ids))
            .all(&self.db)
            .await?
            .into_iter()
            .map(|t| (t.id, thread_to_domain(t)))
            .collect();

        let pairs = bookmark_rows
            .into_iter()
            .filter_map(|b| {
                let thread_id = b.thread_id;
                thread_map
                    .get(&thread_id)
                    .cloned()
                    .map(|t| (bookmark_to_domain(b), t))
            })
            .collect();

        Ok((pairs, total))
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
