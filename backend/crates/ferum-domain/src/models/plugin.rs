use chrono::{DateTime, Utc};
use uuid::Uuid;

/// An escalating trust ladder, not interchangeable packaging formats: each tier
/// buys capability by giving up isolation.
#[derive(Clone, Debug, PartialEq)]
pub enum PluginTier {
    /// Declarative webhooks only; no code runs in this process.
    Manifest,
    /// JS in a `boa_engine` sandbox, own thread, hook timeout + circuit breaker.
    /// Needs the `script_plugins` feature, else reported as unsupported.
    Script,
    /// Out-of-process sidecar. Phase 3 — does not yet select a working runtime.
    Service,
}

impl PluginTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginTier::Manifest => "manifest",
            PluginTier::Script => "script",
            PluginTier::Service => "service",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PluginStatus {
    Installing,
    Active,
    Inactive,
    Error,
    Disabled,
    Uninstalling,
}

impl PluginStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginStatus::Installing => "installing",
            PluginStatus::Active => "active",
            PluginStatus::Inactive => "inactive",
            PluginStatus::Error => "error",
            PluginStatus::Disabled => "disabled",
            PluginStatus::Uninstalling => "uninstalling",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Plugin {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub version: String,
    pub tier: PluginTier,
    pub status: PluginStatus,
    /// Parsed plugin.toml stored as JSON
    pub manifest: serde_json::Value,
    /// Admin-set config values
    pub config: serde_json::Value,
    /// Capabilities granted by admin (subset of requested)
    pub granted_capabilities: serde_json::Value,
    /// Absolute path to extracted plugin directory
    pub install_path: String,
    /// PostgreSQL schema name owned by this plugin (Tier 3 only)
    pub db_schema_name: Option<String>,
    /// Current migration version applied
    pub db_schema_version: i32,
    pub installed_by: Option<Uuid>,
    pub installed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub activated_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    /// Last heartbeat from Tier 3 sidecar
    pub last_seen_at: Option<DateTime<Utc>>,
    pub restart_count: i32,
    pub circuit_open: bool,
}

#[derive(Clone, Debug)]
pub struct PluginHook {
    pub id: Uuid,
    pub plugin_id: Uuid,
    pub hook_name: String,
    pub priority: i32,
    pub is_active: bool,
    /// Rolling average hook latency in milliseconds
    pub avg_ms: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct PluginUiSlot {
    pub id: Uuid,
    pub plugin_id: Uuid,
    pub plugin_slug: String,
    pub slot_name: String,
    pub asset_url: String,
    /// Always [`ui_slot_element_tag`] of this row's slug and slot name — the
    /// repository derives it on read rather than trusting the stored column, so
    /// a row written under an older naming rule cannot go on addressing an
    /// element no bundle defines.
    pub custom_element_tag: String,
    pub props: Vec<String>,
    pub load_order: i32,
    pub is_active: bool,
}

/// The custom element name carrying a plugin's widget in a named slot.
///
/// **The slug is in the name, which is what makes a slot multi-occupancy.**
/// Naming it after the slot alone gave two plugins in `home_feed_top` identical
/// tags; both `customElements.define` calls raced and the winner rendered into
/// both elements — one widget twice, the other missing, no error anywhere.
///
/// The constant prefix guarantees the required hyphen and a leading letter, so
/// the output is valid even for a slug starting with a digit.
pub fn ui_slot_element_tag(plugin_slug: &str, slot_name: &str) -> String {
    fn sanitize(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for ch in s.chars() {
            if ch.is_ascii_alphanumeric() {
                out.extend(ch.to_lowercase());
            } else if !out.ends_with('-') {
                out.push('-');
            }
        }
        out.trim_matches('-').to_string()
    }

    format!(
        "ferum-slot-{}-{}",
        sanitize(plugin_slug),
        sanitize(slot_name)
    )
}

#[derive(Clone, Debug)]
pub struct PluginLog {
    pub id: Uuid,
    pub plugin_id: Uuid,
    pub level: String,
    pub hook_name: Option<String>,
    pub duration_ms: Option<i32>,
    pub message: String,
    pub context: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct NewPlugin {
    pub slug: String,
    pub name: String,
    pub version: String,
    pub tier: PluginTier,
    pub manifest: serde_json::Value,
    pub granted_capabilities: serde_json::Value,
    pub install_path: String,
    pub installed_by: Option<Uuid>,
}

#[derive(Debug)]
pub struct NewPluginHook {
    pub plugin_id: Uuid,
    pub hook_name: String,
    pub priority: i32,
}

#[derive(Debug)]
pub struct NewPluginUiSlot {
    pub plugin_id: Uuid,
    pub slot_name: String,
    pub asset_url: String,
    pub custom_element_tag: String,
    pub props: Vec<String>,
    pub load_order: i32,
}

#[derive(Debug)]
pub struct NewPluginLog {
    pub plugin_id: Uuid,
    pub level: String,
    pub hook_name: Option<String>,
    pub duration_ms: Option<i32>,
    pub message: String,
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Default)]
pub struct PluginLogQuery {
    pub level: Option<String>,
    pub hook_name: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub limit: u64,
    pub offset: u64,
}
