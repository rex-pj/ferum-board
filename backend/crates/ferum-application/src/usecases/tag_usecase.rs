use std::sync::Arc;

use uuid::Uuid;

use crate::shared::AppError;
use ferum_domain::models::role::perm;
use ferum_domain::models::tag::{NewTag, Tag};
use ferum_domain::repositories::tag_repository::TagRepository;
use ferum_domain::AuthUser;

pub struct TagUseCase {
    pub tags: Arc<dyn TagRepository>,
}

impl TagUseCase {
    pub fn new(tags: Arc<dyn TagRepository>) -> Self {
        Self { tags }
    }

    pub async fn list(&self, query: Option<String>) -> Result<Vec<Tag>, AppError> {
        self.tags.list(query.as_deref(), 50).await
    }

    /// Find existing tags by their slugified names; create new ones only if the actor
    /// has the `tag.create` permission. Returns up to `max` tags.
    pub async fn resolve_or_create(
        &self,
        actor: &AuthUser,
        names: Vec<String>,
        max: usize,
    ) -> Result<Vec<Tag>, AppError> {
        let mut result = Vec::new();
        for raw_name in names.into_iter().take(max) {
            let name = raw_name.trim().to_string();
            if name.is_empty() {
                continue;
            }
            let slug = slug::slugify(&name);
            match self.tags.find_by_slug(&slug).await? {
                Some(tag) => result.push(tag),
                None => {
                    if !actor.has_perm(perm::TAG_CREATE) {
                        return Err(AppError::forbidden("tag_create_permission_required"));
                    }
                    let tag = self
                        .tags
                        .create(NewTag {
                            id: Uuid::new_v4(),
                            name,
                            slug,
                            color: None,
                            created_by_id: Some(actor.id),
                        })
                        .await?;
                    result.push(tag);
                }
            }
        }
        Ok(result)
    }
}
