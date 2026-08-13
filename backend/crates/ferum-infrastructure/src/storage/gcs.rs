//! Google Cloud Storage backend, spoken natively over the XML API.
//!
//! # Why this is hand-rolled rather than a client library
//!
//! The `StorageService` port is four methods, two of which are pure string
//! manipulation. The remaining two are one `PUT` and one `DELETE` against a
//! documented REST endpoint. Measured against that, every published client was
//! disproportionate — `cargo tree` on 2026-08-03, compared name-by-name against
//! this workspace's `Cargo.lock`:
//!
//! | Crate                          | Crates in tree | Names absent from this workspace | Duplicate majors forced |
//! | ------------------------------ | -------------- | -------------------------------- | ----------------------- |
//! | `google-cloud-storage` 1.17    | 178            | 27 (incl. tonic, prost, OpenTelemetry) | `reqwest` 0.13 vs our 0.12 |
//! | `gcloud-storage` 1.3           | 148            | 8                                | `reqwest` 0.13 vs 0.12, `jsonwebtoken` 10 vs 9 |
//! | this module                    | 0              | 0                                | none                    |
//!
//! A duplicate major of `reqwest` is precisely the failure the workspace
//! dependency table in `backend/Cargo.toml` exists to prevent: two builds of one
//! library in one binary, no warning. Both clients also carry a gRPC or
//! middleware stack for API surface this application will never call.
//!
//! Everything below is built from crates `ferum-infrastructure` already links:
//! `reqwest` for HTTP, `jsonwebtoken` for the RS256 assertion (its
//! `EncodingKey::from_rsa_pem` accepts the PKCS#8 `-----BEGIN PRIVATE KEY-----`
//! body of a service-account JSON), `serde_json`, `chrono`, `tokio`. The only
//! addition is `urlencoding`, already in `[workspace.dependencies]` and already
//! compiled for `ferum-web`.
//!
//! # Why the XML API and not the JSON API
//!
//! The XML API accepts an OAuth 2.0 bearer token exactly like the JSON API, but
//! its object endpoint *is* the public path-style URL, and it stores
//! `Content-Type` and `Cache-Control` request headers as object metadata
//! directly. The JSON API's `uploadType=media` would need a second
//! `multipart/related` body just to attach the same metadata.
//!
//! HMAC interoperability keys are deliberately not used here. They work — see
//! `S3StorageService`, which can be pointed at `https://storage.googleapis.com`
//! — but they are long-lived static secrets. Going through OAuth is what makes
//! keyless Workload Identity on GKE and Cloud Run possible.
//!
//! # What is NOT covered by an automated test
//!
//! `public_url`/`key_from_url`, credential parsing and object-name mapping are
//! unit-tested in `backend/tests/infrastructure/src/storage/gcs.rs`, and both
//! URL round trips are pinned across the whole configuration matrix in
//! `storage/file_url_contract.rs`.
//!
//! Everything that makes a request is **compile-checked only**: `put`, `delete`,
//! token acquisition for all three credential types, and the `401` refresh-and-
//! retry in [`GcsStorageService::send_authorized`]. There is no fake-gcs-server
//! or testcontainers harness in this repository, so no test observes a real
//! request or response. Covering the retry specifically would need
//! [`XML_API_ROOT`] to be injectable — test-only plumbing in a production
//! constructor — which is a worse trade than saying plainly that it is
//! unverified.

use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use reqwest::header::{CACHE_CONTROL, CONTENT_TYPE};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use tokio::sync::RwLock;

use ferum_application::ports::StorageService;
use ferum_application::shared::AppError;

use super::url_shapes::{
    strip_base, strip_files_prefix, under_base, without_query_or_fragment, UnderBase,
};
use crate::network_utils::truncate_for_log;

/// Path-style XML API root. Also the default `public_url` origin.
const XML_API_ROOT: &str = "https://storage.googleapis.com";

/// Google's OAuth 2.0 token endpoint — the audience of the signed assertion as
/// well as the URL it is posted to.
const OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// GCE/GKE/Cloud Run metadata server. The default identity source, and the one
/// that requires no secret material anywhere in the deployment.
const METADATA_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-account/token";

