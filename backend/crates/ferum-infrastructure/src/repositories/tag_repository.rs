use async_trait::async_trait;
use chrono::Utc;
use sea_orm::prelude::*;
use sea_orm::sea_query::extension::postgres::PgExpr;
use sea_orm::*;
use std::collections::HashMap;
use uuid::Uuid;

use crate::entities::{tags, thread_tags};
use ferum_application::shared::AppError;
use ferum_domain::models::tag::{NewTag, Tag};
use ferum_domain::repositories::tag_repository::TagRepository;

pub struct PgTagRepository {
    db: DatabaseConnection,
}

impl PgTagRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: tags::Model) -> Tag {
    Tag {
        id: m.id,
        name: m.name,
        slug: m.slug,
        color: m.color,
        created_by_id: m.created_by_id,
        created_at: m.created_at.with_timezone(&Utc),
    }
}

#[async_trait]
impl TagRepository for PgTagRepository {
    async fn list<'a>(&self, query: Option<&'a str>, limit: u32) -> Result<Vec<Tag>, AppError> {
        let mut q = tags::Entity::find().order_by_asc(tags::Column::Name);
        if let Some(search) = query.filter(|s| !s.is_empty()) {
            // Case-insensitive (ILIKE) so tag suggestions ignore letter case.
            let pattern = format!("%{}%", search);
            q = q.filter(
                Condition::any()
                    .add(Expr::col(tags::Column::Name).ilike(pattern.clone()))
                    .add(Expr::col(tags::Column::Slug).ilike(pattern)),
            );
        }
        Ok(q.limit(limit as u64).all(&self.db).await?.into_iter().map(to_domain).collect())
    }

    async fn find_by_slug<'a>(&self, slug: &'a str) -> Result<Option<Tag>, AppError> {
        Ok(tags::Entity::find()
            .filter(tags::Column::Slug.eq(slug))
            .one(&self.db)
            .await?
            .map(to_domain))
    }

    async fn create(&self, tag: NewTag) -> Result<Tag, AppError> {
        let model = tags::ActiveModel {
            id: Set(tag.id),
            name: Set(tag.name),
            slug: Set(tag.slug),
            color: Set(tag.color),
            created_by_id: Set(tag.created_by_id),
            ..Default::default()
        };
        Ok(to_domain(model.insert(&self.db).await?))
    }

    async fn assign_to_thread(&self, thread_id: Uuid, tag_ids: &[Uuid]) -> Result<(), AppError> {
        if tag_ids.is_empty() {
            return Ok(());
        }
        let models: Vec<thread_tags::ActiveModel> = tag_ids
            .iter()
            .map(|&tag_id| thread_tags::ActiveModel {
                thread_id: Set(thread_id),
                tag_id: Set(tag_id),
            })
            .collect();
        thread_tags::Entity::insert_many(models)
            .on_conflict(
                sea_orm::sea_query::OnConflict::columns([
                    thread_tags::Column::ThreadId,
                    thread_tags::Column::TagId,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn replace_thread_tags(&self, thread_id: Uuid, tag_ids: &[Uuid]) -> Result<(), AppError> {
        thread_tags::Entity::delete_many()
            .filter(thread_tags::Column::ThreadId.eq(thread_id))
            .exec(&self.db)
            .await?;
        self.assign_to_thread(thread_id, tag_ids).await
    }

    async fn find_by_thread(&self, thread_id: Uuid) -> Result<Vec<Tag>, AppError> {
        let tag_ids: Vec<Uuid> = thread_tags::Entity::find()
            .filter(thread_tags::Column::ThreadId.eq(thread_id))
            .all(&self.db)
            .await?
            .into_iter()
            .map(|r| r.tag_id)
            .collect();
        if tag_ids.is_empty() {
            return Ok(vec![]);
        }
        Ok(tags::Entity::find()
            .filter(tags::Column::Id.is_in(tag_ids))
            .order_by_asc(tags::Column::Name)
            .all(&self.db)
            .await?
            .into_iter()
            .map(to_domain)
            .collect())
    }

    async fn find_by_threads(
        &self,
        thread_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Vec<Tag>>, AppError> {
        if thread_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = thread_tags::Entity::find()
            .filter(thread_tags::Column::ThreadId.is_in(thread_ids.to_vec()))
            .all(&self.db)
            .await?;
        if rows.is_empty() {
            return Ok(HashMap::new());
        }
        let all_tag_ids: Vec<Uuid> = rows.iter().map(|r| r.tag_id).collect();
        let tag_map: HashMap<Uuid, Tag> = tags::Entity::find()
            .filter(tags::Column::Id.is_in(all_tag_ids))
            .all(&self.db)
            .await?
            .into_iter()
            .map(|t| (t.id, to_domain(t)))
            .collect();

        let mut result: HashMap<Uuid, Vec<Tag>> = HashMap::new();
        for row in rows {
            if let Some(tag) = tag_map.get(&row.tag_id) {
                result.entry(row.thread_id).or_default().push(tag.clone());
            }
        }
        Ok(result)
    }
}
