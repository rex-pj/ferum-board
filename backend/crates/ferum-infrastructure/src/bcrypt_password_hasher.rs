use ferum_application::constants::BCRYPT_COST;
use ferum_application::ports::PasswordHasher;
use ferum_application::shared::AppError;

pub struct BcryptPasswordHasher;

#[async_trait::async_trait]
impl PasswordHasher for BcryptPasswordHasher {
    async fn hash(&self, password: &str) -> Result<String, AppError> {
        let pwd = password.to_string();
        tokio::task::spawn_blocking(move || {
            bcrypt::hash(&pwd, BCRYPT_COST)
                .map_err(|e| AppError::internal(format!("bcrypt hash error: {}", e)))
        })
        .await
        .map_err(|e| AppError::internal(format!("thread join error: {}", e)))?
    }

    async fn verify<'a>(&self, password: &'a str, hash: &'a str) -> Result<bool, AppError> {
        let pwd = password.to_string();
        let h = hash.to_string();
        tokio::task::spawn_blocking(move || {
            bcrypt::verify(&pwd, &h)
                .map_err(|e| AppError::internal(format!("bcrypt verify error: {}", e)))
        })
        .await
        .map_err(|e| AppError::internal(format!("thread join error: {}", e)))?
    }
}
