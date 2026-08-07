//! Startup check: can an anonymous visitor actually read what we upload?
//!
//! # The failure this exists to make visible
//!
//! `public_url` is written straight into `users.avatar_url`, `site_config` and
//! the stored HTML of every post, and browsers fetch it directly — no request
//! reaches this application. If the bucket is not readable by `allUsers`, every
//! one of those fetches is a 403 and **nothing on the server ever notices**:
//! uploads succeed, rows are written, logs are clean, and the only evidence is a
//! page of broken images. That is the same class of silent breakage as the
//! `img-src` CSP bug, and it is discovered the same way — by a user, late.
//!
//! # Why not signed URLs instead of a public bucket
//!
//! Because the URL is denormalised into content that is never rewritten. A
//! signed URL expires; the post HTML holding it does not. Serving private
//! objects would mean minting a fresh URL at render time, which cannot work for
//! text already stored. A publicly readable bucket is a consequence of the
//! design, not a shortcut around it.
//!
//! # How the probe works
//!
//! An **unauthenticated** GET for an object name that cannot exist. Both GCS and
//! S3 answer it the same way, and the two answers are what distinguish the
//! cases:
//!
//! * `404` — anonymous reads are allowed, the object simply is not there. Good.
//! * `403` — anonymous reads are refused. The store will not reveal whether the
//!   object exists to someone who may not read it, and that is precisely the
//!   response every `<img>` on the site is about to get.
//!
//! No credentials are attached on purpose: the question is what a *visitor*
//! sees, not what this process can do.

use std::time::Duration;

use ferum_application::ports::StorageService;

/// An object name no CAS key can collide with — `cas_key` emits
/// `{prefix}/{32 hex}.{ext}`, which has a `/` and no underscores at the front.
const PROBE_KEY: &str = "__ferum_public_read_probe__";

/// Deliberately short. This runs off the startup path and its answer is
/// advisory; a slow object store should not keep a task alive for 30 seconds.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub enum PublicReadProbe {
    /// `public_url` is relative, so this application serves the bytes itself and
    /// there is no third party whose permissions could be wrong.
    SameOrigin,
    /// An anonymous request was answered without an authorization error.
    Readable,
    /// An anonymous request was refused. Every uploaded image is a broken link.
    Forbidden,
    /// Could not tell — network failure, or a status that means neither.
    Inconclusive(String),
}

impl PublicReadProbe {
    /// Stable one-word outcome, for the `/health/ready` body and for deciding
    /// whether a repeated probe is worth a log line.
    ///
    /// Deliberately excludes the `Inconclusive` detail string: it carries a
    /// network error message that changes between attempts, so comparing it
    /// would report a "change" every time nothing had changed.
    pub fn label(&self) -> &'static str {
        match self {
            PublicReadProbe::SameOrigin => "same-origin",
            PublicReadProbe::Readable => "readable",
            PublicReadProbe::Forbidden => "forbidden",
            PublicReadProbe::Inconclusive(_) => "unknown",
        }
    }
}

/// Probes whatever origin `storage` currently mints URLs under.
///
/// Takes the port rather than a URL string so the probed origin is by
/// construction the one in use, with no second copy of the backend-selection
/// rules to drift out of step.
pub async fn probe_public_read(storage: &dyn StorageService) -> PublicReadProbe {
    let url = storage.public_url(PROBE_KEY);
    if !url.contains("://") {
        return PublicReadProbe::SameOrigin;
    }

    // NOT `network_utils::build_pinned_client`. That guard refuses private and
    // link-local addresses, which is right for user-supplied URLs and wrong
    // here: this address comes from operator configuration, and a perfectly
    // ordinary dev or single-host deployment points it at `http://minio:9000`
    // on a private network.
    let client = match reqwest::Client::builder().timeout(PROBE_TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => return PublicReadProbe::Inconclusive(format!("HTTP client build failed: {e}")),
    };

    match client.get(&url).send().await {
        Ok(response) => {
            let status = response.status();
            if status == reqwest::StatusCode::FORBIDDEN
                || status == reqwest::StatusCode::UNAUTHORIZED
            {
                PublicReadProbe::Forbidden
            } else if status == reqwest::StatusCode::NOT_FOUND || status.is_success() {
                // 404 is the expected answer and the informative one. A success
                // would mean somebody really stored an object under the sentinel
                // name — unlikely, and equally proof that reads are open.
                PublicReadProbe::Readable
            } else {
                PublicReadProbe::Inconclusive(format!("unexpected status {status}"))
            }
        }
        Err(e) => PublicReadProbe::Inconclusive(e.to_string()),
    }
}
