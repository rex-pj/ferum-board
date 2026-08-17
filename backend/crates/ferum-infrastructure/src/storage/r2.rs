//! Cloudflare R2, over the S3 API. Uploads share `S3Compatible`; only the URL
//! policy differs, and that is why this is a separate adapter.
//!
//! **R2's S3 endpoint serves signed requests only**, so its URL is useless in an
//! `<img>` tag and no public origin can be derived from the account or bucket.
//! Hence `R2_PUBLIC_BASE_URL` is mandatory and its absence FAILS STARTUP: a
//! minted URL is written into post HTML that is never rewritten, so degrading
//! would bake in links that can never load.
//!
//! `put`/`delete` are compile-checked only — there is no R2 emulator here.

use async_trait::async_trait;
use bytes::Bytes;

use ferum_application::ports::StorageService;
use ferum_application::shared::AppError;

use super::s3_client::S3Compatible;
use super::url_shapes::{strip_base, strip_files_prefix, under_base, without_query_or_fragment};

/// Host suffix of the S3 API endpoint. Also covers the jurisdiction-restricted
/// endpoints (`{account}.eu.r2.cloudflarestorage.com` and friends), which is
/// what makes the "not a valid public base" check below catch all of them.
const API_HOST_SUFFIX: &str = ".r2.cloudflarestorage.com";

/// Host suffix of the Cloudflare-managed public development subdomain.
const DEV_HOST_SUFFIX: &str = ".r2.dev";

/// R2 accepts only `auto`. An empty value and `us-east-1` alias to it, so the
/// generic S3 default would in fact work — naming it correctly costs nothing and
/// removes an `R2_REGION` variable from the configuration surface.
const REGION: &str = "auto";

pub struct R2StorageService {
    /// SDK client and the two operations, shared with `S3StorageService`.
    inner: S3Compatible,
    /// The **only** origin this adapter mints. Never the API endpoint: see the
    /// module docs.
    public_base: String,
    /// `{scheme}://{host}[/bucket]` prefixes that can precede one of our object
    /// names on Cloudflare's own hosts, precomputed. Read-only — nothing here is
    /// ever minted, but an install that previously ran through
    /// `S3StorageService` wrote these into rows that are still live, and a shape
    /// we stop recognising is a reference that stops being counted.
    native_prefixes: Vec<String>,
    /// `public_base`, plus `CDN_BASE_URL` when one is set. Longest first.
    ///
    /// `CDN_BASE_URL` cannot *configure* this backend's public origin — that is
    /// `R2_PUBLIC_BASE_URL` alone — but it is still recognised on read. Anyone
    /// who ran R2 through the S3 adapter had to set it or their images were
    /// already broken, so URLs under it can sit in rows predating `file_url`.
    /// Recognising it costs one entry and prevents exactly the silent
    /// ref-count loss `url_shapes` is written about.
    accepted_bases: Vec<String>,
}

impl R2StorageService {
    /// Fallible, and `startup.rs` propagates with `?` — every error below is a
    /// configuration that could only ever produce broken links.
    ///
    /// `endpoint_override` exists for the jurisdiction-restricted endpoints,
    /// whose exact hostnames this code deliberately does not try to enumerate.
    /// When it is `None` the canonical `https://{account}.r2.cloudflarestorage.com`
    /// is used.
    pub async fn new(
        account_id: &str,
        endpoint_override: Option<&str>,
        bucket: &str,
        access_key: &str,
        secret_key: &str,
        public_base_url: Option<&str>,
        cdn_base_url: Option<&str>,
    ) -> Result<Self, AppError> {
        let account_id = account_id.trim();
        if account_id.is_empty() {
            return Err(AppError::internal("R2_ACCOUNT_ID is set but empty"));
        }
        let bucket = bucket.trim();
        if bucket.is_empty() {
            return Err(AppError::internal(
                "R2_ACCOUNT_ID is set, so R2_BUCKET must name the bucket to upload into",
            ));
        }
        if access_key.trim().is_empty() || secret_key.trim().is_empty() {
            return Err(AppError::internal(
                "R2_ACCESS_KEY and R2_SECRET_KEY are both required — create an R2 API token \
                 in the Cloudflare dashboard and use its Access Key ID and Secret Access Key",
            ));
        }

        let endpoint = endpoint_override
            .map(str::trim)
            .filter(|e| !e.is_empty())
            .map(|e| e.trim_end_matches('/').to_string())
            .unwrap_or_else(|| format!("https://{account_id}{API_HOST_SUFFIX}"));

        let public_base = Self::validate_public_base(public_base_url)?;

        // Every Cloudflare-owned shape an object of ours can appear under,
        // anchored on THIS bucket so another tenant's URL is never claimed. The
        // canonical account endpoint is included even when an override is set,
        // because an install that switched to a jurisdiction endpoint still has
        // rows written under the old one.
        let mut native_prefixes = Vec::with_capacity(4);
        Self::push_vendor_prefixes(&mut native_prefixes, &endpoint, bucket);
        Self::push_vendor_prefixes(
            &mut native_prefixes,
            &format!("https://{account_id}{API_HOST_SUFFIX}"),
            bucket,
        );
        // Longest first, for the same reason `S3StorageService` sorts its bases:
        // one root can be a strict prefix of another, and the shorter match would
        // hand back a key that still carries a path segment.
        native_prefixes.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        native_prefixes.dedup();

        let mut accepted_bases = vec![public_base.clone()];
        accepted_bases.extend(
            cdn_base_url
                .map(|u| u.trim_end_matches('/'))
                .filter(|u| !u.is_empty())
                .map(str::to_string),
        );
        accepted_bases.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        accepted_bases.dedup();

        let inner =
            S3Compatible::connect("R2", &endpoint, access_key, secret_key, bucket, REGION).await;

        Ok(Self {
            inner,
            public_base,
            native_prefixes,
            accepted_bases,
        })
    }

