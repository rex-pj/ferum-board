use chacha20poly1305::aead::{Aead, Generate, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};

use ferum_application::shared::AppError;

/// Marks a value as sealed, and pins the envelope version.
///
/// An ASCII sentinel no real SMTP password or webhook secret begins with, which
/// is what lets [`SecretCipher::is_sealed`] be a `starts_with` rather than a
/// parse. The version lives *in the prefix* so a future `enc:v2:` — a different
/// algorithm, or a rotation epoch — can coexist with v1 row by row, with no data
/// migration and no ambiguity about which key opened which value.
///
/// `pub` because a SQL query needs it: the startup check for "does this database
/// hold sealed data" filters with `LIKE 'enc:v1:%'` rather than loading every row.
pub const SEALED_PREFIX: &str = "enc:v1:";

/// Length of the hex-encoded key an operator supplies: 32 bytes.
const KEY_HEX_LEN: usize = 64;

/// XChaCha20's nonce, in bytes. 24 rather than ChaCha20's 12 — the whole reason
/// random nonces need no counter and no birthday-bound reasoning here.
const NONCE_LEN: usize = 24;

/// A decrypted value, plus whether it should be written back under the current
/// key.
pub struct Opened {
    pub value: String,
    /// True when the value arrived as plaintext, or opened only under the
    /// *previous* key. Either way the caller may re-seal it; nothing breaks if it
    /// does not, which is what makes the migration lazy rather than a flag day.
    pub needs_reseal: bool,
}

/// Authenticated encryption for the secrets this application stores in Postgres.
///
/// Protects exactly one scenario: the database is read without the process
/// environment — a dump, a backup, a leaked replica. It does NOT protect against
/// an attacker on the host, where the key is readable.
///
/// AEAD specifically, because one protected value is an HMAC key: an
/// unauthenticated cipher would yield 32 bytes of garbage that the webhook
/// dispatcher would then sign with, failing silently at every subscriber.
pub struct SecretCipher {
    current: XChaCha20Poly1305,
    /// Opened-with-only-this reports `needs_reseal`, which the startup sweep acts
    /// on. That is the entire rotation mechanism: no key id in the envelope, no
    /// second format, and it reuses machinery the plaintext migration needs
    /// anyway.
    previous: Option<XChaCha20Poly1305>,
}

impl SecretCipher {
    /// Builds a cipher from hex-encoded key material.
    ///
    /// `previous` is the key being rotated *out*: values are opened with
    /// `current` first, then `previous`, and anything that needed the second is
    /// flagged for re-sealing.
    pub fn from_hex(current: &str, previous: Option<&str>) -> Result<Self, AppError> {
        Ok(Self {
            current: Self::parse_key(current, "SECRET_ENCRYPTION_KEY")?,
            previous: match previous.map(str::trim).filter(|k| !k.is_empty()) {
                Some(k) => Some(Self::parse_key(k, "SECRET_ENCRYPTION_KEY_PREVIOUS")?),
                None => None,
            },
        })
    }

    fn parse_key(hex_key: &str, var: &str) -> Result<XChaCha20Poly1305, AppError> {
        let hex_key = hex_key.trim();
        if hex_key.len() != KEY_HEX_LEN {
            // The length and the variable name, never the value: this message
            // reaches the startup log.
            return Err(AppError::internal(format!(
                "{var} must be {KEY_HEX_LEN} hex characters (32 bytes); got {} characters. \
                 Generate one with `openssl rand -hex 32`.",
                hex_key.len()
            )));
        }
        let bytes = hex::decode(hex_key).map_err(|_| {
            AppError::internal(format!(
                "{var} is not valid hex. Generate one with `openssl rand -hex 32`."
            ))
        })?;
        // The length check above already guarantees 32 bytes, so this cannot fail;
        // `TryFrom` rather than the deprecated `from_slice`, which panicked.
        let key = Key::try_from(bytes.as_slice())
            .map_err(|_| AppError::internal(format!("{var} is not a 32-byte key")))?;
        Ok(XChaCha20Poly1305::new(&key))
    }

