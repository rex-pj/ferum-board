use std::io::Read;
use std::path::{Path, PathBuf};

use bytes::Bytes;
use ferum_application::shared::AppError;

const MAX_PACKAGE_SIZE: usize = 50 * 1024 * 1024; // 50 MB
const MAX_EXTRACTED_SIZE: u64 = 200 * 1024 * 1024; // 200 MB
const MAX_FILES: usize = 1000;

/// Extract a .fpkg (ZIP archive) into `plugins_dir/{slug}/`.
///
/// Returns the absolute path to the extracted plugin directory.
/// All security checks (path traversal, size limits, file count) are enforced here.
pub fn extract(
    package_bytes: &Bytes,
    plugins_dir: &Path,
    slug: &str,
) -> Result<PathBuf, AppError> {
    if package_bytes.len() > MAX_PACKAGE_SIZE {
        return Err(AppError::unprocessable(&format!(
            "Package too large. Maximum size is {}MB",
            MAX_PACKAGE_SIZE / 1024 / 1024
        )));
    }

    let cursor = std::io::Cursor::new(package_bytes.as_ref());
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| {
        AppError::unprocessable(&format!("Not a valid .fpkg ZIP archive: {}", e))
    })?;

    // Validate before extracting
    let file_count = archive.len();
    if file_count > MAX_FILES {
        return Err(AppError::unprocessable(&format!(
            "Package contains too many files ({} > {})",
            file_count, MAX_FILES
        )));
    }

    let mut total_extracted: u64 = 0;

    // Sanitize the slug for use as a directory name
    let safe_slug = sanitize_slug(slug);
    let plugin_dir = plugins_dir.join(&safe_slug);

    // Remove existing directory if re-installing
    if plugin_dir.exists() {
        std::fs::remove_dir_all(&plugin_dir).map_err(|e| {
            AppError::internal(format!("Failed to clean existing plugin dir: {}", e))
        })?;
    }
    std::fs::create_dir_all(&plugin_dir)
        .map_err(|e| AppError::internal(format!("Failed to create plugin dir: {}", e)))?;

    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|e| {
            AppError::internal(format!("Failed to read archive entry {}: {}", i, e))
        })?;

        let raw_path = file
            .enclosed_name()
            .ok_or_else(|| AppError::unprocessable("Archive contains unsafe path (path traversal attempt)"))?;

        // Reject absolute paths and path traversal
        if raw_path.components().any(|c| {
            matches!(
                c,
                std::path::Component::RootDir | std::path::Component::ParentDir
            )
        }) {
            return Err(AppError::unprocessable(
                "Archive contains path traversal (../) — rejected",
            ));
        }

        let target_path = plugin_dir.join(&raw_path);

        // Ensure the target path is still inside plugin_dir (double-check)
        if !target_path.starts_with(&plugin_dir) {
            return Err(AppError::unprocessable(
                "Archive entry escapes plugin directory",
            ));
        }

        if file.is_dir() {
            std::fs::create_dir_all(&target_path)
                .map_err(|e| AppError::internal(format!("Failed to create dir: {}", e)))?;
        } else {
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| AppError::internal(format!("Failed to create parent dir: {}", e)))?;
            }

            // Bound the actual decompressed read by the remaining size budget instead of
            // trusting the entry's declared uncompressed_size — that field is attacker
            // controlled and a crafted entry can understate it while a real DEFLATE
            // stream decompresses to far more (zip bomb), so read_to_end() alone would
            // be unbounded. Reading one byte past the budget lets us detect and reject
            // an oversized entry without ever materializing it in full.
            let remaining = MAX_EXTRACTED_SIZE.saturating_sub(total_extracted);
            let mut content = Vec::new();
            file.take(remaining + 1)
                .read_to_end(&mut content)
                .map_err(|e| AppError::internal(format!("Failed to read file: {}", e)))?;
            if content.len() as u64 > remaining {
                let _ = std::fs::remove_dir_all(&plugin_dir);
                return Err(AppError::unprocessable(&format!(
                    "Extracted size exceeds maximum ({}MB)",
                    MAX_EXTRACTED_SIZE / 1024 / 1024
                )));
            }
            total_extracted += content.len() as u64;

            std::fs::write(&target_path, &content)
                .map_err(|e| AppError::internal(format!("Failed to write file: {}", e)))?;
        }
    }

    // Verify plugin.toml exists after extraction
    if !plugin_dir.join("plugin.toml").exists() {
        let _ = std::fs::remove_dir_all(&plugin_dir);
        return Err(AppError::unprocessable(
            "Archive does not contain plugin.toml at root level",
        ));
    }

    Ok(plugin_dir)
}

/// Remove a plugin directory from disk. Called during uninstall.
pub fn remove_plugin_dir(install_path: &str) -> Result<(), AppError> {
    let path = Path::new(install_path);
    if path.exists() {
        std::fs::remove_dir_all(path)
            .map_err(|e| AppError::internal(format!("Failed to remove plugin dir: {}", e)))?;
    }
    Ok(())
}

/// Sanitize a plugin slug for use as a filesystem directory name.
/// Allows alphanumeric, dots, hyphens, underscores.
pub fn sanitize_slug(slug: &str) -> String {
    slug.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