    /// The whole reason this constructor is fallible.
    fn validate_public_base(public_base_url: Option<&str>) -> Result<String, AppError> {
        let base = public_base_url
            .map(|u| u.trim().trim_end_matches('/'))
            .filter(|u| !u.is_empty())
            .ok_or_else(|| {
                AppError::internal(
                    "R2_PUBLIC_BASE_URL is required when R2_ACCOUNT_ID is set. R2's S3 API \
                     endpoint serves signed requests only, so there is no public origin to \
                     fall back to — an anonymous browser request for an uploaded image would \
                     be refused. Set it to the bucket's public development URL \
                     (https://pub-<hash>.r2.dev) or to a custom domain bound to the bucket.",
                )
            })?;

        let host = host_of(base).ok_or_else(|| {
            AppError::internal(format!(
                "R2_PUBLIC_BASE_URL (`{base}`) must be an absolute URL including the scheme, \
                 e.g. `https://cdn.example.com`. Without one it is stored as a relative path \
                 and every image resolves against the page's own origin instead."
            ))
        })?;

        if host.ends_with(API_HOST_SUFFIX) {
            return Err(AppError::internal(format!(
                "R2_PUBLIC_BASE_URL (`{base}`) points at R2's S3 API endpoint, which serves \
                 signed requests only — every image URL built from it would be refused for \
                 anonymous visitors, and those URLs are written into post content that is \
                 never rewritten. Point it at the bucket's public development URL \
                 (https://pub-<hash>.r2.dev) or a custom domain bound to the bucket."
            )));
        }

        if host == "r2.dev" || host.ends_with(DEV_HOST_SUFFIX) {
            // Accepted, because it makes a development install work with no DNS
            // setup. Warned, because Cloudflare documents this endpoint as rate
            // limited and non-production, with no caching, WAF or bot
            // management — and a forum that reaches production still pointing
            // here would degrade under exactly the traffic it wants.
            tracing::warn!(
                "R2_PUBLIC_BASE_URL (`{base}`) is an r2.dev development URL. Cloudflare rate \
                 limits it and documents it as non-production — it gets no caching, WAF or bot \
                 management. Bind a custom domain to the bucket before serving real traffic."
            );
        }

        Ok(base.to_string())
    }

    /// Path-style and virtual-hosted prefixes for one endpoint root.
    fn push_vendor_prefixes(out: &mut Vec<String>, endpoint: &str, bucket: &str) {
        let Some((scheme, rest)) = endpoint.split_once("://") else {
            return;
        };
        let Some(host) = rest.split('/').next().filter(|h| !h.is_empty()) else {
            return;
        };
        // Path-style: what `S3Compatible` is pinned to and what the S3 adapter
        // minted while it was the one talking to R2.
        out.push(format!("{scheme}://{host}/{bucket}"));
        // Virtual-hosted: what most SDKs emit by default.
        out.push(format!("{scheme}://{bucket}.{host}"));
    }
}

/// Host of an absolute URL, port stripped. `None` for a URL with no scheme.
fn host_of(url: &str) -> Option<&str> {
    let (_, rest) = url.split_once("://")?;
    let host = rest.split('/').next()?;
    let host = host.split(':').next()?;
    (!host.is_empty()).then_some(host)
}

#[async_trait]
impl StorageService for R2StorageService {
    async fn put(&self, key: &str, data: Bytes, content_type: &str) -> Result<(), AppError> {
        self.inner.put(key, data, content_type).await
    }

    async fn delete(&self, key: &str) -> Result<(), AppError> {
        self.inner.delete(key).await
    }

    async fn list_keys(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Option<Vec<String>>, AppError> {
        self.inner.list(after, limit).await.map(Some)
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.public_base, key)
    }

    /// Vendor hosts first, then our own bases, then the bare resolver path. See
    /// `url_shapes` for why that order is load-bearing.
    fn key_from_url(&self, url: &str) -> Option<String> {
        let url = without_query_or_fragment(url);

        // 1. Cloudflare's own hosts, anchored on this account and this bucket.
        //    `strip_base` rather than `under_base`: nothing has ever minted a
        //    `/files/` path under the API host, so admitting one there would
        //    only widen what gets claimed.
        if let Some(key) = self
            .native_prefixes
            .iter()
            .find_map(|prefix| strip_base(url, prefix))
        {
            return Some(key.to_string());
        }

        // 2. The public base, and `CDN_BASE_URL` if one is set. Both can carry
        //    two shapes — this adapter's object names, and `/files/` paths
        //    written while the same domain sat in front of database storage.
        //    Neither applies a prefix on write, so they resolve the same way.
        if let Some(found) = self
            .accepted_bases
            .iter()
            .find_map(|base| under_base(url, base))
        {
            return Some(found.into_key().to_string());
        }

        // 3. The bare `/files/{key}` resolver path — what `file_url` persists,
        //    and therefore the shape that dominates the database. Anchored at
        //    the start, so a URL on somebody else's host is not claimed.
        strip_files_prefix(url).map(str::to_string)
    }
}
