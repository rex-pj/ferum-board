use std::path::Path;

use ferum_application::shared::AppError;
use ferum_domain::models::plugin::PluginTier;

/// Parsed representation of a plugin.toml manifest.
#[derive(Debug, Clone)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub min_ferum: Option<String>,
    pub tier: PluginTier,
    pub description: Option<String>,
    pub author: Option<String>,
    /// Full raw manifest as serde_json::Value (stored in DB).
    pub raw: serde_json::Value,
}

/// Load and parse a plugin.toml from the given plugin directory.
pub fn load_from_dir(plugin_dir: &Path) -> Result<PluginManifest, AppError> {
    let manifest_path = plugin_dir.join("plugin.toml");

    let content = std::fs::read_to_string(&manifest_path).map_err(|e| {
        AppError::unprocessable(&format!("Cannot read plugin.toml: {}", e))
    })?;

    parse_manifest_str(&content)
}

/// Parse a plugin.toml string into a PluginManifest.
pub fn parse_manifest_str(content: &str) -> Result<PluginManifest, AppError> {
    let table: toml::Value = content
        .parse()
        .map_err(|e| AppError::unprocessable(&format!("Invalid plugin.toml TOML: {}", e)))?;

    // Convert to JSON for storage and flexible access
    let raw: serde_json::Value = serde_json::to_value(&table)
        .map_err(|e| AppError::internal(format!("TOML→JSON conversion failed: {}", e)))?;

    let meta = table
        .get("meta")
        .ok_or_else(|| AppError::unprocessable("plugin.toml missing [meta] section"))?;

    let id = meta
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::unprocessable("plugin.toml missing meta.id"))?
        .to_string();

    validate_plugin_id(&id)?;

    let name = meta
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::unprocessable("plugin.toml missing meta.name"))?
        .to_string();

    let version = meta
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::unprocessable("plugin.toml missing meta.version"))?
        .to_string();

    let tier_str = meta
        .get("tier")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::unprocessable("plugin.toml missing meta.tier"))?;

    let tier = match tier_str {
        "manifest" => PluginTier::Manifest,
        "script" => PluginTier::Script,
        "service" => PluginTier::Service,
        other => {
            return Err(AppError::unprocessable(&format!(
                "Unknown plugin tier '{}'. Must be manifest, script, or service",
                other
            )))
        }
    };

    let min_ferum = meta
        .get("min_ferum")
        .and_then(|v| v.as_str())
        .map(String::from);

    let description = meta
        .get("description")
        .and_then(|v| v.as_str())
        .map(String::from);

    let author = meta.get("author").and_then(|v| v.as_str()).map(String::from);

    Ok(PluginManifest {
        id,
        name,
        version,
        min_ferum,
        tier,
        description,
        author,
        raw,
    })
}

/// Validate that the plugin ID follows reverse-domain notation.
/// e.g. "com.example.my-plugin" — alphanumeric, dots, hyphens only.
pub fn validate_plugin_id(id: &str) -> Result<(), AppError> {
    if id.is_empty() || id.len() > 256 {
        return Err(AppError::unprocessable(
            "Plugin ID must be between 1 and 256 characters",
        ));
    }

    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        return Err(AppError::unprocessable(
            "Plugin ID must contain only alphanumeric characters, dots, hyphens, and underscores",
        ));
    }

    if id.starts_with('.') || id.ends_with('.') {
        return Err(AppError::unprocessable(
            "Plugin ID must not start or end with a dot",
        ));
    }

    Ok(())
}
