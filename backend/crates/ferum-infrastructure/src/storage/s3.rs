
use async_trait::async_trait;
use bytes::Bytes;

use ferum_application::ports::StorageService;
use ferum_application::shared::AppError;

use super::s3_client::S3Compatible;
use super::url_shapes::{strip_files_prefix, under_base, without_query_or_fragment};

pub struct S3StorageService {
    /// The SDK client and the two operations, shared with `R2StorageService` —
    /// see `s3_client.rs` for the signing and addressing configuration.
    inner: S3Compatible,
    /// Origin that new URLs are minted under: the CDN when one is configured,
    /// otherwise the path-style bucket root at the endpoint.
    public_base: String,
    /// Prefixes `key_from_url` strips, longest first.
    ///
    /// Two of them when a CDN is configured, because adding `CDN_BASE_URL` to a
    /// running forum must not orphan the `{endpoint}/{bucket}/{key}` URLs
    /// already written into post HTML — the same concern that makes
    /// `DatabaseStorageService` keep reading same-origin URLs after a CDN
    /// appears.
    accepted_bases: Vec<String>,
}

impl S3StorageService {
    /// `cdn_base_url` is `None` when `CDN_BASE_URL` is unset, and that
    /// distinction is load-bearing.
    ///
    /// `startup.rs` used to substitute the endpoint when no CDN was configured.
    /// Since `public_url` is `{base}/{key}`, that dropped the bucket segment and
    /// minted `{endpoint}/{key}` for objects living at
    /// `{endpoint}/{bucket}/{key}` — every image URL a 404, with nothing
    /// reporting it. An `Option` lets the no-CDN case build the bucket-qualified
    /// root instead.
    pub async fn new(
        endpoint: &str,
        access_key: &str,
        secret_key: &str,
        bucket: &str,
        region: &str,
        cdn_base_url: Option<&str>,
    ) -> Self {
        let inner =
            S3Compatible::connect("S3", endpoint, access_key, secret_key, bucket, region).await;

        // The same path-style shape `S3Compatible` pins the client to — see the
        // `force_path_style` rationale in `s3_client.rs`. Reads and writes agree
        // on one shape only because both come from that decision.
        let bucket_root = format!("{}/{}", endpoint.trim_end_matches('/'), bucket);

        let cdn_base = cdn_base_url
            .map(|u| u.trim_end_matches('/'))
            .filter(|u| !u.is_empty())
            .map(str::to_string);
        let public_base = cdn_base.clone().unwrap_or_else(|| bucket_root.clone());

        let mut accepted_bases = vec![bucket_root];
        accepted_bases.extend(cdn_base);
        // LONGEST FIRST. An operator who points CDN_BASE_URL straight at the S3
        // endpoint makes it a strict prefix of `{endpoint}/{bucket}`, and
        // matching the short one first would then resolve
        // `http://minio:9000/forum-uploads/avatars/a.png` to the key
        // `forum-uploads/avatars/a.png` — a key that exists nowhere, so the
        // reference lands on a file that does not exist while the real one goes
        // uncounted.
        accepted_bases.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        accepted_bases.dedup();

        Self {
            inner,
            public_base,
            accepted_bases,
        }
    }
}

#[async_trait]
impl StorageService for S3StorageService {
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

    /// Bases first, the bare resolver path last — see `url_shapes` for why the
    /// order matters. This adapter applies no prefix on write, so the two shapes
    /// `under_base` distinguishes both yield the key directly; the distinction
    /// is only load-bearing for GCS, which has `GCS_PREFIX`.
    fn key_from_url(&self, url: &str) -> Option<String> {
        // A presigned URL carries its authorization in the query string, which
        // is no part of the key.
        let url = without_query_or_fragment(url);
        // The shapes this adapter mints, longest base first — plus the `/files/`
        // paths written while the same CDN sat in front of database storage.
        if let Some(found) = self
            .accepted_bases
            .iter()
            .find_map(|base| under_base(url, base))
        {
            return Some(found.into_key().to_string());
        }
        // The bare resolver path, which is what `file_url` persists and so the
        // shape that dominates the database. Anchored at the start, so a URL on
        // somebody else's host is not claimed as ours.
        strip_files_prefix(url).map(str::to_string)
    }
}
