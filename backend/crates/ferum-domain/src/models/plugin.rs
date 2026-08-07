use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
pub enum PluginTier {
    Manifest,
    Script,
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

/// The custom element name that carries a plugin's widget in a named slot.
///
/// **The plugin slug is part of the name, and that is the whole point.** The tag
/// used to come from the slot name alone, which made a slot single-occupancy in
/// a way nothing announced: the server emits one element per slot *row*, so two
/// plugins in `home_feed_top` produced two identical `<ferum-slot-home-feed-top>`
/// tags, both `customElements.define` calls raced for the one name, and whichever
/// registered first then rendered into **both** elements. The observable symptom
/// was one plugin's widget appearing twice and the other's not at all — with no
/// error anywhere, because nothing in that sequence is a failure.
///
/// With the slug in the name each plugin owns its own element, so several
/// plugins share a slot and render in `load_order` sequence.
///
/// The output is a valid custom element name for any slug: lowercased, every
/// character outside `[a-z0-9]` folded to a single `-`, and the constant prefix
/// guarantees both the required hyphen and a leading letter — which a slug
/// starting with a digit would otherwise violate.
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

// ─── Input DTOs ───────────────────────────────────────────────────────────────

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