const JWT_BEARER_GRANT: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";

/// Read *and* write: this adapter uploads and deletes. `devstorage.read_write`
/// is the narrowest scope that covers both; `cloud-platform` would also work and
/// grants far more than object access.
const STORAGE_SCOPE: &str = "https://www.googleapis.com/auth/devstorage.read_write";

/// A CAS key is a digest of the bytes it names, so the object behind a key can
/// never change. Matches what `/files/` serves for database-stored blobs, so an
/// install that switches backends does not change how anything caches.
const IMMUTABLE_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

/// Every Google-owned host an object of ours may legitimately appear under.
///
/// Each is expanded in [`GcsStorageService::new`] into the concrete path-style
/// and virtual-hosted prefixes for *this* bucket, which is what keeps
/// `key_from_url` from claiming an object in somebody else's bucket.
const GCS_HOSTS: &[&str] = &[
    // XML/JSON API, path-style and virtual-hosted.
    "storage.googleapis.com",
    // Cookie-authenticated browser downloads — what the Cloud Console's "copy
    // link" button produces, so it does end up pasted into posts.
    "storage.cloud.google.com",
    // Mutual-TLS variants of the above.
    "storage.mtls.googleapis.com",
    "storage.mtls.cloud.google.com",
    // The CNAME target for custom-domain buckets. A deployment using one will
    // normally also set CDN_BASE_URL, but the raw target is a valid URL too.
    "c.storage.googleapis.com",
];

/// Grace applied to a token's stated lifetime, so a request that starts just
/// before expiry does not arrive just after it.
const TOKEN_EXPIRY_SKEW_SECS: i64 = 60;

/// Fallback lifetime when the token endpoint omits `expires_in`. Google always
/// sends it; assuming *forever* if it were ever missing would wedge the process
/// on a dead token until restart, so assume the documented minimum instead.
const DEFAULT_TOKEN_LIFETIME_SECS: i64 = 3600;

// ─── Credentials ──────────────────────────────────────────────────────────────

/// The shape of an Application Default Credentials JSON document, covering the
/// two `type`s that can mint a token without additional infrastructure.
#[derive(Deserialize)]
struct AdcDocument {
    #[serde(rename = "type")]
    kind: String,
    // service_account
    client_email: Option<String>,
    private_key: Option<String>,
    private_key_id: Option<String>,
    // authorized_user (`gcloud auth application-default login`)
    client_id: Option<String>,
    client_secret: Option<String>,
    refresh_token: Option<String>,
}

/// A resolved identity. Parsed once at construction so a malformed key fails the
/// deploy rather than the first upload.
enum Credentials {
    /// Service-account key: sign a JWT assertion, exchange it for a token.
    ServiceAccount {
        client_email: String,
        private_key_id: Option<String>,
        signing_key: Box<EncodingKey>,
    },
    /// A developer's own `gcloud` credentials — the local-machine case.
    AuthorizedUser {
        client_id: String,
        client_secret: String,
        refresh_token: String,
    },
    /// Ask the platform. No secret exists anywhere in the deployment; on GKE and
    /// Cloud Run this is Workload Identity. The default when nothing explicit
    /// was configured.
    MetadataServer,
}

