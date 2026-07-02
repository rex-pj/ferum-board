use axum::extract::{Multipart, Path, Query, State};
use axum::response::{Html, IntoResponse};
use axum::Extension;
use serde::{Deserialize, Serialize};
use tera::Context;

use super::super::{render_admin, require_admin, site_ctx};
use crate::app_state::AppState;
use crate::handlers::pages::{PageError, require_page_auth};
use crate::middleware::AuthUser;
use crate::view_models::page_context::{CurrentUserCtx, PluginDetailCtx, PluginLogCtx};
use ferum_infrastructure::plugins::package_extractor;

#[derive(Serialize)]
struct HookCapabilityCtx {
    name: String,
    priority: i64,
}

#[derive(Serialize)]
struct PluginReviewCtx {
    slug: String,
    name: String,
    version: String,
    tier: String,
    author: String,
    description: String,
    capabilities_json: String,
    hooks: Vec<HookCapabilityCtx>,
    http_allowlist: Vec<String>,
    rpc_actions: Vec<String>,
    api_functions: Vec<String>,
    wants_db: bool,
    wants_media: bool,
    schema_tables: Vec<String>,
}

#[derive(Serialize)]
struct PluginRowCtx {
    id: String,
    slug: String,
    name: String,
    version: String,
    tier: String,
    status: String,
}

#[derive(Deserialize)]
pub struct UninstallPluginForm {
    pub confirm_slug: String,
}

#[derive(Deserialize)]
pub struct SavePluginConfigForm {
    pub config_json: String,
}

pub async fn upload_plugin(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let mut pkg_bytes: Option<bytes::Bytes> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| PageError::Internal(anyhow::anyhow!("Multipart error: {}", e)))?
    {
        if field.name() == Some("file") {
            let data = field
                .bytes()
                .await
                .map_err(|e| PageError::Internal(anyhow::anyhow!("Read error: {}", e)))?;
            if data.len() > 50 * 1024 * 1024 {
                return Err(PageError::Internal(anyhow::anyhow!("Package exceeds 50 MB limit")));
            }
            pkg_bytes = Some(data);
            break;
        }
    }

    let pkg_bytes = pkg_bytes
        .ok_or_else(|| PageError::Internal(anyhow::anyhow!("No file field in upload")))?;

    let plugins_dir = std::path::PathBuf::from(&state.plugins_dir);
    let tmp_slug = format!("__tmp_review_{}", uuid::Uuid::new_v4().simple());
    let extracted = ferum_infrastructure::plugins::package_extractor::extract(
        &pkg_bytes, &plugins_dir, &tmp_slug,
    )
    .map_err(|e| PageError::Internal(anyhow::anyhow!("Extract failed: {:?}", e)))?;

    let manifest = ferum_infrastructure::plugins::manifest_loader::load_from_dir(&extracted)
        .map_err(|e| {
            let _ = ferum_infrastructure::plugins::package_extractor::remove_plugin_dir(
                &extracted.to_string_lossy(),
            );
            PageError::Internal(anyhow::anyhow!("Manifest parse failed: {:?}", e))
        })?;

    let _ = ferum_infrastructure::plugins::package_extractor::remove_plugin_dir(
        &extracted.to_string_lossy(),
    );

    let capabilities = manifest
        .raw
        .get("capabilities")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let capabilities_json = serde_json::to_string_pretty(&capabilities).unwrap_or_default();

    let hooks: Vec<HookCapabilityCtx> = capabilities
        .get("hooks")
        .and_then(|h| h.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let name = item.get("name")?.as_str()?.to_string();
                    let priority = item.get("priority").and_then(|p| p.as_i64()).unwrap_or(100);
                    Some(HookCapabilityCtx { name, priority })
                })
                .collect()
        })
        .unwrap_or_default();

    let http_allowlist: Vec<String> = capabilities
        .get("http_allowlist")
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let rpc_actions: Vec<String> = capabilities
        .get("rpc")
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let api_functions: Vec<String> = capabilities
        .get("api")
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let wants_db = capabilities.get("db").and_then(|v| v.as_bool()).unwrap_or(false);
    let wants_media = capabilities.get("media").and_then(|v| v.as_bool()).unwrap_or(false);

    // Shown verbatim so the admin can actually read the DDL before granting `db` —
    // a bare "grant SQL schema access" checkbox with no visible statements would
    // not be a meaningful review.
    let schema_tables: Vec<String> = manifest
        .raw
        .get("schema")
        .and_then(|s| s.get("tables"))
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let mut ctx = Context::new();
    ctx.insert("plugin", &PluginReviewCtx {
        slug: manifest.id,
        name: manifest.name,
        version: manifest.version,
        tier: manifest.tier.as_str().to_string(),
        author: manifest.author.unwrap_or_default(),
        description: manifest.description.unwrap_or_default(),
        capabilities_json,
        hooks,
        http_allowlist,
        rpc_actions,
        api_functions,
        wants_db,
        wants_media,
        schema_tables,
    });
    let html = state
        .tera
        .render("admin/plugin_review_partial.html", &ctx)
        .await
        .map_err(|e| PageError::Internal(anyhow::anyhow!("Render error: {:?}", e)))?;
    Ok(Html(html))
}

