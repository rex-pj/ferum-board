use async_trait::async_trait;
use chrono::Utc;
use sea_orm::*;
use uuid::Uuid;

use crate::entities::materials;
use ferum_application::shared::AppError;
use ferum_domain::models::material::{Material, NewMaterial, UpdateMaterial};
use ferum_domain::repositories::material_repository::MaterialRepository;

pub struct PgMaterialRepository {
    db: DatabaseConnection,
}

impl PgMaterialRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: materials::Model) -> Material {
    Material {
        id: m.id,
        slug: m.slug,
        name: m.name,
        category: m.category,
        description: m.description,
        created_at: m.created_at.with_timezone(&Utc),
    }
}

#[async_trait]
impl MaterialRepository for PgMaterialRepository {
    async fn create(&self, material: NewMaterial) -> Result<Material, AppError> {
        let model = materials::ActiveModel {
            id: Set(material.id),
            slug: Set(material.slug),
            name: Set(material.name),
            category: Set(material.category),
            description: Set(material.description),
            ..Default::default()
        };
        Ok(to_domain(model.insert(&self.db).await?))
    }

    async fn find_by_slug<'a>(&self, slug: &'a str) -> Result<Option<Material>, AppError> {
        Ok(materials::Entity::find()
            .filter(materials::Column::Slug.eq(slug))
            .one(&self.db)
            .await?
            .map(to_domain))
    }

    async fn list<'a>(&self, category: Option<&'a str>) -> Result<Vec<Material>, AppError> {
        let mut q = materials::Entity::find().order_by_asc(materials::Column::Name);
        if let Some(cat) = category.filter(|c| !c.is_empty()) {
            q = q.filter(materials::Column::Category.eq(cat));
        }
        Ok(q.all(&self.db).await?.into_iter().map(to_domain).collect())
    }

    async fn update(&self, id: Uuid, patch: UpdateMaterial) -> Result<Material, AppError> {
        let model = materials::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(AppError::NotFound)?;
        let mut active: materials::ActiveModel = model.into();
        if let Some(v) = patch.name {
            active.name = Set(v);
        }
        if let Some(v) = patch.category {
            active.category = Set(v);
        }
        if let Some(v) = patch.description {
            active.description = Set(v);
        }
        Ok(to_domain(active.update(&self.db).await?))
    }

    async fn delete(&self, id: Uuid) -> Result<(), AppError> {
        materials::Entity::delete_by_id(id).exec(&self.db).await?;
        Ok(())
    }
}