impl Credentials {
    /// A short, non-secret label for the startup log.
    fn describe(&self) -> &'static str {
        match self {
            Credentials::ServiceAccount { .. } => "service-account key",
            Credentials::AuthorizedUser { .. } => "gcloud authorized-user credentials",
            Credentials::MetadataServer => "GCE/GKE/Cloud Run metadata server (Workload Identity)",
        }
    }

    fn parse(json: &str) -> Result<Self, AppError> {
        let doc: AdcDocument = serde_json::from_str(json).map_err(|e| {
            // Deliberately does not echo the document: it holds a private key.
            AppError::internal(format!("GCS credentials are not valid JSON: {e}"))
        })?;

        match doc.kind.as_str() {
            "service_account" => {
                let client_email = doc.client_email.ok_or_else(|| {
                    AppError::internal("GCS service-account credentials lack `client_email`")
                })?;
                let private_key = doc.private_key.ok_or_else(|| {
                    AppError::internal("GCS service-account credentials lack `private_key`")
                })?;
                // Google issues PKCS#8 (`-----BEGIN PRIVATE KEY-----`).
                // `from_rsa_pem` classifies by PEM tag and accepts both that and
                // PKCS#1, so no conversion step is needed.
                let signing_key =
                    EncodingKey::from_rsa_pem(private_key.as_bytes()).map_err(|e| {
                        AppError::internal(format!(
                            "GCS service-account `private_key` is not a usable RSA PEM key: {e}"
                        ))
                    })?;
                Ok(Credentials::ServiceAccount {
                    client_email,
                    private_key_id: doc.private_key_id,
                    signing_key: Box::new(signing_key),
                })
            }
            "authorized_user" => Ok(Credentials::AuthorizedUser {
                client_id: doc.client_id.ok_or_else(|| {
                    AppError::internal("GCS authorized-user credentials lack `client_id`")
                })?,
                client_secret: doc.client_secret.ok_or_else(|| {
                    AppError::internal("GCS authorized-user credentials lack `client_secret`")
                })?,
                refresh_token: doc.refresh_token.ok_or_else(|| {
                    AppError::internal("GCS authorized-user credentials lack `refresh_token`")
                })?,
            }),
            // `external_account` is Workload Identity *Federation* (AWS/Azure/OIDC
            // → Google). It needs a whole second credential-source protocol, and
            // silently treating it as "no credentials" would send an operator
            // hunting a 401 that has nothing to do with their bucket.
            other => Err(AppError::internal(format!(
                "GCS credential type `{other}` is not supported. Use a service-account key, \
                 `gcloud auth application-default login`, or run on GCE/GKE/Cloud Run and \
                 configure no credentials at all so the metadata server is used."
            ))),
        }
    }
}

#[derive(Clone)]
struct CachedToken {
    value: String,
    /// Unix seconds, already reduced by [`TOKEN_EXPIRY_SKEW_SECS`].
    good_until: i64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<i64>,
}

// ─── Adapter ──────────────────────────────────────────────────────────────────

pub struct GcsStorageService {
    http: Client,
    bucket: String,
    /// Normalised to `""` or `"something/"`, so `{prefix}{key}` is always the
    /// object name and needs no separator logic at the call sites.
    prefix: String,
    /// Present only when `CDN_BASE_URL` is set. `None` means `public_url` mints
    /// the path-style Google origin.
    cdn_base_url: Option<String>,
    /// Every `{scheme}://{host}[/bucket]` prefix that can precede one of our
    /// object names, precomputed so `key_from_url` does no formatting.
    native_prefixes: Vec<String>,
    credentials: Credentials,
    token: RwLock<Option<CachedToken>>,
}

