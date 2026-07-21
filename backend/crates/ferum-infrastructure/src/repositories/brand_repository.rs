use async_trait::async_trait;
use chrono::Utc;
use sea_orm::*;
use uuid::Uuid;

use crate::entities::brands;
use ferum_application::shared::AppError;
use ferum_domain::models::brand::{Brand, NewBrand, UpdateBrand};
use ferum_domain::repositories::brand_repository::BrandRepository;

pub struct PgBrandRepository {
    db: DatabaseConnection,
}

impl PgBrandRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: brands::Model) -> Brand {
    Brand {
        id: m.id,
        slug: m.slug,
        name: m.name,
        description: m.description,
        logo_url: m.logo_url,
        website: m.website,
        country: m.country,
        is_verified: m.is_verified,
        owner_user_id: m.owner_user_id,
        tier: m.tier,
        created_at: m.created_at.with_timezone(&Utc),
    }
}

#[async_trait]
impl BrandRepository for PgBrandRepository {
    async fn create(&self, brand: NewBrand) -> Result<Brand, AppError> {
        let model = brands::ActiveModel {
            id: Set(brand.id),
            slug: Set(brand.slug),
            name: Set(brand.name),
            description: Set(brand.description),
            logo_url: Set(brand.logo_url),
            website: Set(brand.website),
            country: Set(brand.country),
            is_verified: Set(false),
            owner_user_id: Set(brand.owner_user_id),
            // `tier` left unset → DB default ('free'); allowed set is free|sponsored.
            ..Default::default()
        };
        Ok(to_domain(model.insert(&self.db).await?))
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Brand>, AppError> {
        Ok(brands::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .map(to_domain))
    }

    async fn find_by_slug<'a>(&self, slug: &'a str) -> Result<Option<Brand>, AppError> {
        Ok(brands::Entity::find()
            .filter(brands::Column::Slug.eq(slug))
            .one(&self.db)
            .await?
            .map(to_domain))
    }

    async fn list(&self) -> Result<Vec<Brand>, AppError> {
        Ok(brands::Entity::find()
            .order_by_asc(brands::Column::Name)
            .all(&self.db)
            .await?
            .into_iter()
            .map(to_domain)
            .collect())
    }

    async fn update(&self, id: Uuid, patch: UpdateBrand) -> Result<Brand, AppError> {
        let model = brands::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(AppError::NotFound)?;
        let mut active: brands::ActiveModel = model.into();
        if let Some(v) = patch.name {
            active.name = Set(v);
        }
        if let Some(v) = patch.description {
            active.description = Set(v);
        }
        if let Some(v) = patch.website {
            active.website = Set(v);
        }
        if let Some(v) = patch.country {
            active.country = Set(v);
        }
        if let Some(v) = patch.is_verified {
            active.is_verified = Set(v);
        }
        active.updated_at = Set(Some(Utc::now().into()));
        Ok(to_domain(active.update(&self.db).await?))
    }

    async fn delete(&self, id: Uuid) -> Result<(), AppError> {
        brands::Entity::delete_by_id(id).exec(&self.db).await?;
        Ok(())
    }
}
