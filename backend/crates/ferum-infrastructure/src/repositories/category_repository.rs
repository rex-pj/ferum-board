use async_trait::async_trait;
use sea_orm::sea_query::{Alias, CaseStatement, Expr, Func, JoinType, PostgresQueryBuilder, Query};
use sea_orm::*;
use uuid::Uuid;

use crate::entities::{categories, threads};
use ferum_application::shared::AppError;
use ferum_domain::models::category::{Category, PostPolicy, ViewPolicy};
use ferum_domain::repositories::category_repository::{
    CategoryRepository, NewCategory, UpdateCategory,
};

#[derive(Debug, FromQueryResult)]
struct CategoryCount {
    id: Uuid,
    thread_count: i64,
}

pub struct PgCategoryRepository {
    db: DatabaseConnection,
}

impl PgCategoryRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn entity_to_domain(m: categories::Model) -> Category {
    Category {
        id: m.id,
        parent_id: m.parent_id,
        slug: m.slug,
        name: m.name,
        description: m.description,
        position: m.position,
        view_policy: match m.view_policy {
            categories::ViewPolicy::Public => ViewPolicy::Public,
            categories::ViewPolicy::MembersOnly => ViewPolicy::MembersOnly,
            categories::ViewPolicy::StaffOnly => ViewPolicy::StaffOnly,
        },
        post_policy: match m.post_policy {
            categories::PostPolicy::Members => PostPolicy::Members,
            categories::PostPolicy::Trusted => PostPolicy::Trusted,
            categories::PostPolicy::StaffOnly => PostPolicy::StaffOnly,
            categories::PostPolicy::Closed => PostPolicy::Closed,
            categories::PostPolicy::Moderated => PostPolicy::Moderated,
        },
        color: m.color,
        created_at: m.created_at.with_timezone(&chrono::Utc),
        updated_at: m.updated_at.map(|t| t.with_timezone(&chrono::Utc)),
        created_by_id: m.created_by_id,
        updated_by_id: m.updated_by_id,
    }
}

fn view_policy_to_entity(p: &ViewPolicy) -> categories::ViewPolicy {
    match p {
        ViewPolicy::Public => categories::ViewPolicy::Public,
        ViewPolicy::MembersOnly => categories::ViewPolicy::MembersOnly,
        ViewPolicy::StaffOnly => categories::ViewPolicy::StaffOnly,
    }
}

fn post_policy_to_entity(p: &PostPolicy) -> categories::PostPolicy {
    match p {
        PostPolicy::Members => categories::PostPolicy::Members,
        PostPolicy::Trusted => categories::PostPolicy::Trusted,
        PostPolicy::StaffOnly => categories::PostPolicy::StaffOnly,
        PostPolicy::Closed => categories::PostPolicy::Closed,
        PostPolicy::Moderated => categories::PostPolicy::Moderated,
    }
}

#[async_trait]
impl CategoryRepository for PgCategoryRepository {
    async fn list_all(&self) -> Result<Vec<Category>, AppError> {
        let rows = categories::Entity::find()
            .order_by_asc(categories::Column::Position)
            .all(&self.db)
            .await?;
        Ok(rows.into_iter().map(entity_to_domain).collect())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Category>, AppError> {
        Ok(categories::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .map(entity_to_domain))
    }

    async fn find_by_slug<'a>(&self, slug: &'a str) -> Result<Option<Category>, AppError> {
        Ok(categories::Entity::find()
            .filter(categories::Column::Slug.eq(slug))
            .one(&self.db)
            .await?
            .map(entity_to_domain))
    }

    async fn create(&self, cmd: NewCategory) -> Result<Category, AppError> {
        let model = categories::ActiveModel {
            id: Set(Uuid::new_v4()),
            slug: Set(cmd.slug),
            name: Set(cmd.name),
            description: Set(cmd.description),
            parent_id: Set(cmd.parent_id),
            position: Set(cmd.position),
            view_policy: Set(view_policy_to_entity(&cmd.view_policy)),
            post_policy: Set(post_policy_to_entity(&cmd.post_policy)),
            color: Set(cmd.color),
            created_by_id: Set(cmd.created_by_id),
            ..Default::default()
        };
        let inserted = model.insert(&self.db).await?;
        Ok(entity_to_domain(inserted))
    }

    async fn update(&self, id: Uuid, patch: UpdateCategory) -> Result<Category, AppError> {
        let model = categories::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(AppError::NotFound)?;

        let mut active: categories::ActiveModel = model.into();

        if let Some(v) = patch.name {
            active.name = Set(v);
        }
        if let Some(v) = patch.slug {
            active.slug = Set(v);
        }
        if let Some(v) = patch.description {
            active.description = Set(v);
        }
        if let Some(v) = patch.parent_id {
            active.parent_id = Set(v);
        }
        if let Some(v) = patch.position {
            active.position = Set(v);
        }
        if let Some(v) = patch.view_policy {
            active.view_policy = Set(view_policy_to_entity(&v));
        }
        if let Some(v) = patch.post_policy {
            active.post_policy = Set(post_policy_to_entity(&v));
        }
        if let Some(v) = patch.color {
            active.color = Set(v);
        }
        if let Some(v) = patch.updated_by_id {
            active.updated_by_id = Set(Some(v));
        }

        let updated = active.update(&self.db).await?;
        Ok(entity_to_domain(updated))
    }

    async fn delete(&self, id: Uuid) -> Result<(), AppError> {
        categories::Entity::delete_by_id(id).exec(&self.db).await?;
        Ok(())
    }

    async fn has_children(&self, id: Uuid) -> Result<bool, AppError> {
        let count = categories::Entity::find()
            .filter(categories::Column::ParentId.eq(id))
            .count(&self.db)
            .await?;
        Ok(count > 0)
    }

    async fn count_threads_by_categories(
        &self,
        ids: &[Uuid],
    ) -> Result<Vec<(Uuid, u64)>, AppError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let (sql, values) = Query::select()
            .column((categories::Entity, categories::Column::Id))
            .expr_as(
                Func::coalesce([
                    Func::sum(
                        CaseStatement::new()
                            .case(
                                Expr::col((threads::Entity, threads::Column::Id)).is_not_null()
                                    .and(Expr::col((threads::Entity, threads::Column::DeletedAt)).is_null()),
                                1i32,
                            )
                            .finally(0i32),
                    )
                    .into(),
                    Expr::val(0i64).into(),
                ]),
                Alias::new("thread_count"),
            )
            .from(categories::Entity)
            .join(
                JoinType::LeftJoin,
                threads::Entity,
                Expr::col((threads::Entity, threads::Column::CategoryId))
                    .equals((categories::Entity, categories::Column::Id)),
            )
            .and_where(
                Expr::col((categories::Entity, categories::Column::Id))
                    .is_in(ids.to_vec()),
            )
            .group_by_col((categories::Entity, categories::Column::Id))
            .build(PostgresQueryBuilder);

        let rows = CategoryCount::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
        .all(&self.db)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.id, r.thread_count as u64))
            .collect())
    }
}
