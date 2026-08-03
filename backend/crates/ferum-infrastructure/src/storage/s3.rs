
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::Client;
use bytes::Bytes;

use ferum_application::ports::{StorageService, LEGACY_FILES_PREFIX};
use ferum_application::shared::AppError;

pub struct S3StorageService {
    client: Client,
    bucket: String,
    cdn_base_url: String,
}

impl S3StorageService {
    pub async fn new(
        endpoint: &str,
        access_key: &str,
        secret_key: &str,
        bucket: &str,
        region: &str,
        cdn_base_url: &str,
    ) -> Self {
        let credentials = Credentials::new(access_key, secret_key, None, None, "static");
        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(region.to_string()))
            .credentials_provider(credentials)
            .endpoint_url(endpoint)
            .load()
            .await;

        Self {
            client: Client::new(&config),
            bucket: bucket.to_string(),
            cdn_base_url: cdn_base_url.trim_end_matches('/').to_string(),
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
        format!("{}/{}", self.cdn_base_url, key)
    }

    fn key_from_url(&self, url: &str) -> Option<String> {
        // Legacy first, and this branch is not hypothetical: switching an
        // existing install to S3 leaves every previously stored URL — avatars,
        // site config, and the HTML of every post ever written — in the
        // same-origin form. Those files also keep their bytes in `stored_files`,
        // so they must stay resolvable for reference counting to keep working.
        if let Some(i) = url.rfind(LEGACY_FILES_PREFIX) {
            let key = &url[i + LEGACY_FILES_PREFIX.len()..];
            return (!key.is_empty()).then(|| key.to_string());
        }
        // Our own shape: `{cdn_base_url}/{key}`.
        url.strip_prefix(&self.cdn_base_url)
            .and_then(|rest| rest.strip_prefix('/'))
            .filter(|k| !k.is_empty())
            .map(str::to_string)
    }
}
