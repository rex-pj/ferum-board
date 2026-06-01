use crate::application::ports::PasswordHasher;
use crate::application::shared::AppError;
use crate::constants::BCRYPT_COST;

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

    async fn verify(&self, password: &str, hash: &str) -> Result<bool, AppError> {
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
