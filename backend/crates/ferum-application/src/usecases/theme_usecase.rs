use std::sync::Arc;

use uuid::Uuid;

use crate::constants::DEFAULT_THEME_SLUG;
use crate::permission::PermissionChecker;
use crate::shared::AppError;
use ferum_domain::models::theme::{NewTheme, Theme};
use ferum_domain::repositories::theme_repository::ThemeRepository;
use ferum_domain::AuthUser;

pub struct ThemeUseCase {
    themes: Arc<dyn ThemeRepository>,
}

impl ThemeUseCase {
    pub fn new(themes: Arc<dyn ThemeRepository>) -> Self {
        Self { themes }
    }

    pub async fn list(&self) -> Result<Vec<Theme>, AppError> {
        self.themes.list().await
    }

    pub async fn get_active(&self) -> Result<Theme, AppError> {
        self.themes.get_active().await
    }

    pub async fn register(&self, actor: &AuthUser, cmd: RegisterThemeCmd) -> Result<Theme, AppError> {
        PermissionChecker::can_manage_config(actor)?;
        self.themes
            .upsert(NewTheme {
                id: Uuid::new_v4(),
                slug: cmd.slug,
                name: cmd.name,
                author: cmd.author,
                version: cmd.version,
                description: cmd.description,
                parent_slug: cmd.parent_slug.unwrap_or_else(|| DEFAULT_THEME_SLUG.to_string()),
            })
            .await
    }

    pub async fn set_active(&self, actor: &AuthUser, slug: &str) -> Result<(), AppError> {
        PermissionChecker::can_manage_config(actor)?;
        self.themes.set_active(slug).await
    }

    /// Walk `parent_slug` up to "default", returning the full inheritance chain
    /// ordered from the active theme down to the root (e.g. ["ferum-dark", "default"]).
    /// Used by SSR render_with_theme() to find the nearest template in the hierarchy.
    pub async fn resolve_chain(&self, slug: &str) -> Vec<String> {
        let mut chain: Vec<String> = Vec::new();
        let mut current = slug.to_string();
        let mut seen = std::collections::HashSet::new();

        loop {
            if !seen.insert(current.clone()) {
                break;
            }
            chain.push(current.clone());
            if current == DEFAULT_THEME_SLUG {
                break;
            }
            match self.themes.find_by_slug(&current).await {
                Ok(Some(theme)) if theme.parent_slug != current => {
                    current = theme.parent_slug;
                }
                _ => {
                    if chain.last().map(|s| s.as_str()) != Some(DEFAULT_THEME_SLUG) {
                        chain.push(DEFAULT_THEME_SLUG.to_string());
                    }
                    break;
                }
            }
        }
        chain
    }

    pub async fn set_preview(
        &self,
        actor: &AuthUser,
        slug: &str,
        url: Option<String>,
    ) -> Result<(), AppError> {
        PermissionChecker::can_manage_config(actor)?;
        self.themes.update_preview_url(slug, url).await
    }

    pub async fn delete(&self, actor: &AuthUser, slug: &str) -> Result<(), AppError> {
        PermissionChecker::can_manage_config(actor)?;
        self.themes.delete(slug).await
    }
}

pub struct RegisterThemeCmd {
    pub slug: String,
    pub name: String,
    pub author: Option<String>,
    pub version: String,
    pub description: Option<String>,
    pub parent_slug: Option<String>,
}
