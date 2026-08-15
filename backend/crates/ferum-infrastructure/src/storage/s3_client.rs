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
            // MANDATORY for MinIO, R2 and GCS interop — this client never talks
            // to real AWS. Since aws-sdk-s3 1.x the SDK folds `x-amz-checksum-*`
            // into the SigV4 canonical string, which those endpoints do not
            // recognise, so EVERY request fails `SignatureDoesNotMatch`.
            //
            // The integrity given up is already covered more strongly: each key
            // is a SHA-256 of its own bytes.
            .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
            .response_checksum_validation(ResponseChecksumValidation::WhenRequired)
            .load()
            .await;

        // Path-style: MinIO has no DNS for `{bucket}.{host}`, a bucket name with
        // a dot breaks TLS under virtual-hosted, and it matches what
        // `public_url` mints so reads and writes agree on one shape.
        //
        // AWS's deprecation does not apply — neither adapter is constructed
        // without an explicit endpoint.
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
