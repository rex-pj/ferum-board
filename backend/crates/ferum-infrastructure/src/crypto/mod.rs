//! Encryption at rest for the secrets this application stores in PostgreSQL.
//!
//! A concrete infrastructure type rather than a port trait: there is exactly one
//! implementation and only infrastructure adapters use it, so a trait would be
//! ceremony with no second implementor to justify it. It also keeps a cipher out
//! of `ferum-domain`, whose "no I/O, no frameworks" rule is enforced by
//! feature-gating rather than merely intended.

pub mod secret_cipher;

pub use secret_cipher::{Opened, SecretCipher, SEALED_PREFIX};

use ferum_application::constants::SMTP_PASS_KEY;

/// `site_config` keys stored encrypted when a cipher is configured.
///
/// **Adding a key here is safe**: an existing plaintext row reads back unchanged
/// and is sealed on its next write, or by the startup sweep. **Removing one is
/// not** — already-sealed rows would then be handed to callers as literal
/// `enc:v1:…` text, and whatever consumed them would use that as the value.
///
/// Lives here rather than in the repository because three places must agree on
/// it: the repository that seals and opens, the startup sweep that converts
/// existing rows, and the startup self-check that proves the key matches.
pub const ENCRYPTED_CONFIG_KEYS: &[&str] = &[SMTP_PASS_KEY];

/// Additional authenticated data for a `site_config` value.
///
/// Binds the ciphertext to the key it belongs to, so a sealed value copied into a
/// different row fails authentication rather than decrypting. Not secret, not
/// stored — reconstructed from the key on both sides.
pub fn site_config_aad(key: &str) -> Vec<u8> {
    format!("site_config:{key}").into_bytes()
}

/// Additional authenticated data for `webhooks.secret`.
///
/// Distinct from every `site_config` AAD, so a secret lifted from one table
/// cannot be decrypted as a value of the other.
pub const WEBHOOK_SECRET_AAD: &[u8] = b"webhooks.secret";
