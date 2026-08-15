pub use ferum_domain::AppError;
pub use ferum_domain::OptionExt;

use sha2::{Digest, Sha256};

/// How many hex characters of the digest reach the log. 12 is 48 bits — far more
/// than enough to tell two probed addresses apart in a log window, and far too
/// few to be worth a reversal attempt against a value nothing else corroborates.
const EMAIL_DIGEST_HEX_LEN: usize = 12;

/// Renders an email address safe to write to a log line.
///
/// Returns `<12 hex>@<domain>`, or a bare `<12 hex>` when the input has no
/// single unambiguous domain part.
///
/// **Never log a raw address.** Two reasons, and the second is the one that
/// surprises people:
///
/// * It is PII, and a login endpoint is exactly where the addresses of people who
///   do *not* have accounts show up — so the log accumulates identifiers for
///   non-users, who never agreed to anything.
/// * Users paste passwords into the email field constantly. An unmasked
///   `login failed: unknown email` line is therefore a plausible place to find a
///   real credential for an account that exists under a *different* address.
///   Hashing removes that failure mode entirely; a "mask the local part" scheme
///   like `a***@example.com` does not, because the leading characters of a
///   password are still a meaningful head start.
///
/// The digest keeps what the log line is actually for: seeing that one source is
/// probing many distinct addresses, or the same one repeatedly. The domain is
/// kept in the clear because "someone is enumerating @ourcorp.com" is an
/// operationally different event from scattered noise, and a domain shared by
/// millions of mailboxes identifies nobody on its own.
///
/// A value with no `@` — the pasted-password case — yields the digest alone, so
/// nothing recognisable survives.
pub fn email_log_key(email: &str) -> String {
    let normalized = email.trim().to_lowercase();
    let digest = hex::encode(Sha256::digest(normalized.as_bytes()));
    let short = &digest[..EMAIL_DIGEST_HEX_LEN];

    // `rsplit_once` rather than `split_once`: an address may legally quote an `@`
    // in its local part, and the domain is what follows the LAST one. Anything
    // with no `@`, an empty domain, or a domain still containing an `@` (which no
    // valid address has) falls back to the digest alone rather than guessing.
    match normalized.rsplit_once('@') {
        Some((local, domain)) if !local.is_empty() && !domain.is_empty() => {
            format!("{short}@{domain}")
        }
        _ => short.to_string(),
    }
}
