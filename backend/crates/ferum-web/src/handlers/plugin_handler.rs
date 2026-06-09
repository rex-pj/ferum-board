use std::collections::HashMap;
use std::path::PathBuf;

use axum::extract::{Extension, Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use crate::app_state::AppState;
use crate::middleware::AuthUser;
use crate::view_models::plugin::{
    ActiveSlotsResponse, CapabilityReviewResponse, ConfigurePluginRequest, DebugHookRequest,
    DebugHookResponse, PluginDetailResponse, PluginListItem, PluginLogQueryParams,
    PluginLogResponse, SlotEntry, TogglePluginStatusRequest, UninstallPluginRequest,
};
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::ports::{HookContext, HookDecision};
use ferum_application::shared::AppError;
use ferum_domain::models::plugin::PluginLogQuery;
use ferum_infrastructure::plugins::{manifest_loader, package_extractor};

const MAX_FPKG_SIZE: usize = 50 * 1024 * 1024; // 50 MB

// ─── List all plugins ─────────────────────────────────────────────────────────

pub async fn list_plugins_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let plugins = state.plugin.list(actor).await?;
    let items: Vec<PluginListItem> = plugins.iter().map(PluginListItem::from).collect();
    Ok(Json(DataResponse::new(items)))
}

// ─── Get single plugin ────────────────────────────────────────────────────────

pub async fn get_plugin_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let plugin = state.plugin.get_by_slug(actor, &slug).await?;
    Ok(Json(DataResponse::new(PluginDetailResponse::from(&plugin))))
}

// ─── Upload .fpkg — step 1 of install (capability review) ────────────────────

/// Accepts multipart/form-data with a "file" field containing the .fpkg archive.
/// Extracts and validates the package, then returns the capability review payload
/// for the admin to review before confirming installation.
pub async fn upload_plugin_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    // Permission check happens inside register_extracted; do a lightweight check here
    // to fail fast before doing file I/O.
    ferum_application::permission::PermissionChecker::can_manage_plugins(actor)?;

    let mut pkg_bytes: Option<bytes::Bytes> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?
    {
        if field.name() == Some("file") {
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;

            if data.len() > MAX_FPKG_SIZE {
                return Err(AppError::unprocessable("Package too large (max 50MB)").into());
            }
            pkg_bytes = Some(data);
            break;
        }
    }

    let pkg_bytes = pkg_bytes.ok_or_else(|| AppError::unprocessable("No file field in request"))?;

    // Extract to a temporary location for validation
    let plugins_dir = PathBuf::from(&state.plugins_dir);
    let tmp_slug = format!("__tmp_upload_{}", uuid::Uuid::new_v4().simple());
    let extracted_path = package_extractor::extract(&pkg_bytes, &plugins_dir, &tmp_slug)
        .map_err(|e| { let _ = package_extractor::remove_plugin_dir(&plugins_dir.join(&tmp_slug).to_string_lossy()); e })?;

    let manifest = manifest_loader::load_from_dir(&extracted_path)
        .map_err(|e| { let _ = package_extractor::remove_plugin_dir(&extracted_path.to_string_lossy()); e })?;

    // Remove tmp extraction — will be re-extracted with correct slug on install confirm
    let _ = package_extractor::remove_plugin_dir(&extracted_path.to_string_lossy());

    let capabilities_requested = manifest
        .raw
        .get("capabilities")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    Ok((
        StatusCode::OK,
        Json(DataResponse::new(CapabilityReviewResponse {
            slug: manifest.id,
            name: manifest.name,
            version: manifest.version,
            tier: manifest.tier.as_str().to_string(),
            description: manifest.description,
            author: manifest.author,
            capabilities_requested,
        })),
    ))
}

// ─── Confirm install — step 2 ─────────────────────────────────────────────────