impl GcsStorageService {
    /// `credentials_json` wins over `credentials_file`, which wins over the
    /// well-known `gcloud` ADC file, which wins over the metadata server.
    ///
    /// Credentials are *parsed* here so that a broken key fails startup, but no
    /// token is fetched — binding process start to a network round trip would
    /// make a transient IAM blip look like a crashed deploy.
    pub fn new(
        bucket: &str,
        prefix: Option<&str>,
        cdn_base_url: Option<&str>,
        credentials_json: Option<&str>,
        credentials_file: Option<&str>,
    ) -> Result<Self, AppError> {
        if bucket.trim().is_empty() {
            return Err(AppError::internal("GCS_BUCKET is set but empty"));
        }
        let bucket = bucket.trim().to_string();

        let prefix = prefix
            .map(|p| p.trim_matches('/'))
            .filter(|p| !p.is_empty())
            .map(|p| format!("{p}/"))
            .unwrap_or_default();

        let cdn_base_url = cdn_base_url
            .map(|u| u.trim_end_matches('/').to_string())
            .filter(|u| !u.is_empty())
            .and_then(|base| {
                if !is_bucketless_gcs_host(&base) {
                    return Some(base);
                }
                // Named and ignored, the same treatment `startup.rs` gives a
                // storage backend that loses the precedence contest. Honouring
                // it would be worse than useless — see the function's docs.
                tracing::warn!(
                    "CDN_BASE_URL is `{base}`, the Google Storage host with no bucket \
                     segment. Every object URL built from it would omit the bucket, and \
                     in a path-style GCS URL the first segment IS the bucket — so those \
                     URLs would address somebody else's bucket, not a 404. Ignoring it; \
                     unset CDN_BASE_URL (the default already mints the correct \
                     path-style URL) or point it at a real CDN hostname."
                );
                None
            });

        let credentials = Self::resolve_credentials(credentials_json, credentials_file)?;
        tracing::info!("GCS identity: {}", credentials.describe());

        let mut native_prefixes = Vec::with_capacity(GCS_HOSTS.len() * 4);
        for host in GCS_HOSTS {
            for scheme in ["https", "http"] {
                // Path-style: the shape `public_url` mints and the XML API uses.
                native_prefixes.push(format!("{scheme}://{host}/{bucket}"));
                // Virtual-hosted: what most SDKs and the Cloud Console emit.
                native_prefixes.push(format!("{scheme}://{bucket}.{host}"));
            }
        }

        // Deliberately NOT `network_utils::build_pinned_client`. That guard
        // exists for user-, admin- and plugin-supplied URLs, and it refuses
        // private and link-local addresses — which is exactly where the metadata
        // server lives (169.254.169.254). Every destination reached from this
        // module is a compile-time constant, so there is no URL for an attacker
        // to influence and nothing for the SSRF guard to protect.
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| AppError::internal(format!("GCS HTTP client build failed: {e}")))?;

