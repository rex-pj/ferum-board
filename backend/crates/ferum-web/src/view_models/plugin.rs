use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use ferum_domain::models::plugin::{Plugin, PluginLog};

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct PluginListItem {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub version: String,
    pub tier: String,
    pub status: String,
    pub error_message: Option<String>,
    pub installed_at: DateTime<Utc>,
    pub activated_at: Option<DateTime<Utc>>,
    pub circuit_open: bool,
}

impl From<&Plugin> for PluginListItem {
    fn from(p: &Plugin) -> Self {
        Self {
            id: p.id,
            slug: p.slug.clone(),
            name: p.name.clone(),
            version: p.version.clone(),
            tier: p.tier.as_str().to_string(),
            status: p.status.as_str().to_string(),
            error_message: p.error_message.clone(),
            installed_at: p.installed_at,
            activated_at: p.activated_at,
            circuit_open: p.circuit_open,
        }
    }
}

#[derive(Serialize)]
pub struct PluginDetailResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub version: String,
    pub tier: String,
    pub status: String,
    /// config_schema section from manifest (for admin UI to render config form)
    pub config_schema: serde_json::Value,
    /// Current admin-set config values
    pub config: serde_json::Value,
    /// Capabilities granted by admin
    pub granted_capabilities: serde_json::Value,
    pub error_message: Option<String>,
    pub installed_at: DateTime<Utc>,
    pub activated_at: Option<DateTime<Utc>>,
    pub circuit_open: bool,
    pub restart_count: i32,
}

impl From<&Plugin> for PluginDetailResponse {
    fn from(p: &Plugin) -> Self {
        let config_schema = p
            .manifest
            .get("config_schema")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        Self {
            id: p.id,
            slug: p.slug.clone(),
            name: p.name.clone(),
            version: p.version.clone(),
            tier: p.tier.as_str().to_string(),
            status: p.status.as_str().to_string(),
            config_schema,
            config: p.config.clone(),
            granted_capabilities: p.granted_capabilities.clone(),
            error_message: p.error_message.clone(),
            installed_at: p.installed_at,
            activated_at: p.activated_at,
            circuit_open: p.circuit_open,
            restart_count: p.restart_count,
        }
    }
}

/// Returned after upload validation, before admin grants capabilities.
#[derive(Serialize)]
pub struct CapabilityReviewResponse {
    /// Temporary slug from manifest (not yet in DB)
    pub slug: String,
    pub name: String,
    pub version: String,
    pub tier: String,
    pub description: Option<String>,
    pub author: Option<String>,
    /// Full capabilities section from manifest
    pub capabilities_requested: serde_json::Value,
}

#[derive(Serialize)]
pub struct PluginLogResponse {
    pub id: Uuid,
    pub level: String,
    pub hook_name: Option<String>,
    pub duration_ms: Option<i32>,
    pub message: String,
    pub context: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl From<PluginLog> for PluginLogResponse {
    fn from(l: PluginLog) -> Self {
        Self {
            id: l.id,
            level: l.level,
            hook_name: l.hook_name,
            duration_ms: l.duration_ms,
            message: l.message,
            context: l.context,
            created_at: l.created_at,
        }
    }
}

// ─── Request types ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ConfigurePluginRequest {
    pub config: serde_json::Value,
}

#[derive(Deserialize)]
pub struct TogglePluginStatusRequest {
    pub active: bool,
}

#[derive(Deserialize, Validate)]
pub struct UninstallPluginRequest {
    #[validate(length(min = 1, max = 100, message = "confirm_slug must be 1–100 characters"))]
    pub confirm_slug: String,
}

#[derive(Deserialize, Default, Validate)]
pub struct PluginLogQueryParams {
    #[validate(length(max = 20))]
    pub level: Option<String>,
    #[validate(length(max = 100))]
    pub hook_name: Option<String>,
    #[serde(default = "default_log_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
}

fn default_log_limit() -> u64 {
    100
}

#[derive(Deserialize, Validate)]
pub struct DebugHookRequest {
    #[validate(length(min = 1, max = 200, message = "hook name must be 1–200 characters"))]
    pub hook: String,
    pub payload: serde_json::Value,
}

#[derive(Serialize)]
pub struct DebugHookResponse {
    pub decision: String,
    pub reason: Option<String>,
    pub error_code: Option<String>,
}

// ─── Slot entry for frontend SSR ─────────────────────────────────────────────

#[derive(Serialize)]
pub struct ActiveSlotsResponse {
    /// Map of slot_name → Vec<slot entries>
    pub slots: std::collections::HashMap<String, Vec<SlotEntry>>,
}

/// Public-facing slot entry returned by the unauthenticated `/api/plugins/active-slots`
/// endpoint. Does NOT include `plugin_slug` or `asset_url` — those fields would reveal
/// which plugins are installed, helping attackers target known CVEs.
#[derive(Serialize)]
pub struct SlotEntry {
    pub custom_element_tag: String,
    pub props: Vec<String>,
    pub load_order: i32,
}

/// Admin-facing UI slot row — includes everything needed to build a placement editor.
#[derive(Serialize)]
pub struct UiSlotAdminItem {
    pub id: Uuid,
    pub slot_name: String,
    pub custom_element_tag: String,
    pub load_order: i32,
    pub is_active: bool,
}

impl From<ferum_domain::models::plugin::PluginUiSlot> for UiSlotAdminItem {
    fn from(s: ferum_domain::models::plugin::PluginUiSlot) -> Self {
        Self {
            id: s.id,
            slot_name: s.slot_name,
            custom_element_tag: s.custom_element_tag,
            load_order: s.load_order,
            is_active: s.is_active,
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateUiSlotRequest {
    pub slot_name: String,
    #[serde(default)]
    pub load_order: i32,
}

