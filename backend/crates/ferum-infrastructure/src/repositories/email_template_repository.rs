use async_trait::async_trait;
use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};

use ferum_domain::models::email_template::EmailTemplate;
use ferum_domain::repositories::email_template_repository::EmailTemplateRepository;
use ferum_domain::AppError;

use crate::entities::email_templates;

pub struct PgEmailTemplateRepository {
    db: DatabaseConnection,
}

impl PgEmailTemplateRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: email_templates::Model) -> EmailTemplate {
    EmailTemplate {
        key: m.key,
        locale: m.locale,
        subject: m.subject,
        body_html: m.body_html,
        updated_at: m.updated_at.with_timezone(&Utc),
    }
}

#[async_trait]
impl EmailTemplateRepository for PgEmailTemplateRepository {
    async fn list(&self) -> Result<Vec<EmailTemplate>, AppError> {
        Ok(email_templates::Entity::find()
            .order_by_asc(email_templates::Column::Key)
            .order_by_asc(email_templates::Column::Locale)
            .all(&self.db)
            .await?
            .into_iter()
            .map(to_domain)
            .collect())
    }

    async fn find(&self, key: &str, locale: &str) -> Result<Option<EmailTemplate>, AppError> {
        Ok(email_templates::Entity::find_by_id((
            key.to_string(),
            locale.to_string(),
        ))
        .one(&self.db)
        .await?
        .map(to_domain))
    }

    async fn resolve(
        &self,
        key: &str,
        chain: &[String],
    ) -> Result<Option<EmailTemplate>, AppError> {
        if chain.is_empty() {
            return Ok(None);
        }

        let rows = email_templates::Entity::find()
            .filter(email_templates::Column::Key.eq(key))
            .filter(email_templates::Column::Locale.is_in(chain.iter().map(String::as_str)))
            .all(&self.db)
            .await?;

        // Ordered by the chain, never by the database. `IN` returns rows in
        // whatever order the planner likes, so sorting there would pick the
        // wrong locale in a way that only shows up under load or after a
        // statistics change.
        Ok(chain.iter().find_map(|candidate| {
            rows.iter()
                .find(|r| &r.locale == candidate)
                .cloned()
                .map(to_domain)
        }))
    }

    async fn upsert(
        &self,
        key: &str,
        locale: &str,
        subject: &str,
        body_html: &str,
    ) -> Result<(), AppError> {
        let model = email_templates::ActiveModel {
            key: Set(key.to_string()),
            locale: Set(locale.to_string()),
            subject: Set(subject.to_string()),
            body_html: Set(body_html.to_string()),
            updated_at: Set(Utc::now().fixed_offset()),
        };
        email_templates::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([
                    email_templates::Column::Key,
                    email_templates::Column::Locale,
                ])
                .update_columns([
                    email_templates::Column::Subject,
                    email_templates::Column::BodyHtml,
                    email_templates::Column::UpdatedAt,
                ])
                .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn insert_if_absent(
        &self,
        key: &str,
        locale: &str,
        subject: &str,
        body_html: &str,
    ) -> Result<bool, AppError> {
        let model = email_templates::ActiveModel {
            key: Set(key.to_string()),
            locale: Set(locale.to_string()),
            subject: Set(subject.to_string()),
            body_html: Set(body_html.to_string()),
            updated_at: Set(Utc::now().fixed_offset()),
        };
        // `do_nothing` makes the whole statement a no-op on conflict, which
        // sea-orm surfaces as `DbErr::RecordNotInserted` rather than Ok — the
        // row already existing is the expected case on every boot after the
        // first, so it is a `false`, not an error.
        match email_templates::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([
                    email_templates::Column::Key,
                    email_templates::Column::Locale,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(&self.db)
            .await
        {
            Ok(_) => Ok(true),
            Err(sea_orm::DbErr::RecordNotInserted) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn delete(&self, key: &str, locale: &str) -> Result<(), AppError> {
        email_templates::Entity::delete_by_id((key.to_string(), locale.to_string()))
            .exec(&self.db)
            .await?;
        Ok(())
    }
}
