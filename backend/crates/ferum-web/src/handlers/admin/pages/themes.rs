use axum::extract::{Multipart, Path, Query, State};
use axum::response::IntoResponse;
use axum::Extension;
use serde::Deserialize;
use std::io::Read;
use tera::Context;

use super::super::{render_admin, require_admin, site_ctx};
use crate::app_state::AppState;
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::CurrentUserCtx;
use ferum_application::usecases::theme_usecase::RegisterThemeCmd;

#[derive(Deserialize, Default)]
pub struct ThemeFlash {
    pub success: Option<String>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct ThemeJsonMeta {
    slug: String,
    name: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default = "default_version")]
    version: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    parent: Option<String>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

pub async fn themes(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
    Query(flash): Query<ThemeFlash>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let themes = state.theme.list().await?;

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));
    ctx.insert("themes", &themes);
    if let Some(msg) = flash.success {
        ctx.insert("flash_success", &msg);
    }
    if let Some(msg) = flash.error {
        ctx.insert("flash_error", &msg);
    }

    render_admin(&state, &req_locale, "admin/themes.html", &ctx).await
}

pub async fn upload_theme(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let mut zip_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        PageError::Internal(anyhow::anyhow!("Multipart error: {}", e))
    })? {
        if field.name() == Some("theme_zip") {
            zip_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| PageError::Internal(anyhow::anyhow!("Read error: {}", e)))?
                    .to_vec(),
            );
        }
    }

    let zip_bytes = zip_bytes
        .ok_or_else(|| PageError::Internal(anyhow::anyhow!("No theme_zip field in upload")))?;

    if zip_bytes.len() > 10 * 1024 * 1024 {
        return Err(PageError::Internal(anyhow::anyhow!("Theme zip exceeds 10 MB limit")));
    }

    let cursor = std::io::Cursor::new(&zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| PageError::Internal(anyhow::anyhow!("Invalid zip: {}", e)))?;

    let theme_json = {
        let mut file = archive
            .by_name("theme.json")
            .map_err(|_| PageError::Internal(anyhow::anyhow!("theme.json not found in zip")))?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| PageError::Internal(anyhow::anyhow!("Failed to read theme.json: {}", e)))?;
        content
    };

    let meta: ThemeJsonMeta = serde_json::from_str(&theme_json)
        .map_err(|e| PageError::Internal(anyhow::anyhow!("Invalid theme.json: {}", e)))?;

    if meta.slug.contains("..") || meta.slug.contains('/') || meta.slug.contains('\\') {
        return Err(PageError::Internal(anyhow::anyhow!("Invalid theme slug")));
    }

    const FORBIDDEN_EXTENSIONS: &[&str] = &["svelte", "ts", "tsx", "jsx", "vue"];
    let cursor = std::io::Cursor::new(&zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| PageError::Internal(anyhow::anyhow!("Invalid zip: {}", e)))?;

    let mut has_base_template = false;
    for i in 0..archive.len() {
        let file = archive
            .by_index(i)
            .map_err(|e| PageError::Internal(anyhow::anyhow!("Zip read error: {}", e)))?;
        if let Some(path) = file.enclosed_name() {
            let name = path.to_string_lossy();
            if name == "templates/base.html" {
                has_base_template = true;
            }
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if FORBIDDEN_EXTENSIONS.contains(&ext) {
                    return Err(PageError::Internal(anyhow::anyhow!(
                        "Theme zip contains '{}' which is not supported in Tera themes. \
                         Only pre-compiled HTML templates and CSS are allowed.",
                        name
                    )));
                }
            }
        }
    }
    if !has_base_template {
        return Err(PageError::Internal(anyhow::anyhow!(
            "Theme zip must contain 'templates/base.html'"
        )));
    }

    let themes_dir = std::path::PathBuf::from(&state.themes_dir);
    let theme_dir = themes_dir.join(&meta.slug);
    std::fs::create_dir_all(&theme_dir)
        .map_err(|e| PageError::Internal(anyhow::anyhow!("Failed to create theme dir: {}", e)))?;

    let cursor = std::io::Cursor::new(&zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| PageError::Internal(anyhow::anyhow!("Invalid zip: {}", e)))?;

    // Bound total decompressed size regardless of the entries' declared
    // uncompressed_size (attacker-controlled) — a small, highly-compressed
    // upload could otherwise decompress far past the 10 MB raw-upload cap
    // and exhaust disk space. Mirrors the limit already enforced for plugin
    // .fpkg extraction in package_extractor.rs.
    const MAX_EXTRACTED_SIZE: u64 = 50 * 1024 * 1024; // 50 MB
    let mut total_extracted: u64 = 0;

    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|e| {
            PageError::Internal(anyhow::anyhow!("Zip read error: {}", e))
        })?;

        let raw_path = match file.enclosed_name() {
            Some(path) => path,
            None => continue,
        };
        let outpath = theme_dir.join(&raw_path);

        // Double-check the resolved path is still inside theme_dir, matching
        // the guard used for plugin package extraction.
        if !outpath.starts_with(&theme_dir) {
            let _ = std::fs::remove_dir_all(&theme_dir);
            return Err(PageError::Internal(anyhow::anyhow!(
                "Archive entry escapes theme directory"
            )));
        }

        if file.is_dir() {
            std::fs::create_dir_all(&outpath).ok();
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let remaining = MAX_EXTRACTED_SIZE.saturating_sub(total_extracted);
            let mut content = Vec::new();
            file.take(remaining + 1).read_to_end(&mut content).ok();
            if content.len() as u64 > remaining {
                let _ = std::fs::remove_dir_all(&theme_dir);
                return Err(PageError::Internal(anyhow::anyhow!(
                    "Theme zip decompresses beyond the {}MB limit",
                    MAX_EXTRACTED_SIZE / 1024 / 1024
                )));
            }
            total_extracted += content.len() as u64;
            std::fs::write(&outpath, content).ok();
        }
    }

    state
        .theme
        .register(
            &auth_user,
            RegisterThemeCmd {
                slug: meta.slug.clone(),
                name: meta.name,
                author: meta.author,
                version: meta.version,
                description: meta.description,
                parent_slug: meta.parent,
            },
        )
        .await?;

    if let Err(e) = state.tera.reload_themes().await {
        tracing::warn!("Tera reload failed after theme upload: {}", e);
        return Ok(axum::response::Redirect::to(
            "/admin/themes?error=Theme+files+extracted+but+templates+failed+to+reload",
        ));
    }

    Ok(axum::response::Redirect::to(
        &format!("/admin/themes?success=Theme+%22{}%22+uploaded+successfully", meta.slug),
    ))
}

