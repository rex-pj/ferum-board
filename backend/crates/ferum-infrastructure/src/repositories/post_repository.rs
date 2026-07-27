use async_trait::async_trait;
use chrono::Utc;
use sea_orm::prelude::*;
use sea_orm::*;
use uuid::Uuid;

use crate::entities::{posts, sea_orm_active_enums, threads};
use ferum_application::shared::AppError;
use ferum_domain::models::post::{Post, PostStatus};
use ferum_domain::repositories::post_repository::{NewPost, PostRepository};

use crate::observability::slow_query_threshold_ms;

pub struct PgPostRepository {
    db: DatabaseConnection,
}

impl PgPostRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn entity_status_to_domain(s: &sea_orm_active_enums::PostStatus) -> PostStatus {
    match s {
        sea_orm_active_enums::PostStatus::Pending => PostStatus::Pending,
        sea_orm_active_enums::PostStatus::Published => PostStatus::Published,
    }
}

fn domain_status_to_entity(s: PostStatus) -> sea_orm_active_enums::PostStatus {
    match s {
        PostStatus::Pending => sea_orm_active_enums::PostStatus::Pending,
        PostStatus::Published => sea_orm_active_enums::PostStatus::Published,
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
        status: entity_status_to_domain(&m.status),
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
        thread_slug: None,
        thread_title: None,
    }
}

/// Published posts are always visible; a Pending post is visible only to its
/// own author (so they can see their own post awaiting review). Soft-deleted
/// posts are never filtered here — the caller renders them as tombstones so
/// reply chains keep their context. Shared by `list_by_thread` and
/// `position_in_thread` so the two can never define "visible" differently.
fn visibility_condition(viewer_id: Option<Uuid>) -> Condition {
    let mut condition =
        Condition::any().add(posts::Column::Status.eq(sea_orm_active_enums::PostStatus::Published));
    if let Some(viewer) = viewer_id {
        condition = condition.add(
            Condition::all()
                .add(posts::Column::Status.eq(sea_orm_active_enums::PostStatus::Pending))
                .add(posts::Column::AuthorId.eq(viewer)),
        );
    }
    condition
}