pub async fn install_plugin(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let mut pkg_bytes: Option<bytes::Bytes> = None;
    let mut granted_capabilities = serde_json::json!({});

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| PageError::Internal(anyhow::anyhow!("Multipart error: {}", e)))?
    {
        match field.name() {
            Some("file") => {
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| PageError::Internal(anyhow::anyhow!("Read error: {}", e)))?;
                pkg_bytes = Some(data);
            }
            Some("granted_capabilities") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| PageError::Internal(anyhow::anyhow!("Read error: {}", e)))?;
                granted_capabilities =
                    serde_json::from_str(&text).unwrap_or(serde_json::json!({}));
            }
            _ => {}
        }
    }

    let pkg_bytes =
        pkg_bytes.ok_or_else(|| PageError::Internal(anyhow::anyhow!("No file field")))?;

    let plugins_dir = std::path::PathBuf::from(&state.plugins_dir);

    let manifest = {
        let tmp = format!("__tmp_install_{}", uuid::Uuid::new_v4().simple());
        let path = ferum_infrastructure::plugins::package_extractor::extract(
            &pkg_bytes, &plugins_dir, &tmp,
        )
        .map_err(|e| PageError::Internal(anyhow::anyhow!("{:?}", e)))?;
        let m = ferum_infrastructure::plugins::manifest_loader::load_from_dir(&path)
            .map_err(|e| PageError::Internal(anyhow::anyhow!("{:?}", e)))?;
        let _ = ferum_infrastructure::plugins::package_extractor::remove_plugin_dir(
            &path.to_string_lossy(),
        );
        m
    };

    let install_path = ferum_infrastructure::plugins::package_extractor::extract(
        &pkg_bytes,
        &plugins_dir,
        &manifest.id,
    )
    .map_err(|e| PageError::Internal(anyhow::anyhow!("{:?}", e)))?;

    let plugin_id = manifest.id.clone();
    state
        .plugin
        .register_extracted(
            &auth_user,
            manifest.id,
            manifest.name,
            manifest.version,
            manifest.tier,
            manifest.raw,
            install_path.to_string_lossy().to_string(),
            granted_capabilities,
        )
        .await
        .map_err(|e| {
            let _ = package_extractor::remove_plugin_dir(&install_path.to_string_lossy());
            PageError::Internal(anyhow::anyhow!("{:?}", e))
        })?;

    Ok(axum::response::Redirect::to(&format!(
        "/admin/plugins?success=Plugin+%22{}%22+installed+successfully",
        plugin_id
    )))
}

pub async fn plugins(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(flash): Query<super::themes::ThemeFlash>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let plugins = state.plugin.list(&auth_user).await?;
    let plugins_ctx: Vec<PluginRowCtx> = plugins
        .into_iter()
        .map(|p| {
            use ferum_domain::models::plugin::{PluginStatus, PluginTier};
            PluginRowCtx {
                id: p.id.to_string(),
                slug: p.slug.clone(),
                name: p.name.clone(),
                version: p.version.clone(),
                tier: match p.tier {
                    PluginTier::Manifest => "Manifest",
                    PluginTier::Script => "Script",
                    PluginTier::Service => "Service",
                }
                .to_string(),
                status: match p.status {
                    PluginStatus::Installing => "installing",
                    PluginStatus::Active => "active",
                    PluginStatus::Inactive => "inactive",
                    PluginStatus::Error => "error",
                    PluginStatus::Disabled => "disabled",
                    PluginStatus::Uninstalling => "uninstalling",
                }
                .to_string(),
            }
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));
    ctx.insert("plugins", &plugins_ctx);
    if let Some(msg) = flash.success {
        ctx.insert("flash_success", &msg);
    }
    if let Some(msg) = flash.error {
        ctx.insert("flash_error", &msg);
    }

    render_admin(&state, "admin/plugins.html", &ctx).await
}