        Ok(Self {
            http,
            bucket,
            prefix,
            cdn_base_url,
            native_prefixes,
            credentials,
            token: RwLock::new(None),
        })
    }

    fn resolve_credentials(
        credentials_json: Option<&str>,
        credentials_file: Option<&str>,
    ) -> Result<Credentials, AppError> {
        if let Some(json) = credentials_json.map(str::trim).filter(|j| !j.is_empty()) {
            return Credentials::parse(json);
        }
        if let Some(path) = credentials_file.map(str::trim).filter(|p| !p.is_empty()) {
            let body = std::fs::read_to_string(path).map_err(|e| {
                AppError::internal(format!("cannot read GCS credentials file `{path}`: {e}"))
            })?;
            return Credentials::parse(&body);
        }
        // The file `gcloud auth application-default login` writes. Checked before
        // falling through so a developer's laptop behaves the same as every other
        // Google tool without any Ferum-specific configuration.
        if let Some(path) = well_known_adc_path() {
            if let Ok(body) = std::fs::read_to_string(&path) {
                return Credentials::parse(&body);
            }
        }
        Ok(Credentials::MetadataServer)
    }

    /// Object name for a CAS key: the key with the configured prefix applied.
    fn object_name(&self, key: &str) -> String {
        format!("{}{}", self.prefix, key)
    }

    /// Inverse of [`object_name`](Self::object_name). `None` when the object
    /// lies outside our prefix — that is somebody else's data in a shared
    /// bucket, and claiming it would decrement a reference count we do not own.
    fn key_from_object_name(&self, object_name: &str) -> Option<String> {
        let key = if self.prefix.is_empty() {
            object_name
        } else {
            object_name.strip_prefix(&self.prefix)?
        };
        (!key.is_empty()).then(|| key.to_string())
    }

    /// The XML API endpoint for an object — also its path-style public URL, but
    /// percent-encoded per path segment.
    ///
    /// `public_url` deliberately does *not* encode: it must round-trip through
    /// `key_from_url` byte for byte, and `cas_key` only ever emits
    /// `[a-z0-9_-]+/[0-9a-f]{32}.[a-z]{3,4}`, every character of which is
    /// RFC 3986 unreserved. Encoding here is belt-and-braces for a hand-set
    /// `GCS_PREFIX`, which an operator can write anything into.
    fn object_endpoint(&self, key: &str) -> String {
        let encoded = self
            .object_name(key)
            .split('/')
            .map(|segment| urlencoding::encode(segment).into_owned())
            .collect::<Vec<_>>()
            .join("/");
        format!("{XML_API_ROOT}/{}/{}", self.bucket, encoded)
    }

    async fn access_token(&self) -> Result<String, AppError> {
        let now = Utc::now().timestamp();
        if let Some(token) = self.token.read().await.as_ref() {
            if token.good_until > now {
                return Ok(token.value.clone());
            }
        }

        // Refresh under the write lock, and re-check first: concurrent uploads
        // that all missed above would otherwise each mint a token.
        let mut guard = self.token.write().await;
        if let Some(token) = guard.as_ref() {
            if token.good_until > Utc::now().timestamp() {
                return Ok(token.value.clone());
            }
        }

        let fetched = self.fetch_token().await?;
        let value = fetched.access_token;
        *guard = Some(CachedToken {
            value: value.clone(),
            good_until: Utc::now().timestamp()
                + fetched.expires_in.unwrap_or(DEFAULT_TOKEN_LIFETIME_SECS)
                - TOKEN_EXPIRY_SKEW_SECS,
        });
        Ok(value)
    }

    /// Drops the cached token, but only if it is still the one that just failed.
    ///
    /// The conditional is the point. Several requests can be in flight with the
    /// same token when it dies, so an unconditional clear would have each of
    /// them throw away whatever refresh the others had already completed, and
    /// the "one bad token" case would turn into a token request per in-flight
    /// upload. Comparing first means the first caller through refreshes and the
    /// rest find a token that is no longer theirs and leave it alone.
    async fn invalidate_token(&self, stale: &str) {
        let mut guard = self.token.write().await;
        if guard.as_ref().is_some_and(|cached| cached.value == stale) {
            *guard = None;
        }
    }

    /// Sends an authorized request, retrying **once** with a fresh token if the
    /// first attempt comes back `401`.
    ///
    /// A cached token is trusted until its stated expiry, which is normally
    /// right and occasionally not: deleting or disabling the service account
    /// kills the token immediately, and clock skew can put us on the wrong side
    /// of an expiry we thought we had a minute of slack on. Without this, the
    /// process keeps presenting a token it has every reason to believe is valid
    /// and every upload fails until the clock says it expired — up to an hour of
    /// broken uploads that heal themselves for no visible reason.
    ///
    /// Only `401`. A `403` is an authorization decision about a token the
    /// endpoint accepted — the bucket IAM is wrong, or the scope is too narrow —
    /// and retrying with an identical fresh token would just double the request
    /// count on every genuine permission failure.
    ///
    /// `build` is called per attempt rather than the request being cloned, so
    /// the second attempt carries the new bearer token rather than re-sending
    /// the dead one.
    async fn send_authorized<F>(&self, op: &str, build: F) -> Result<reqwest::Response, AppError>
    where
        F: Fn(&Client, &str) -> reqwest::RequestBuilder,
    {
        let token = self.access_token().await?;
        let response = build(&self.http, &token)
            .send()
            .await
            .map_err(|e| AppError::internal(format!("GCS {op} error: {e}")))?;

        if response.status() != StatusCode::UNAUTHORIZED {
            return Ok(response);
        }

        tracing::warn!(
            "GCS rejected a cached access token ({op}); refreshing and retrying once"
        );
        self.invalidate_token(&token).await;
        let token = self.access_token().await?;
        build(&self.http, &token)
            .send()
            .await
            .map_err(|e| AppError::internal(format!("GCS {op} error after token refresh: {e}")))
    }

    async fn fetch_token(&self) -> Result<TokenResponse, AppError> {
        let response = match &self.credentials {
            Credentials::ServiceAccount {
                client_email,
                private_key_id,
                signing_key,
            } => {
                let now = Utc::now().timestamp();
                let claims = serde_json::json!({
                    "iss": client_email,
                    "scope": STORAGE_SCOPE,
                    "aud": OAUTH_TOKEN_URL,
                    "iat": now,
                    "exp": now + DEFAULT_TOKEN_LIFETIME_SECS,
                });
                let mut header = Header::new(Algorithm::RS256);
                header.kid = private_key_id.clone();
                let assertion =
                    jsonwebtoken::encode(&header, &claims, signing_key).map_err(|e| {
                        AppError::internal(format!("GCS assertion signing failed: {e}"))
                    })?;

                self.http
                    .post(OAUTH_TOKEN_URL)
                    .form(&[
                        ("grant_type", JWT_BEARER_GRANT),
                        ("assertion", assertion.as_str()),
                    ])
                    .send()
                    .await
            }
            Credentials::AuthorizedUser {
                client_id,
                client_secret,
                refresh_token,
            } => {
                self.http
                    .post(OAUTH_TOKEN_URL)
                    .form(&[
                        ("grant_type", "refresh_token"),
                        ("client_id", client_id.as_str()),
                        ("client_secret", client_secret.as_str()),
                        ("refresh_token", refresh_token.as_str()),
                    ])
                    .send()
                    .await
            }
            Credentials::MetadataServer => {
                // The header is the whole anti-SSRF handshake the metadata
                // server requires; without it the request is refused.
                self.http
                    .get(METADATA_TOKEN_URL)
                    .header("Metadata-Flavor", "Google")
                    .send()
                    .await
            }
        }
        .map_err(|e| AppError::internal(format!("GCS token request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::internal(format!(
                "GCS token request rejected ({status}): {}",
                truncate_for_log(&body)
            )));
        }
        response
            .json::<TokenResponse>()
            .await
            .map_err(|e| AppError::internal(format!("GCS token response unreadable: {e}")))
    }
}

