use std::sync::Arc;

use uuid::Uuid;

use crate::constants::DEFAULT_THEME_SLUG;
use crate::permission::PermissionChecker;
use crate::ports::{JobQueue, StorageService};
use crate::shared::AppError;
use ferum_domain::models::theme::{NewTheme, Theme};
use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
use ferum_domain::repositories::theme_repository::ThemeRepository;
use ferum_domain::AuthUser;

pub struct ThemeUseCase {
    themes: Arc<dyn ThemeRepository>,
    /// The three pieces needed to give a preview image's CAS reference back.
    ///
    /// Optional as a set, and only ever read together — `with_cas` is the only
    /// way to populate them, so there is no state where one is present and the
    /// others are not. `None` means "no reference counting", which is what the
    /// test builders want and what the code did before this existed.
    cas: Option<CasRefs>,
}

struct CasRefs {
    stored_files: Arc<dyn StoredFileRepository>,
    jobs: Arc<dyn JobQueue>,
    /// Only for `key_from_url`: `themes.preview_url` stores a `/files/{key}`
    /// identity, and turning that back into a key is the storage backend's job
    /// — the shape differs per backend and older rows may hold absolute URLs.
    storage: Arc<dyn StorageService>,
}

impl ThemeUseCase {
    pub fn new(themes: Arc<dyn ThemeRepository>) -> Self {
        Self { themes, cas: None }
    }

    pub fn with_cas(
        mut self,
        stored_files: Arc<dyn StoredFileRepository>,
        jobs: Arc<dyn JobQueue>,
        storage: Arc<dyn StorageService>,
    ) -> Self {
        self.cas = Some(CasRefs {
            stored_files,
            jobs,
            storage,
        });
        self
    }

    /// Releases whatever preview image `slug` currently points at.
    ///
    /// Both callers need this and neither used to do it: replacing a preview
    /// took a fresh reference and abandoned the old one at `ref_count = 1`, and
    /// deleting a theme abandoned it outright. A row stuck at 1 is worse than
    /// one stuck at 0 — it looks referenced, so a sweep looking for orphans by
    /// `ref_count = 0` walks straight past it.
    async fn release_current_preview(&self, slug: &str) {
        let Some(cas) = &self.cas else { return };
        let Ok(Some(theme)) = self.themes.find_by_slug(slug).await else {
            return;
        };
        let Some(key) = theme
            .preview_url
            .as_deref()
            .filter(|url| !url.is_empty())
            .and_then(|url| cas.storage.key_from_url(url))
        else {
            return;
        };
        crate::storage_utils::release_cas_ref(&cas.stored_files, &cas.jobs, &key).await;
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
        // Before the overwrite: once `preview_url` is replaced the old key is
        // unreachable, and the reference it holds can never be given back.
        self.release_current_preview(slug).await;
        self.themes.update_preview_url(slug, url).await
    }

    pub async fn delete(&self, actor: &AuthUser, slug: &str) -> Result<(), AppError> {
        PermissionChecker::can_manage_config(actor)?;
        // Same reason, one step earlier: deleting the row takes `preview_url`
        // with it.
        self.release_current_preview(slug).await;
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
