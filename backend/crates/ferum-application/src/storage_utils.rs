//! CAS key derivation and image validation shared by every upload path.

use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::ports::{ForumJob, JobQueue};
use ferum_domain::repositories::stored_file_repository::StoredFileRepository;

/// Gives back one reference to a CAS key, and schedules collection when that
/// was the last one.
///
/// **Use this rather than writing the decrement inline.** The two-step shape —
/// decrement, then enqueue only on zero — was copied into eight call sites, and
/// three of them got it wrong in the same way: they deleted the `stored_files`
/// row directly with `delete_by_key`, or forgot to release at all. Under
/// database storage that is invisible, because the row *is* the bytes; under
/// S3/GCS/R2 it strands the object in the bucket with nothing left pointing at
/// it, so it can never be found or counted again.
///
/// Errors are logged, never returned: failing an admin's "remove logo" because
/// a bookkeeping UPDATE failed would be a worse outcome than a leaked object,
/// and the reconciliation this needs is a separate sweep either way.
pub async fn release_cas_ref(
    stored_files: &Arc<dyn StoredFileRepository>,
    jobs: &Arc<dyn JobQueue>,
    key: &str,
) {
    match stored_files.decrement_ref(key).await {
        Ok(0) => {
            // `GcStorageKey` re-tests `ref_count = 0` inside its DELETE, so a
            // key revived between here and the job running is left alone —
            // bytes included. It must not decrement again; see the job's doc.
            if let Err(e) = jobs
                .enqueue(ForumJob::GcStorageKey {
                    key: key.to_string(),
                })
                .await
            {
                tracing::warn!(cas_key = %key, error = ?e, "could not enqueue CAS collection");
            }
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(cas_key = %key, error = ?e, "CAS decrement failed"),
    }
}

/// Generates a content-addressed storage key.
/// Format: `{prefix}/{sha256_hex_16}.{ext}`
pub fn cas_key(prefix: &str, data: &[u8], content_type: &str) -> String {
    let hash = Sha256::digest(data);
    let hex = hex::encode(&hash[..16]);
    let ext = content_type_to_ext(content_type);
    format!("{}/{}.{}", prefix, hex, ext)
}

fn content_type_to_ext(ct: &str) -> &'static str {
    match ct {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        "image/x-icon" | "image/vnd.microsoft.icon" => "ico",
        _ => "bin",
    }
}

pub fn validate_image_content_type(ct: &str) -> bool {
    matches!(
        ct,
        "image/jpeg" | "image/jpg" | "image/png" | "image/webp" | "image/gif"
    )
}

pub fn validate_favicon_content_type(ct: &str) -> bool {
    // SVG excluded: browsers may execute embedded scripts when served inline,
    // and an attacker with admin.config can achieve stored XSS via a crafted SVG.
    matches!(
        ct,
        "image/x-icon"
            | "image/vnd.microsoft.icon"
            | "image/png"
            | "image/gif"
            | "image/jpeg"
            | "image/jpg"
    )
}