/// Is this `CDN_BASE_URL` a Google Storage host carrying no bucket?
///
/// That one shape cannot work, and fails in the worst available way. `public_url`
/// is `{base}/{object name}`, so a base of `https://storage.googleapis.com`
/// yields `https://storage.googleapis.com/{prefix}/{key}` — and in a path-style
/// GCS URL the **first path segment is the bucket**. The result is not a broken
/// link to our bucket; it is a working link to a *different* bucket, named after
/// our `GCS_PREFIX`, belonging to whoever registered it.
///
/// Every other Google-host shape is fine and must not be flagged:
/// `https://storage.googleapis.com/our-bucket` (path-style with the bucket) and
/// `https://our-bucket.storage.googleapis.com` (virtual-hosted) both already
/// name the bucket, so `{base}/{object name}` is exactly right.
fn is_bucketless_gcs_host(base: &str) -> bool {
    let authority = base
        .strip_prefix("https://")
        .or_else(|| base.strip_prefix("http://"))
        .unwrap_or(base);
    // A `/` means a path, and for these hosts a path means the bucket.
    !authority.contains('/') && GCS_HOSTS.contains(&authority)
}

/// Where `gcloud auth application-default login` writes its credentials.
fn well_known_adc_path() -> Option<std::path::PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA")?
    } else {
        let mut home = std::env::var_os("HOME")?;
        home.push("/.config");
        home
    };
    let mut path = std::path::PathBuf::from(base);
    path.push("gcloud");
    path.push("application_default_credentials.json");
    Some(path)
}