/// After admin reviews capabilities:
/// Re-uploads .fpkg (or re-extracts from a cached temp location — simplified: re-upload).
/// In production this would use a session token from step 1.
/// For M1 simplicity: the frontend re-sends the file along with granted_capabilities.
pub async fn install_plugin_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    ferum_application::permission::PermissionChecker::can_manage_plugins(actor)?;

    let mut pkg_bytes: Option<bytes::Bytes> = None;
    let mut granted_capabilities = serde_json::json!({});

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?
    {
        match field.name() {
            Some("file") => {
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
                pkg_bytes = Some(data);
            }
            Some("granted_capabilities") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
                granted_capabilities = serde_json::from_str(&text)
                    .map_err(|_| AppError::unprocessable("Invalid granted_capabilities JSON"))?;
            }
            _ => {}
        }
    }

    let pkg_bytes = pkg_bytes.ok_or_else(|| AppError::unprocessable("No file field"))?;

    let plugins_dir = PathBuf::from(&state.plugins_dir);

    // Parse manifest to get slug
    let manifest = {
        let tmp_slug = format!("__tmp_install_{}", uuid::Uuid::new_v4().simple());
        let extracted_path = package_extractor::extract(&pkg_bytes, &plugins_dir, &tmp_slug)?;
        let m = manifest_loader::load_from_dir(&extracted_path)?;
        let _ = package_extractor::remove_plugin_dir(&extracted_path.to_string_lossy());
        m
    };

    // Extract to final location using plugin slug
    let install_path = package_extractor::extract(&pkg_bytes, &plugins_dir, &manifest.id)?;

    let plugin = state
        .plugin
        .register_extracted(
            actor,
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
            // Clean up on failure
            let _ = package_extractor::remove_plugin_dir(&install_path.to_string_lossy());
            e
        })?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(PluginDetailResponse::from(&plugin))),
    ))
}

// ─── Configure ────────────────────────────────────────────────────────────────

pub async fn configure_plugin_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Json(body): Json<ConfigurePluginRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    state.plugin.configure(actor, &slug, body.config).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── Toggle status ────────────────────────────────────────────────────────────

pub async fn toggle_plugin_status_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Json(body): Json<TogglePluginStatusRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    if body.active {
        let plugin = state.plugin.activate(actor, &slug).await?;
        Ok(Json(DataResponse::new(PluginDetailResponse::from(&plugin))))
    } else {
        state.plugin.deactivate(actor, &slug).await?;
        Ok(Json(DataResponse::new(
            state.plugin.get_by_slug(actor, &slug).await.map(|p| PluginDetailResponse::from(&p))?,
        )))
    }
}

// ─── Uninstall ────────────────────────────────────────────────────────────────

pub async fn uninstall_plugin_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Json(body): Json<UninstallPluginRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let install_path = state
        .plugin
        .uninstall(actor, &slug, &body.confirm_slug)
        .await?;

    package_extractor::remove_plugin_dir(&install_path)?;

    Ok(StatusCode::NO_CONTENT)
}

// ─── Logs ─────────────────────────────────────────────────────────────────────

pub async fn get_plugin_logs_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Query(params): Query<PluginLogQueryParams>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    let query = PluginLogQuery {
        level: params.level,
        hook_name: params.hook_name,
        since: None,
        limit: params.limit,
        offset: params.offset,
    };
    let logs = state.plugin.get_logs(actor, &slug, query).await?;
    let items: Vec<PluginLogResponse> = logs.into_iter().map(PluginLogResponse::from).collect();
    Ok(Json(DataResponse::new(items)))
}

// ─── Debug hook (admin only) ─────────────────────────────────────────────────

pub async fn debug_hook_handler(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<DebugHookRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.as_ref().ok_or(AppError::Unauthorized)?;
    ferum_application::permission::PermissionChecker::can_manage_plugins(actor)?;

    let ctx = HookContext {
        hook_name: body.hook.clone(),
        actor_id: Some(actor.id),
        actor_trust_level: format!("{:?}", actor.trust_level).to_lowercase(),
        payload: body.payload,
    };

    let decision = state
        .plugin_runtime
        .dispatch_before_hook(&body.hook, &ctx)
        .await?;

    let response = match decision {
        HookDecision::Allow => DebugHookResponse {
            decision: "allow".to_string(),
            reason: None,
            error_code: None,
        },
        HookDecision::Deny { reason, error_code } => DebugHookResponse {
            decision: "deny".to_string(),
            reason: Some(reason),
            error_code: Some(error_code),
        },
    };

    Ok(Json(DataResponse::new(response)))
}

// ─── Active UI slots (public, no auth) ───────────────────────────────────────

pub async fn get_active_slots_handler(
    State(state): State<AppState>,
) -> HandlerResult<impl IntoResponse> {
    let slots = state.plugin_runtime.active_ui_slots().await;

    let mut grouped: HashMap<String, Vec<SlotEntry>> = HashMap::new();
    for slot in slots {
        grouped.entry(slot.slot_name).or_default().push(SlotEntry {
            plugin_slug: slot.plugin_slug,
            asset_url: slot.asset_url,
            custom_element_tag: slot.custom_element_tag,
            props: slot.props,
            load_order: slot.load_order,
        });
    }

    Ok(Json(DataResponse::new(ActiveSlotsResponse { slots: grouped })))
}
