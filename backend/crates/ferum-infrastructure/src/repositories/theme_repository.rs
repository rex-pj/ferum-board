use async_trait::async_trait;
use chrono::Utc;
use sea_orm::prelude::*;
use sea_orm::*;

use crate::entities::themes;
use ferum_application::shared::AppError;
use ferum_domain::models::theme::{NewTheme, Theme};
use ferum_domain::repositories::theme_repository::ThemeRepository;

pub struct PgThemeRepository {
    db: DatabaseConnection,
}

impl PgThemeRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: themes::Model) -> Theme {
    Theme {
        id: m.id,
        slug: m.slug,
        name: m.name,
        author: m.author,
        version: m.version,
        description: m.description,
        parent_slug: m.parent_slug,
        is_system: m.is_system,
        is_active: m.is_active,
        preview_url: m.preview_url,
        created_at: m.created_at.with_timezone(&Utc),
    }
}

#[async_trait]
impl ThemeRepository for PgThemeRepository {
    async fn list(&self) -> Result<Vec<Theme>, AppError> {
        Ok(themes::Entity::find()
            .order_by_asc(themes::Column::CreatedAt)
            .all(&self.db)
            .await?
            .into_iter()
            .map(to_domain)
            .collect())
    }

    async fn get_active(&self) -> Result<Theme, AppError> {
        themes::Entity::find()
            .filter(themes::Column::IsActive.eq(true))
            .one(&self.db)
            .await?
            .map(to_domain)
            .ok_or(AppError::NotFound)
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<Theme>, AppError> {
        Ok(themes::Entity::find()
            .filter(themes::Column::Slug.eq(slug))
            .one(&self.db)
            .await?
            .map(to_domain))
    }

    async fn upsert(&self, theme: NewTheme) -> Result<Theme, AppError> {
        let existing = themes::Entity::find()
            .filter(themes::Column::Slug.eq(&theme.slug))
            .one(&self.db)
            .await?;

        let model = if let Some(existing) = existing {
            let mut active: themes::ActiveModel = existing.into();
            active.name = Set(theme.name);
            active.author = Set(theme.author);
            active.version = Set(theme.version);
            active.description = Set(theme.description);
            active.parent_slug = Set(theme.parent_slug);
            active.update(&self.db).await?
        } else {
            themes::ActiveModel {
                id: Set(theme.id),
                slug: Set(theme.slug),
                name: Set(theme.name),
                author: Set(theme.author),
                version: Set(theme.version),
                description: Set(theme.description),
                parent_slug: Set(theme.parent_slug),
                is_system: Set(false),
                is_active: Set(false),
                preview_url: Set(None),
                ..Default::default()
            }
            .insert(&self.db)
            .await?
        };
        Ok(to_domain(model))
    }

    async fn set_active(&self, slug: &str) -> Result<(), AppError> {
        // Deactivate all, then activate the target
        themes::Entity::update_many()
            .col_expr(themes::Column::IsActive, Expr::value(false))
            .exec(&self.db)
            .await?;

        let updated = themes::Entity::update_many()
            .col_expr(themes::Column::IsActive, Expr::value(true))
            .filter(themes::Column::Slug.eq(slug))
            .exec(&self.db)
            .await?;

        if updated.rows_affected == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    async fn update_preview_url(&self, slug: &str, url: Option<String>) -> Result<(), AppError> {
        let updated = themes::Entity::update_many()
            .col_expr(themes::Column::PreviewUrl, Expr::value(url))
            .filter(themes::Column::Slug.eq(slug))
            .exec(&self.db)
            .await?;
        if updated.rows_affected == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    async fn delete(&self, slug: &str) -> Result<(), AppError> {
        let theme = themes::Entity::find()
            .filter(themes::Column::Slug.eq(slug))
            .one(&self.db)
            .await?
            .ok_or(AppError::NotFound)?;

        if theme.is_system {
            return Err(AppError::Forbidden(
                "Cannot delete the built-in default theme.".to_string(),
            ));
        }
        if theme.is_active {
            return Err(AppError::UnprocessableEntity(
                "Cannot delete the active theme. Activate another theme first.".to_string(),
            ));
        }

        themes::Entity::delete_by_id(theme.id).exec(&self.db).await?;
        Ok(())
    }
}
