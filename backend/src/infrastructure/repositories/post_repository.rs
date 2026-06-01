use async_trait::async_trait;
use chrono::Utc;
use sea_orm::prelude::*;
use sea_orm::*;
use uuid::Uuid;

use crate::application::shared::AppError;
use crate::domain::models::post::Post;
use crate::domain::repositories::post_repository::{NewPost, PostRepository};
use crate::entities::posts;

pub struct PgPostRepository {
    db: DatabaseConnection,
}

impl PgPostRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn entity_to_domain(m: posts::Model) -> Post {
    Post {
        id: m.id,
        thread_id: m.thread_id,
        author_id: m.author_id,
        parent_id: m.parent_id,
        content_md: m.content_md,
        content_html: m.content_html,
        is_deleted: m.is_deleted,
        deleted_at: m.deleted_at.map(|t| t.with_timezone(&Utc)),
        deleted_by_id: m.deleted_by_id,
        edited_at: m.edited_at.map(|t| t.with_timezone(&Utc)),
        edited_by_id: m.edited_by_id,
        edit_count: m.edit_count,
        created_at: m.created_at.with_timezone(&Utc),
        author_username: None,
        author_display_name: None,
        author_avatar_url: None,
        author_role: None,
        reactions: vec![],
        my_reactions: vec![],
    }
}

#[async_trait]
impl PostRepository for PgPostRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Post>, AppError> {
        Ok(posts::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .map(entity_to_domain))
    }

    async fn list_by_thread(
        &self,
        thread_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Post>, u64), AppError> {
        let offset = (page.saturating_sub(1)) * per_page;
        let query = posts::Entity::find()
            .filter(posts::Column::ThreadId.eq(thread_id))
            .filter(posts::Column::IsDeleted.eq(false))
            .order_by_asc(posts::Column::CreatedAt);

        let total = query.clone().count(&self.db).await?;
        let rows = query.limit(per_page).offset(offset).all(&self.db).await?;
        Ok((rows.into_iter().map(entity_to_domain).collect(), total))
    }

    async fn create(&self, cmd: NewPost) -> Result<Post, AppError> {
        let model = posts::ActiveModel {
            id: Set(Uuid::new_v4()),
            thread_id: Set(cmd.thread_id),
            author_id: Set(cmd.author_id),
            parent_id: Set(cmd.parent_id),
            content_md: Set(cmd.content_md),
            content_html: Set(cmd.content_html),
            ..Default::default()
        };
        let inserted = model.insert(&self.db).await?;
        Ok(entity_to_domain(inserted))
    }

    async fn update_content(
        &self,
        id: Uuid,
        content_md: String,
        content_html: String,
        edited_by_id: Uuid,
    ) -> Result<Post, AppError> {
        let model = posts::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(AppError::NotFound)?;

        let mut active: posts::ActiveModel = model.into();
        active.content_md = Set(content_md);
        active.content_html = Set(content_html);
        active.edited_at = Set(Some(Utc::now().fixed_offset()));
        active.edited_by_id = Set(Some(edited_by_id));
        active.edit_count = Set(active.edit_count.clone().unwrap() + 1);

        let updated = active.update(&self.db).await?;
        Ok(entity_to_domain(updated))
    }

    async fn soft_delete(&self, id: Uuid, deleted_by_id: Uuid) -> Result<(), AppError> {
        posts::Entity::update_many()
            .col_expr(posts::Column::IsDeleted, Expr::value(true))
            .col_expr(posts::Column::DeletedAt, Expr::value(Utc::now().fixed_offset()))
            .col_expr(posts::Column::DeletedById, Expr::value(deleted_by_id))
            .filter(posts::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(())
    }
}
