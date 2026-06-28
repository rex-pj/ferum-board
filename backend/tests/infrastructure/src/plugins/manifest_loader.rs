use ferum_domain::models::plugin::PluginTier;
use ferum_infrastructure::plugins::manifest_loader::{parse_manifest_str, validate_plugin_id};

fn valid_toml(tier: &str) -> String {
    format!(
        r#"
[meta]
id = "com.example.test-plugin"
name = "Test Plugin"
version = "1.0.0"
tier = "{tier}"
description = "A test plugin"
author = "Test Author"
min_ferum = "0.1.0"
"#
    )
}

// ─── parse_manifest_str ───────────────────────────────────────────────────────

#[test]
fn parses_valid_manifest_tier_manifest() {
    let m = parse_manifest_str(&valid_toml("manifest")).unwrap();
    assert_eq!(m.id, "com.example.test-plugin");
    assert_eq!(m.name, "Test Plugin");
    assert_eq!(m.version, "1.0.0");
    assert!(matches!(m.tier, PluginTier::Manifest));
    assert_eq!(m.description.as_deref(), Some("A test plugin"));
    assert_eq!(m.author.as_deref(), Some("Test Author"));
    assert_eq!(m.min_ferum.as_deref(), Some("0.1.0"));
}

#[test]
fn parses_valid_manifest_tier_script() {
    let m = parse_manifest_str(&valid_toml("script")).unwrap();
    assert!(matches!(m.tier, PluginTier::Script));
}

#[test]
fn parses_valid_manifest_tier_service() {
    let m = parse_manifest_str(&valid_toml("service")).unwrap();
    assert!(matches!(m.tier, PluginTier::Service));
}

#[test]
fn optional_fields_absent_become_none() {
    let toml = r#"
[meta]
id = "com.example.minimal"
name = "Minimal"
version = "0.1.0"
tier = "manifest"
"#;
    let m = parse_manifest_str(toml).unwrap();
    assert!(m.description.is_none());
    assert!(m.author.is_none());
    assert!(m.min_ferum.is_none());
}

#[test]
fn raw_json_is_populated() {
    let m = parse_manifest_str(&valid_toml("manifest")).unwrap();
    assert!(m.raw.is_object());
    assert!(m.raw["meta"]["id"].is_string());
}

#[test]
fn missing_meta_section_returns_error() {
    let toml = r#"
[plugin]
id = "com.example.test"
"#;
    assert!(parse_manifest_str(toml).is_err());
}

#[test]
fn missing_id_returns_error() {
    let toml = r#"
[meta]
name = "No ID Plugin"
version = "1.0.0"
tier = "manifest"
"#;
    assert!(parse_manifest_str(toml).is_err());
}

#[test]
fn missing_name_returns_error() {
    let toml = r#"
[meta]
id = "com.example.test"
version = "1.0.0"
tier = "manifest"
"#;
    assert!(parse_manifest_str(toml).is_err());
}

#[test]
fn missing_version_returns_error() {
    let toml = r#"
[meta]
id = "com.example.test"
name = "Test"
tier = "manifest"
"#;
    assert!(parse_manifest_str(toml).is_err());
}

#[test]
fn missing_tier_returns_error() {
    let toml = r#"
[meta]
id = "com.example.test"
name = "Test"
version = "1.0.0"
"#;
    assert!(parse_manifest_str(toml).is_err());
}

#[test]
fn unknown_tier_returns_error() {
    let toml = r#"
[meta]
id = "com.example.test"
name = "Test"
version = "1.0.0"
tier = "wasm"
"#;
    assert!(parse_manifest_str(toml).is_err());
}

#[test]
fn invalid_toml_syntax_returns_error() {
    assert!(parse_manifest_str("not valid { toml !!!").is_err());
}

// ─── validate_plugin_id ───────────────────────────────────────────────────────

#[test]
fn valid_ids_pass_validation() {
    assert!(validate_plugin_id("com.example.my-plugin").is_ok());
    assert!(validate_plugin_id("org.ferum.core").is_ok());
    assert!(validate_plugin_id("simple").is_ok());
    assert!(validate_plugin_id("with-hyphen_and.dots123").is_ok());
}

#[test]
fn empty_id_is_rejected() {
    assert!(validate_plugin_id("").is_err());
}

#[test]
fn id_starting_with_dot_is_rejected() {
    assert!(validate_plugin_id(".com.example").is_err());
}

#[test]
fn id_ending_with_dot_is_rejected() {
    assert!(validate_plugin_id("com.example.").is_err());
}

#[test]
fn id_with_spaces_is_rejected() {
    assert!(validate_plugin_id("com example plugin").is_err());
}

#[test]
fn id_with_slash_is_rejected() {
    assert!(validate_plugin_id("com/example").is_err());
}

#[test]
fn id_over_256_chars_is_rejected() {
    let long_id = "a".repeat(257);
    assert!(validate_plugin_id(&long_id).is_err());
}

#[test]
fn id_of_exactly_256_chars_is_allowed() {
    let max_id = "a".repeat(256);
    assert!(validate_plugin_id(&max_id).is_ok());
}
