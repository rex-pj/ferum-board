use async_trait::async_trait;
use uuid::Uuid;

use ferum_application::ports::BulkSeedService;
use ferum_domain::AppError;

/// No-op BulkSeedService — does nothing, succeeds immediately.
pub struct NoopBulkSeedService;

#[async_trait]
impl BulkSeedService for NoopBulkSeedService {
    async fn seed_bulk(&self, _admin_id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
}