pub async fn activate_theme(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;
    state.theme.set_active(&auth_user, &slug).await?;
    let chain = state.theme.resolve_chain(&slug).await;
    let color_scheme = crate::startup::read_theme_color_scheme(&state.themes_dir, &slug);
    *state.active_theme_cache.write().await = slug;
    *state.active_theme_chain_cache.write().await = chain;
    *state.active_theme_color_scheme_cache.write().await = color_scheme;
    Ok(axum::response::Redirect::to("/admin/themes?success=Theme+activated"))
}

pub async fn delete_theme(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;
    state.theme.delete(&auth_user, &slug).await?;

    let theme_dir = std::path::Path::new(&state.themes_dir).join(&slug);
    if theme_dir.exists() {
        if let Err(e) = tokio::fs::remove_dir_all(&theme_dir).await {
            tracing::warn!("Failed to remove theme directory {:?}: {}", theme_dir, e);
        }
    }

    let _ = state.tera.reload_themes().await;

    Ok(axum::response::Redirect::to("/admin/themes?success=Theme+deleted"))
}

pub async fn upload_theme_preview(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, PageError> {
    use ferum_application::storage_utils::cas_key;

    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    const MAX_PREVIEW_BYTES: usize = 512 * 1024;

    let mut found: Option<(String, Vec<u8>)> = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        PageError::Internal(anyhow::anyhow!("multipart error: {}", e))
    })? {
        if field.name() != Some("preview_image") {
            continue;
        }
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        if !content_type.starts_with("image/") {
            return Ok(axum::response::Redirect::to(
                "/admin/themes?error=Preview+must+be+an+image+file",
            ));
        }
        let data = field.bytes().await.map_err(|e| {
            PageError::Internal(anyhow::anyhow!("read multipart: {}", e))
        })?;
        if data.len() > MAX_PREVIEW_BYTES {
            return Ok(axum::response::Redirect::to(
                "/admin/themes?error=Preview+image+must+be+under+512+KB",
            ));
        }
        found = Some((content_type, data.to_vec()));
        break;
    }

    let (content_type, data) = match found {
        Some(v) => v,
        None => {
            return Ok(axum::response::Redirect::to(
                "/admin/themes?error=No+image+file+provided",
            ))
        }
    };

    let key = cas_key("theme-previews", &data, &content_type);
    let size = data.len() as i64;

    state
        .stored_files
        .upsert_and_ref(&key, &content_type, &data, size, Some(auth_user.id))
        .await
        .map_err(|e| PageError::Internal(anyhow::anyhow!("storage: {:?}", e)))?;

    let preview_url = format!("/files/{key}");
    state
        .theme
        .set_preview(&auth_user, &slug, Some(preview_url))
        .await
        .map_err(|e| PageError::Internal(anyhow::anyhow!("{:?}", e)))?;

    Ok(axum::response::Redirect::to(
        "/admin/themes?success=Preview+image+uploaded",
    ))
}
