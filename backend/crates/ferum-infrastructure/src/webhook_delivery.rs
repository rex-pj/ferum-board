use async_trait::async_trait;

use crate::job_queue::inline_runner::hmac_sha256;
use crate::network_utils::build_pinned_client;
use ferum_application::ports::{WebhookDeliveryService, WebhookTestResult};
use ferum_application::shared::AppError;

pub struct ReqwestWebhookDeliveryService;

#[async_trait]
impl WebhookDeliveryService for ReqwestWebhookDeliveryService {
    async fn send_test(&self, url: &str, secret: Option<&str>) -> Result<WebhookTestResult, AppError> {
        let client = match build_pinned_client(url).await {
            Ok(c) => c,
            Err(reason) => {
                return Ok(WebhookTestResult { success: false, status_code: None, error: Some(reason) });
            }
        };

        let body = serde_json::json!({
            "event": "test",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "data": { "message": "This is a test delivery from Ferum Board." },
        });
        let body_str = serde_json::to_string(&body).map_err(|e| AppError::internal(e.to_string()))?;

        let mut req = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("X-Ferum-Event", "test")
            .body(body_str.clone());

        if let Some(s) = secret {
            let sig = hmac_sha256(s, &body_str)?;
            req = req.header("X-Ferum-Signature", format!("sha256={}", sig));
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                Ok(WebhookTestResult {
                    success: status.is_success(),
                    status_code: Some(status.as_u16()),
                    error: if status.is_success() { None } else { Some(format!("Received HTTP {status}")) },
                })
            }
            Err(e) => Ok(WebhookTestResult { success: false, status_code: None, error: Some(e.to_string()) }),
        }
    }
}
