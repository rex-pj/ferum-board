
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region, RequestChecksumCalculation, ResponseChecksumValidation};
use aws_sdk_s3::Client;
use bytes::Bytes;

use ferum_application::ports::StorageService;
use ferum_application::shared::AppError;

use super::url_shapes::{strip_files_prefix, under_base, without_query_or_fragment};

pub struct S3StorageService {
    client: Client,
    bucket: String,
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
        let credentials = Credentials::new(access_key, secret_key, None, None, "static");
        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(region.to_string()))
            .credentials_provider(credentials)
            .endpoint_url(endpoint)
            // This adapter is only ever constructed when `S3_ENDPOINT` is set,
            // i.e. it never talks to real AWS — its entire population is MinIO,
            // Cloudflare R2 and Google Cloud Storage's interoperability
            // endpoint. Since aws-sdk-s3 1.x the SDK computes a CRC32 by default
            // and folds `x-amz-checksum-*` / `x-amz-sdk-checksum-algorithm` into
            // the SigV4 canonical string. Those endpoints do not recognise the
            // headers, so the signature they compute differs from ours and every
            // request — list, put, delete — fails with `SignatureDoesNotMatch`
            // or `XAmzContentChecksumMismatch`. `WhenRequired` restores the
            // pre-2025 behaviour, which is what all three document as supported.
            //
            // The integrity this gives up is already provided end-to-end and
            // more strongly: every key is a SHA-256 of the bytes stored under it
            // (`storage_utils::cas_key`), so corrupted content cannot masquerade
            // under a valid key.
            .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
            .response_checksum_validation(ResponseChecksumValidation::WhenRequired)
            .load()
            .await;

        // Path-style addressing, for the same reason as the checksum settings
        // above: this adapter never talks to real AWS, and none of the endpoints
        // it does talk to are well served by the SDK's virtual-hosted default.
        //
        // * MinIO needs DNS for `{bucket}.{host}` that a compose file does not
        //   provide, so virtual-hosted simply fails there.
        // * A bucket name containing a dot breaks TLS under virtual-hosted —
        //   `*.storage.googleapis.com` does not match `my.bucket.storage.…`.
        // * It is what `public_url` already mints (`{endpoint}/{bucket}/{key}`),
        //   so reads and writes now agree on one shape instead of two.
        //
        // AWS is deprecating path-style for new buckets, which does not apply
        // here: this type is only ever constructed when `S3_ENDPOINT` is set.
        let s3_config = aws_sdk_s3::config::Builder::from(&config)
            .force_path_style(true)
            .build();

        // The same path-style shape the client above is pinned to.
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
            client: Client::from_conf(s3_config),
            bucket: bucket.to_string(),
            public_base,
            accepted_bases,
        }
    }
}

#[async_trait]
impl StorageService for S3StorageService {
    async fn put(&self, key: &str, data: Bytes, content_type: &str) -> Result<(), AppError> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(data.into())
            .send()
            .await
            .map_err(|e| AppError::internal(format!("S3 put error: {}", e)))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), AppError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| AppError::internal(format!("S3 delete error: {}", e)))?;
        Ok(())
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