#[async_trait]
impl PostRepository for PgPostRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Post>, AppError> {
        Ok(posts::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .map(entity_to_domain))
    }

    async fn find_many_by_ids(&self, ids: &[Uuid]) -> Result<Vec<Post>, AppError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        Ok(posts::Entity::find()
            .filter(posts::Column::Id.is_in(ids.to_vec()))
            .all(&self.db)
            .await?
            .into_iter()
            .map(entity_to_domain)
            .collect())
    }

    async fn list_by_thread(
        &self,
        thread_id: Uuid,
        viewer_id: Option<Uuid>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Post>, u64), AppError> {
        let t0 = std::time::Instant::now();
        let offset = (page.saturating_sub(1)) * per_page;

        let query = posts::Entity::find()
            .filter(posts::Column::ThreadId.eq(thread_id))
            .filter(visibility_condition(viewer_id))
            .order_by_asc(posts::Column::CreatedAt);

        let (total, rows) = tokio::try_join!(
            query.clone().count(&self.db),
            query.limit(per_page).offset(offset).all(&self.db),
        )?;
        let elapsed = t0.elapsed();
        if elapsed.as_millis() > slow_query_threshold_ms() {
            tracing::warn!(elapsed_ms = elapsed.as_millis(), %thread_id, page, "slow_query: list_by_thread");
        }
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
            status: Set(domain_status_to_entity(cmd.status)),
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
            .col_expr(
                posts::Column::DeletedAt,
                Expr::value(Utc::now().fixed_offset()),
            )
            .col_expr(posts::Column::DeletedById, Expr::value(deleted_by_id))
            .filter(posts::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn content_md_by_thread(&self, thread_id: Uuid) -> Result<Vec<String>, AppError> {
        Ok(posts::Entity::find()
            .select_only()
            .column(posts::Column::ContentMd)
            .filter(posts::Column::ThreadId.eq(thread_id))
            .filter(posts::Column::IsDeleted.eq(false))
            .into_tuple::<String>()
            .all(&self.db)
            .await?)
    }

    async fn set_status(&self, id: Uuid, status: PostStatus) -> Result<(), AppError> {
        let model = posts::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(AppError::NotFound)?;
        let mut active: posts::ActiveModel = model.into();
        active.status = Set(domain_status_to_entity(status));
        active.update(&self.db).await?;
        Ok(())
    }

    async fn list_by_author(
        &self,
        author_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Post>, u64), AppError> {
        use std::collections::HashMap;

        let offset = (page.saturating_sub(1)) * per_page;
        let query = posts::Entity::find()
            .filter(posts::Column::AuthorId.eq(author_id))
            .filter(posts::Column::IsDeleted.eq(false))
            .filter(posts::Column::Status.eq(sea_orm_active_enums::PostStatus::Published))
            .order_by_desc(posts::Column::CreatedAt);

        let (total, rows) = tokio::try_join!(
            query.clone().count(&self.db),
            query.limit(per_page).offset(offset).all(&self.db),
        )?;

        let thread_ids: Vec<Uuid> = rows.iter().map(|r| r.thread_id).collect();
        let thread_map: HashMap<Uuid, (String, String)> = threads::Entity::find()
            .filter(threads::Column::Id.is_in(thread_ids))
            .all(&self.db)
            .await?
            .into_iter()
            .map(|t| (t.id, (t.slug, t.title)))
            .collect();

        let posts = rows
            .into_iter()
            .map(|row| {
                let mut post = entity_to_domain(row.clone());
                if let Some((slug, title)) = thread_map.get(&row.thread_id) {
                    post.thread_slug = Some(slug.clone());
                    post.thread_title = Some(title.clone());
                }
                post
            })
            .collect();

        Ok((posts, total))
    }

    async fn list_pending<'a>(
        &self,
        category_id: Option<Uuid>,
        allowed_category_ids: Option<&'a [Uuid]>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Post>, u64), AppError> {
        use crate::entities::threads;

        let offset = (page.saturating_sub(1)) * per_page;

        let mut query = posts::Entity::find()
            .filter(posts::Column::Status.eq(sea_orm_active_enums::PostStatus::Pending))
            .filter(posts::Column::IsDeleted.eq(false))
            .order_by_asc(posts::Column::CreatedAt);

        // Both filters live on `threads`, so join once if either is present —
        // joining per-filter would duplicate the join and the result rows.
        if category_id.is_some() || allowed_category_ids.is_some() {
            query = query.join(JoinType::InnerJoin, posts::Relation::Threads.def());
        }
        if let Some(cat_id) = category_id {
            query = query.filter(threads::Column::CategoryId.eq(cat_id));
        }
        if let Some(allowed) = allowed_category_ids {
            // An empty allowlist must match nothing (`IN ()` → `1 = 2`), not everything.
            query = query.filter(threads::Column::CategoryId.is_in(allowed.to_vec()));
        }

        let (total, rows) = tokio::try_join!(
            query.clone().count(&self.db),
            query.limit(per_page).offset(offset).all(&self.db),
        )?;
        Ok((rows.into_iter().map(entity_to_domain).collect(), total))
    }

    async fn position_in_thread(
        &self,
        thread_id: Uuid,
        post_id: Uuid,
        viewer_id: Option<Uuid>,
    ) -> Result<Option<u64>, AppError> {
        let Some(target) = posts::Entity::find_by_id(post_id).one(&self.db).await? else {
            return Ok(None);
        };
        if target.thread_id != thread_id {
            return Ok(None);
        }

        // Confirm the target itself is visible to this viewer under the same
        // rule list_by_thread uses, otherwise "position" is meaningless.
        let is_visible = target.status == sea_orm_active_enums::PostStatus::Published
            || (target.status == sea_orm_active_enums::PostStatus::Pending && viewer_id == Some(target.author_id));
        if !is_visible {
            return Ok(None);
        }

        // Count visible posts strictly before the target in list_by_thread's
        // ordering (CreatedAt asc, tie-broken by Id so it matches a stable sort).
        let before_condition = Condition::any()
            .add(posts::Column::CreatedAt.lt(target.created_at))
            .add(
                Condition::all()
                    .add(posts::Column::CreatedAt.eq(target.created_at))
                    .add(posts::Column::Id.lt(target.id)),
            );

        let count = posts::Entity::find()
            .filter(posts::Column::ThreadId.eq(thread_id))
            .filter(visibility_condition(viewer_id))
            .filter(before_condition)
            .count(&self.db)
            .await?;

        Ok(Some(count))
    }
}
