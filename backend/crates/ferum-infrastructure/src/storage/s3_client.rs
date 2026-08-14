//! The `aws-sdk-s3` half that `S3StorageService` and `R2StorageService` share.
//!
//! Both adapters speak the same protocol to the same SDK; what separates them is
//! entirely which URLs they mint and which they recognise. Splitting the client
//! out keeps the SigV4, checksum and addressing configuration — the part whose
//! duplication would be genuinely dangerous, because a copy that drifted would
//! fail every request with an opaque signature error — in exactly one place.
//!
//! Nothing here knows about public URLs. That is the adapters' business, and for
//! R2 it is not derivable from anything in this file.

use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{
    Credentials, Region, RequestChecksumCalculation, ResponseChecksumValidation,
};
use aws_sdk_s3::Client;
use bytes::Bytes;

use ferum_application::shared::AppError;

/// A bucket on an S3-compatible endpoint, plus the two operations the
/// `StorageService` port actually needs.
pub(super) struct S3Compatible {
    client: Client,
    bucket: String,
    /// Names the backend in error messages, so a failed upload says which
    /// adapter produced it. `"S3"` or `"R2"`.
    label: &'static str,
}

impl S3Compatible {
    pub(super) async fn connect(
        label: &'static str,
        endpoint: &str,
        access_key: &str,
        secret_key: &str,
        bucket: &str,
        region: &str,
    ) -> Self {
        let credentials = Credentials::new(access_key, secret_key, None, None, "static");
        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(region.to_string()))
            .credentials_provider(credentials)
            .endpoint_url(endpoint)
            // This client is only ever built against an operator-set endpoint,
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
        // above: this client never talks to real AWS, and none of the endpoints
        // it does talk to are well served by the SDK's virtual-hosted default.
        //
        // * MinIO needs DNS for `{bucket}.{host}` that a compose file does not
        //   provide, so virtual-hosted simply fails there.
        // * A bucket name containing a dot breaks TLS under virtual-hosted —
        //   `*.storage.googleapis.com` does not match `my.bucket.storage.…`.
        // * It is what `S3StorageService::public_url` already mints
        //   (`{endpoint}/{bucket}/{key}`), so reads and writes agree on one
        //   shape instead of two.
        //
        // AWS is deprecating path-style for new buckets, which does not apply
        // here: neither adapter is constructed without an explicit endpoint.
        let s3_config = aws_sdk_s3::config::Builder::from(&config)
            .force_path_style(true)
            .build();

        Self {
            client: Client::from_conf(s3_config),
            bucket: bucket.to_string(),
            label,
        }
    }

    pub(super) async fn put(
        &self,
        key: &str,
        data: Bytes,
        content_type: &str,
    ) -> Result<(), AppError> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(data.into())
            .send()
            .await
            .map_err(|e| AppError::internal(format!("{} put error: {}", self.label, e)))?;
        Ok(())
    }

    pub(super) async fn delete(&self, key: &str) -> Result<(), AppError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| AppError::internal(format!("{} delete error: {}", self.label, e)))?;
        Ok(())
    }
}
