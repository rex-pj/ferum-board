#![allow(dead_code)]

use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::Client;
use bytes::Bytes;

use ferum_application::ports::StorageService;
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
}
