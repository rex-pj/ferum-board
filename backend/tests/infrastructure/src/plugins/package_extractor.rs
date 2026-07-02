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

#[test]
fn extract_zip_with_parent_dir_traversal_entry_is_rejected() {
    let pkg = make_zip(&[
        ("plugin.toml", b"[meta]\nid=\"x\"\nname=\"x\"\nversion=\"1\"\ntier=\"manifest\"\n"),
        ("../../evil.txt", b"pwned"),
    ]);
    let dir = temp_plugins_dir();
    let result = extract(&pkg, &dir, "traversal-plugin");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_err(), "path traversal entry must be rejected");
    // The escaped file must never land outside the plugins temp dir.
    assert!(!dir.parent().unwrap().join("evil.txt").exists());
}

#[test]
fn extract_zip_with_absolute_path_entry_is_rejected() {
    let pkg = make_zip(&[
        ("plugin.toml", b"[meta]\nid=\"x\"\nname=\"x\"\nversion=\"1\"\ntier=\"manifest\"\n"),
        ("/etc/passwd", b"pwned"),
    ]);
    let dir = temp_plugins_dir();
    let result = extract(&pkg, &dir, "abs-path-plugin");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_err(), "absolute-path entry must be rejected");
}

#[test]
fn extract_zip_exceeding_max_file_count_is_rejected() {
    // MAX_FILES is 1000 — build an archive with 1001 tiny entries.
    let mut files: Vec<(String, Vec<u8>)> = (0..1001)
        .map(|i| (format!("file_{i}.txt"), b"x".to_vec()))
        .collect();
    files.push(("plugin.toml".to_string(), b"[meta]\nid=\"x\"\nname=\"x\"\nversion=\"1\"\ntier=\"manifest\"\n".to_vec()));
    let refs: Vec<(&str, &[u8])> = files.iter().map(|(n, c)| (n.as_str(), c.as_slice())).collect();
    let pkg = make_zip(&refs);

    let dir = temp_plugins_dir();
    let result = extract(&pkg, &dir, "too-many-files-plugin");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_err(), "archive with >1000 entries must be rejected");
    let msg = format!("{:?}", result.unwrap_err());
    assert!(msg.contains("too many files"));
}

#[test]
fn extract_entry_decompressing_past_extracted_size_cap_is_rejected() {
    // MAX_EXTRACTED_SIZE is 200MB. A single highly-compressible (all-zero) entry just
    // over that, Deflate-compressed, keeps the on-disk package tiny (< 50MB package
    // cap) while forcing the extractor's bounded-read path to trip on real decompressed
    // bytes rather than trusting the entry's declared uncompressed_size metadata.
    let cursor = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let stored = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);

    zip.start_file("plugin.toml", stored).unwrap();
    zip.write_all(b"[meta]\nid=\"x\"\nname=\"x\"\nversion=\"1\"\ntier=\"manifest\"\n").unwrap();

    zip.start_file("huge.bin", deflated).unwrap();
    let chunk = vec![0u8; 1024 * 1024]; // 1MB of zeros, written repeatedly
    for _ in 0..(200 + 4) {
        zip.write_all(&chunk).unwrap();
    }
    let pkg = Bytes::from(zip.finish().unwrap().into_inner());

    assert!(
        pkg.len() < 50 * 1024 * 1024,
        "test package should compress well under the 50MB package cap, got {} bytes",
        pkg.len()
    );

    let dir = temp_plugins_dir();
    let result = extract(&pkg, &dir, "zip-bomb-plugin");
    let plugin_dir = dir.join("zip-bomb-plugin");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_err(), "entry exceeding the extracted-size cap must be rejected");
    let msg = format!("{:?}", result.unwrap_err());
    assert!(msg.contains("Extracted size exceeds maximum"));
    // Partial extraction must be cleaned up, not left on disk.
    assert!(!plugin_dir.exists());
}
