#![allow(dead_code)]

use async_trait::async_trait;
use uuid::Uuid;

use ferum_application::ports::NotificationBus;
use ferum_application::shared::AppError;

pub struct NullNotificationBus;

#[async_trait]
impl NotificationBus for NullNotificationBus {
    async fn publish(&self, _user_id: Uuid, _payload: serde_json::Value) -> Result<(), AppError> {
        Ok(())
    }
}
