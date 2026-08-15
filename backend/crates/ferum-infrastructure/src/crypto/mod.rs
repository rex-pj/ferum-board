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

/// Additional authenticated data for one secret field in `plugins.config`.
///
/// Binds the ciphertext to **both** the plugin and the field. Binding only the
/// field would let an operator with database access move a sealed credential
/// between two installs of the same plugin — or between two plugins that happen
/// to name a config key `api_key` — and have it decrypt cleanly into a context
/// its owner never granted.
///
/// The slug is used rather than the plugin's UUID so the value survives an
/// uninstall/reinstall cycle, which mints a new id for what the operator
/// reasonably considers the same plugin.
pub fn plugin_config_aad(plugin_slug: &str, key: &str) -> Vec<u8> {
    format!("plugins.config:{plugin_slug}:{key}").into_bytes()
}
