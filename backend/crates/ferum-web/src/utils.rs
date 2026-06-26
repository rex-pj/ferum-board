use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use axum::http::HeaderMap;

/// Tạo fingerprint 16 ký tự hex cho guest viewer dựa trên IP + User-Agent.
///
/// - IP: đọc từ `X-Forwarded-For` (Nginx) hoặc `X-Real-IP`.
/// - User-Agent: phân biệt các browser khác nhau trên cùng IP (VD: văn phòng dùng NAT).
/// - Dùng std DefaultHasher — không cần crypto, chỉ cần đủ phân tán.
/// - Trả về `None` khi cả IP lẫn UA đều trống (không đủ thông tin để dedup).
pub fn guest_fingerprint(headers: &HeaderMap) -> Option<String> {
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(str::trim)
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
        })
        .unwrap_or("");

    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if ip.is_empty() && ua.is_empty() {
        return None;
    }

    let mut hasher = DefaultHasher::new();
    ip.hash(&mut hasher);
    ua.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}
