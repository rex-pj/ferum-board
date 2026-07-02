use async_trait::async_trait;

use ferum_domain::repositories::plugin_db_repository::PluginDbGateway;
use ferum_domain::AppError;

mockall::mock! {
    pub PluginDbGateway {}

    #[async_trait]
    impl PluginDbGateway for PluginDbGateway {
        async fn provision_schema(&self, slug: &str, statements: &[String]) -> Result<(), AppError>;
        async fn drop_schema(&self, slug: &str) -> Result<(), AppError>;
        async fn query(&self, slug: &str, sql: &str, params: Vec<serde_json::Value>) -> Result<serde_json::Value, AppError>;
    }
}
