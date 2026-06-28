use std::io::Write as _;

use bytes::Bytes;
use ferum_infrastructure::plugins::package_extractor::{extract, sanitize_slug};

fn temp_plugins_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ferum_pkg_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_zip(files: &[(&str, &[u8])]) -> Bytes {
    let cursor = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);
    for (name, content) in files {
        zip.start_file(*name, opts).unwrap();
        zip.write_all(content).unwrap();
    }
    Bytes::from(zip.finish().unwrap().into_inner())
}

// ─── sanitize_slug ────────────────────────────────────────────────────────────

#[test]
fn sanitize_slug_preserves_safe_chars() {
    assert_eq!(sanitize_slug("my-plugin.v1_beta"), "my-plugin.v1_beta");
    assert_eq!(sanitize_slug("com.example.foo"), "com.example.foo");
}

#[test]
fn sanitize_slug_replaces_slashes_and_spaces() {
    assert_eq!(sanitize_slug("my/plugin name"), "my_plugin_name");
}

#[test]
fn sanitize_slug_makes_path_traversal_chars_safe() {
    assert_eq!(sanitize_slug("../../etc"), ".._.._etc");
}

// ─── extract ──────────────────────────────────────────────────────────────────

#[test]
fn extract_package_too_large_returns_error() {
    // MAX_PACKAGE_SIZE is 50 MB (50 * 1024 * 1024)
    let huge = Bytes::from(vec![0u8; 50 * 1024 * 1024 + 1]);
    let dir = temp_plugins_dir();
    let result = extract(&huge, &dir, "test-plugin");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_err());
    let msg = format!("{:?}", result.unwrap_err());
    assert!(msg.contains("50"), "error should mention the 50 MB limit");
}

#[test]
fn extract_invalid_zip_bytes_returns_error() {
    let garbage = Bytes::from(b"this is not a zip archive".as_slice());
    let dir = temp_plugins_dir();
    let result = extract(&garbage, &dir, "test-plugin");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_err());
}

#[test]
fn extract_zip_without_plugin_toml_returns_error() {
    let pkg = make_zip(&[("README.md", b"hello")]);
    let dir = temp_plugins_dir();
    let result = extract(&pkg, &dir, "test-plugin");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_err());
    let msg = format!("{:?}", result.unwrap_err());
    assert!(msg.contains("plugin.toml"));
}

#[test]
fn extract_valid_zip_with_plugin_toml_succeeds() {
    let plugin_toml = b"[meta]\nid = \"com.example.test\"\nname = \"T\"\nversion = \"1.0\"\ntier = \"manifest\"\n";
    let pkg = make_zip(&[
        ("plugin.toml", plugin_toml),
        ("README.md", b"# Test Plugin"),
    ]);
    let dir = temp_plugins_dir();
    let result = extract(&pkg, &dir, "valid-plugin");
    let plugin_dir = dir.join("valid-plugin");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
    assert_eq!(result.unwrap(), plugin_dir);
}

#[test]
fn extract_zip_with_nested_dirs_succeeds() {
    let pkg = make_zip(&[
        ("plugin.toml", b"[meta]\nid=\"x\"\nname=\"x\"\nversion=\"1\"\ntier=\"manifest\"\n"),
        ("assets/icon.png", b"PNG"),
        ("assets/styles/main.css", b"body{}"),
    ]);
    let dir = temp_plugins_dir();
    let result = extract(&pkg, &dir, "nested-plugin");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
}
