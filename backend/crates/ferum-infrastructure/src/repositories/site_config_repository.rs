#![allow(dead_code)]

use sea_orm::prelude::*;
use sea_orm::*;
use std::collections::HashMap;

use crate::entities::site_config;
use async_trait::async_trait;
use ferum_application::shared::AppError;
use ferum_domain::repositories::SiteConfigRepository;

pub struct PgSiteConfigRepository {
    db: DatabaseConnection,
}

impl PgSiteConfigRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl SiteConfigRepository for PgSiteConfigRepository {
    async fn get_all(&self) -> Result<HashMap<String, String>, AppError> {
        let rows = site_config::Entity::find().all(&self.db).await?;
        Ok(rows.into_iter().map(|r| (r.key, r.value)).collect())
    }

    async fn get(&self, key: &str) -> Result<Option<String>, AppError> {
        Ok(site_config::Entity::find_by_id(key)
            .one(&self.db)
            .await?
            .map(|r| r.value))
    }

    async fn set(&self, key: &str, value: &str) -> Result<(), AppError> {
        let model = site_config::ActiveModel {
            key: Set(key.to_string()),
            value: Set(value.to_string()),
            updated_at: Set(chrono::Utc::now().fixed_offset()),
            updated_by_id: NotSet,
        };
        site_config::Entity::insert(model)
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(site_config::Column::Key)
                    .update_columns([site_config::Column::Value, site_config::Column::UpdatedAt])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn set_many(&self, entries: &HashMap<String, String>) -> Result<(), AppError> {
        for (k, v) in entries {
            self.set(k, v).await?;
        }
        Ok(())
    }
}
