use async_trait::async_trait;

use ferum_application::ports::PasswordHasher;
use ferum_domain::AppError;

mockall::mock! {
    pub PasswordHasher {}

    #[async_trait]
    impl PasswordHasher for PasswordHasher {
        async fn hash(&self, password: &str) -> Result<String, AppError>;
        async fn verify<'a>(&self, password: &'a str, hash: &'a str) -> Result<bool, AppError>;
    }
}
