use std::collections::HashMap;
use std::path::PathBuf;

use axum::extract::{Extension, Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use validator::Validate;

use crate::app_state::AppState;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::plugin::{
    ActiveSlotsResponse, CapabilityReviewResponse, ConfigurePluginRequest, DebugHookRequest,
    DebugHookResponse, PluginDetailResponse, PluginListItem, PluginLogQueryParams,
    PluginLogResponse, SlotEntry, TogglePluginStatusRequest, UiSlotAdminItem,
    UninstallPluginRequest, UpdateUiSlotRequest,
};
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::ports::{HookContext, HookDecision};
use ferum_application::constants::MAX_PLUGIN_PACKAGE_BYTES;
use ferum_application::shared::AppError;
use ferum_domain::models::plugin::PluginLogQuery;
use ferum_infrastructure::plugins::{manifest_loader, package_extractor};

pub async fn list_plugins(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let plugins = state.plugin.list(actor).await?;
    let items: Vec<PluginListItem> = plugins.iter().map(PluginListItem::from).collect();
    Ok(Json(DataResponse::new(items)))
}

pub async fn get_plugin(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let plugin = state.plugin.get_by_slug(actor, &slug).await?;
    Ok(Json(DataResponse::new(PluginDetailResponse::from(&plugin))))
}

pub async fn upload_plugin(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    ferum_application::permission::PermissionChecker::can_manage_plugins(actor)?;

    let (pkg_bytes, _content_type) = crate::utils::read_file_field(&mut multipart, "file").await?;
    if pkg_bytes.len() > MAX_PLUGIN_PACKAGE_BYTES {
        return Err(AppError::invalid_with(
            "package_too_large",
            [(
                "limit_mb",
                (MAX_PLUGIN_PACKAGE_BYTES / (1024 * 1024)).into(),
            )],
        )
        .into());
    }

    let plugins_dir = PathBuf::from(&state.plugins_dir);
    let tmp_slug = format!("__tmp_upload_{}", uuid::Uuid::new_v4().simple());
    let extracted_path = package_extractor::extract(&pkg_bytes, &plugins_dir, &tmp_slug)
        .inspect_err(|_e| { let _ = package_extractor::remove_plugin_dir(&plugins_dir.join(&tmp_slug).to_string_lossy()); })?;

    let manifest = manifest_loader::load_from_dir(&extracted_path)
        .inspect_err(|_e| { let _ = package_extractor::remove_plugin_dir(&extracted_path.to_string_lossy()); })?;

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

pub async fn install_plugin(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    mut multipart: Multipart,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
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
                    .map_err(|_| AppError::invalid("invalid_granted_capabilities"))?;
            }
            _ => {}
        }
    }

    let pkg_bytes = pkg_bytes.ok_or_else(|| AppError::invalid("file_field_missing"))?;

    let plugins_dir = PathBuf::from(&state.plugins_dir);

    let manifest = {
        let tmp_slug = format!("__tmp_install_{}", uuid::Uuid::new_v4().simple());
        let extracted_path = package_extractor::extract(&pkg_bytes, &plugins_dir, &tmp_slug)?;
        let m = manifest_loader::load_from_dir(&extracted_path)?;
        let _ = package_extractor::remove_plugin_dir(&extracted_path.to_string_lossy());
        m
    };

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
        .inspect_err(|_e| {
            let _ = package_extractor::remove_plugin_dir(&install_path.to_string_lossy());
        })?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(PluginDetailResponse::from(&plugin))),
    ))
}

pub async fn configure_plugin(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Json(body): Json<ConfigurePluginRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.plugin.configure(actor, &slug, body.config).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_ui_slots(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let slots = state.plugin.list_ui_slots(actor, &slug).await?;
    let items: Vec<UiSlotAdminItem> = slots.into_iter().map(UiSlotAdminItem::from).collect();
    Ok(Json(DataResponse::new(items)))
}

pub async fn update_ui_slot(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path((slug, slot_id)): Path<(String, uuid::Uuid)>,
    Json(body): Json<UpdateUiSlotRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let slot = state
        .plugin
        .update_ui_slot_placement(actor, &slug, slot_id, body.slot_name, body.load_order)
        .await?;
    Ok(Json(DataResponse::new(UiSlotAdminItem::from(slot))))
}

pub async fn toggle_status(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Json(body): Json<TogglePluginStatusRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
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

pub async fn uninstall_plugin(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Json(body): Json<UninstallPluginRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let actor = auth_user.require_auth()?;
    state
        .plugin
        .uninstall(actor, &slug, &body.confirm_slug)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_logs(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path(slug): Path<String>,
    Query(params): Query<PluginLogQueryParams>,
) -> HandlerResult<impl IntoResponse> {
    // Enforces the declared caps, including `limit`'s upper bound — without this
    // a single request could ask for the entire log table.
    params
        .validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let actor = auth_user.require_auth()?;
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

#[cfg(debug_assertions)]
pub async fn debug_hook(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Json(body): Json<DebugHookRequest>,
) -> HandlerResult<impl IntoResponse> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let actor = auth_user.require_auth()?;
    ferum_application::permission::PermissionChecker::can_manage_plugins(actor)?;

    let ctx = HookContext {
        hook_name: body.hook.clone(),
        actor_id: Some(actor.id),
        actor_trust_level: format!("{:?}", actor.trust_level).to_lowercase(),
        payload: body.payload,
    };

    let decision = state
        .plugin_hooks
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

/// GET /api/plugins/active-slots — public endpoint for frontend slot injection.
///
/// Returns only `custom_element_tag`, `props`, and `load_order`.
/// `plugin_slug` and `asset_url` are intentionally excluded: exposing them to
/// unauthenticated callers leaks which plugins are installed, enabling targeted CVE searches.
pub async fn get_active_slots(
    State(state): State<AppState>,
) -> HandlerResult<impl IntoResponse> {
    let slots = state.plugin_ui.active_ui_slots().await;

    let mut grouped: HashMap<String, Vec<SlotEntry>> = HashMap::new();
    for slot in slots {
        grouped.entry(slot.slot_name).or_default().push(SlotEntry {
            custom_element_tag: slot.custom_element_tag,
            props: slot.props,
            load_order: slot.load_order,
        });
    }

    Ok(Json(DataResponse::new(ActiveSlotsResponse { slots: grouped })))
}

