use sha2::{Digest, Sha256};

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