#[async_trait]
impl StorageService for GcsStorageService {
    async fn put(&self, key: &str, data: Bytes, content_type: &str) -> Result<(), AppError> {
        let endpoint = self.object_endpoint(key);
        // `Bytes` is refcounted, so the clone a retry needs costs a pointer
        // bump, not a copy of the upload.
        let response = self
            .send_authorized("put", |http, token| {
                http.put(&endpoint)
                    .bearer_auth(token)
                    .header(CONTENT_TYPE, content_type)
                    .header(CACHE_CONTROL, IMMUTABLE_CACHE_CONTROL)
                    .body(data.clone())
            })
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::internal(format!(
                "GCS put rejected ({status}) for `{key}`: {}",
                truncate_for_log(&body)
            )));
        }
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), AppError> {
        let endpoint = self.object_endpoint(key);
        let response = self
            .send_authorized("delete", |http, token| {
                http.delete(&endpoint).bearer_auth(token)
            })
            .await?;

        let status = response.status();
        // Absent is the desired end state, and it is reachable legitimately: the
        // `GcStorageKey` job may be retried, and a key written while the install
        // was on database storage has no GCS object at all. Reporting that as a
        // failure would leave the job erroring forever over nothing.
        if status == StatusCode::NOT_FOUND || status.is_success() {
            return Ok(());
        }
        let body = response.text().await.unwrap_or_default();
        Err(AppError::internal(format!(
            "GCS delete rejected ({status}) for `{key}`: {}",
            truncate_for_log(&body)
        )))
    }

    fn public_url(&self, key: &str) -> String {
        match &self.cdn_base_url {
            // A Cloud CDN / external load balancer in front of the bucket serves
            // objects at the bucket-relative path, so the object name — prefix
            // included — is what follows the custom origin.
            Some(base) => format!("{base}/{}", self.object_name(key)),
            None => format!("{XML_API_ROOT}/{}/{}", self.bucket, self.object_name(key)),
        }
    }

    /// Bases first, the bare resolver path last. See `url_shapes` for why that
    /// order is load-bearing: a CDN mounted at a path ending in `/files` is a
    /// perfectly ordinary way to keep URL shapes stable across a move from
    /// database storage, and scanning for `/files/` first would match the base
    /// and return a key still carrying `GCS_PREFIX`.
    fn key_from_url(&self, url: &str) -> Option<String> {
        let url = without_query_or_fragment(url);

        // 1. Google-owned shapes for *this* bucket: path-style, virtual-hosted,
        //    the console's storage.cloud.google.com links, the mTLS endpoints and
        //    the custom-domain CNAME target. Anchoring each on the bucket name is
        //    what stops a URL pointing at somebody else's bucket from being
        //    claimed as ours. Signed URLs land here too — their signature lives
        //    in the query string, already removed above.
        //
        //    BEFORE the CDN, because these are the more specific bases: each one
        //    names the bucket, while a CDN base names only a host. A
        //    `CDN_BASE_URL` of `https://storage.googleapis.com` is a strict
        //    prefix of every path-style entry here, so consulting the CDN first
        //    made the genuine
        //    `https://storage.googleapis.com/{bucket}/{prefix}/{key}` resolve to
        //    an object name beginning with the *bucket*, fail the prefix strip,
        //    and come back `None` — the same family as the longest-base-first
        //    rule in `S3StorageService`, and just as quiet.
        //
        //    HONEST NOTE ON WHAT GUARDS WHAT: that base no longer survives
        //    construction (`is_bucketless_gcs_host` drops it), so this ordering
        //    is defence in depth rather than the primary fix, and reversing it
        //    on its own does not fail any test — verified, not assumed. It stays
        //    because "most specific base first" is the rule that is true
        //    independently of which bases the constructor happens to admit.
        if let Some(object_name) = self
            .native_prefixes
            .iter()
            .find_map(|prefix| strip_base(url, prefix))
        {
            return self.key_from_object_name(object_name);
        }

        // 2. Our own CDN, if one is configured. Two shapes can appear under it:
        //    this adapter's object names, and `/files/` paths written while the
        //    same CDN sat in front of database storage. Only the former carries
        //    the prefix, which is why the branch reports which it found.
        if let Some(base) = &self.cdn_base_url {
            match under_base(url, base) {
                Some(UnderBase::ResolverPath(key)) => return Some(key.to_string()),
                Some(UnderBase::ObjectName(name)) => return self.key_from_object_name(name),
                None => {}
            }
        }

        // 3. The bare `/files/{key}` resolver path, which is what `file_url`
        //    persists and therefore the shape that actually dominates the
        //    database. Anchored at the start of the string, so it claims only
        //    our own origin — see `strip_files_prefix`. It never carries the
        //    prefix: nothing that mints it knows this adapter exists.
        strip_files_prefix(url).map(str::to_string)
    }
}