    /// True when `stored` is a v1 envelope. Associated rather than a method: the
    /// startup sweep needs to ask this about rows it has not got a key for.
    pub fn is_sealed(stored: &str) -> bool {
        stored.starts_with(SEALED_PREFIX)
    }

    /// Encrypts `plaintext`, binding it to `aad`.
    ///
    /// `aad` names *where the value lives* (`b"site_config:smtp_pass"`,
    /// `b"webhooks.secret"`). It is not secret and is not stored; it is
    /// re-supplied on open, so a ciphertext moved to a different column fails
    /// authentication rather than decrypting. Free, and it closes a whole class of
    /// database-level tampering — swapping a webhook's secret for a copy of one
    /// from another row, for instance.
    pub fn seal(&self, aad: &[u8], plaintext: &str) -> Result<String, AppError> {
        // `try_generate`, not `generate`: the latter panics if the OS RNG fails,
        // and this runs on a request path (saving the settings page).
        let nonce = XNonce::try_generate()
            .map_err(|e| AppError::internal(format!("system RNG unavailable: {e}")))?;
        let ciphertext = self
            .current
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext.as_bytes(),
                    aad,
                },
            )
            // Deliberately says nothing about the value.
            .map_err(|_| AppError::internal("failed to encrypt a secret"))?;

        let mut envelope = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        envelope.extend_from_slice(nonce.as_slice());
        envelope.extend_from_slice(&ciphertext);
        // hex, not base64: `hex` is already a workspace dependency and already
        // used here for HMAC digests, while `base64` appears in the lock only
        // transitively and in two different majors. The 2x size costs nothing on a
        // value this small.
        Ok(format!("{SEALED_PREFIX}{}", hex::encode(envelope)))
    }

    /// Decrypts a v1 envelope, or passes plaintext through unchanged.
    ///
    /// A value with no recognised prefix is returned as-is with
    /// `needs_reseal = true`. That is what makes enabling encryption on a running
    /// forum a no-op at read time: existing rows keep working, and each becomes
    /// sealed the first time it is written (or when the startup sweep reaches it).
    pub fn open(&self, aad: &[u8], stored: &str) -> Result<Opened, AppError> {
        let Some(hex_body) = stored.strip_prefix(SEALED_PREFIX) else {
            return Ok(Opened {
                value: stored.to_string(),
                needs_reseal: true,
            });
        };

        let envelope = hex::decode(hex_body)
            .map_err(|_| AppError::internal("a sealed secret is not valid hex"))?;
        if envelope.len() <= NONCE_LEN {
            return Err(AppError::internal("a sealed secret is truncated"));
        }
        let (nonce, ciphertext) = envelope.split_at(NONCE_LEN);
        let nonce = XNonce::try_from(nonce)
            .map_err(|_| AppError::internal("a sealed secret has a malformed nonce"))?;

        // Current key first, then the one being rotated out. Anything that only
        // opened under `previous` is reported so the caller can re-seal it.
        for (key, needs_reseal) in [(Some(&self.current), false), (self.previous.as_ref(), true)] {
            let Some(key) = key else { continue };
            if let Ok(plaintext) = key.decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad,
                },
            ) {
                let value = String::from_utf8(plaintext).map_err(|_| {
                    AppError::internal("a decrypted secret is not valid UTF-8")
                })?;
                return Ok(Opened {
                    value,
                    needs_reseal,
                });
            }
        }

        // Authentication failed under every key. Three causes, indistinguishable
        // by design and all meaning the same thing operationally: the wrong key,
        // a tampered row, or the wrong `aad` (the value was moved from another
        // column). Never a partially-decrypted result.
        Err(AppError::internal(
            "a sealed secret could not be decrypted — SECRET_ENCRYPTION_KEY does not match \
             the data, or the stored value was altered",
        ))
    }
}