pub async fn activate_plugin(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;
    state
        .plugin
        .activate(&auth_user, &slug)
        .await
        .map_err(|e| PageError::Internal(anyhow::anyhow!("{:?}", e)))?;
    Ok(axum::response::Redirect::to("/admin/plugins?success=Plugin+activated"))
}

pub async fn deactivate_plugin(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;
    state
        .plugin
        .deactivate(&auth_user, &slug)
        .await
        .map_err(|e| PageError::Internal(anyhow::anyhow!("{:?}", e)))?;
    Ok(axum::response::Redirect::to("/admin/plugins?success=Plugin+deactivated"))
}

pub async fn uninstall_plugin(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    axum::Form(form): axum::Form<UninstallPluginForm>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;
    state
        .plugin
        .uninstall(&auth_user, &slug, &form.confirm_slug)
        .await
        .map_err(|e| PageError::Internal(anyhow::anyhow!("{:?}", e)))?;
    Ok(axum::response::Redirect::to("/admin/plugins?success=Plugin+uninstalled"))
}

pub async fn plugin_detail(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let plugin = state.plugin.get_by_slug(&auth_user, &slug).await?;

    let log_query = ferum_domain::models::plugin::PluginLogQuery {
        level: None,
        hook_name: None,
        since: None,
        limit: 100,
        offset: 0,
    };
    let logs = state
        .plugin
        .get_logs(&auth_user, &slug, log_query)
        .await
        .unwrap_or_default();

    let config_schema = plugin
        .manifest
        .get("config_schema")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let config_json =
        serde_json::to_string_pretty(&plugin.config).unwrap_or_else(|_| "{}".to_string());

    let plugin_ctx = PluginDetailCtx {
        slug: plugin.slug.clone(),
        name: plugin.name.clone(),
        version: plugin.version.clone(),
        tier: plugin.tier.as_str().to_string(),
        status: plugin.status.as_str().to_string(),
        is_active: plugin.status == ferum_domain::models::plugin::PluginStatus::Active,
        error_message: plugin.error_message.clone(),
        circuit_open: plugin.circuit_open,
        config_json,
        config_schema,
        install_path: plugin.install_path.clone(),
        installed_at: plugin.installed_at.format("%Y-%m-%d %H:%M UTC").to_string(),
        activated_at: plugin
            .activated_at
            .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string()),
        logs: logs
            .into_iter()
            .map(|l| PluginLogCtx {
                id: l.id.to_string(),
                level: l.level,
                hook_name: l.hook_name,
                duration_ms: l.duration_ms,
                message: l.message,
                created_at: l.created_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            })
            .collect(),
    };

    let flash_success = q.get("success").cloned();
    let flash_error = q.get("error").cloned();

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state).await);
    ctx.insert("current_user", &CurrentUserCtx::from(&auth_user));
    ctx.insert("plugin", &plugin_ctx);
    ctx.insert("flash_success", &flash_success);
    ctx.insert("flash_error", &flash_error);

    render_admin(&state, "admin/plugins/detail.html", &ctx).await
}

pub async fn save_config(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    axum::Form(form): axum::Form<SavePluginConfigForm>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    let config: serde_json::Value = serde_json::from_str(&form.config_json).map_err(|e| {
        PageError::Internal(anyhow::anyhow!("Invalid JSON: {}", e))
    })?;

    state
        .plugin
        .configure(&auth_user, &slug, config)
        .await
        .map_err(|e| PageError::Internal(anyhow::anyhow!("{:?}", e)))?;

    Ok(axum::response::Redirect::to(&format!(
        "/admin/plugins/{slug}?success=Configuration+saved"
    )))
}
