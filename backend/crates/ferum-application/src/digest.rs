//! Short hex fingerprints of a SHA-256 digest.
//!
//! One definition because three call sites derived the same value three ways —
//! `hex::encode(&digest[..16])` in two of them, `&hex[..12]` in the third. Each
//! was provably in bounds and each still read as a slice that a reader has to
//! check. Truncating the hex *string* is total, so `indexing_slicing` stays on
//! with no per-site exception.

use sha2::{Digest, Sha256};

/// Lowercase hex of `SHA-256(data)`, cut to `hex_chars` characters.
///
/// Two hex characters are one digest byte, so 32 characters is the first 16
/// bytes. A `hex_chars` above 64 yields the whole digest rather than panicking.
pub fn short_hex(data: &[u8], hex_chars: usize) -> String {
    hex::encode(Sha256::digest(data))
        .chars()
        .take(hex_chars)
        .collect()
}
