use uuid::Uuid;

use ferum_application::ports::{AccessTokenClaims, TokenService};
use ferum_domain::AppError;

mockall::mock! {
    pub TokenService {}

    impl TokenService for TokenService {
        fn mint_access_token(&self, claims: &AccessTokenClaims) -> Result<String, AppError>;
        fn verify_access_token<'a>(&self, token: &'a str) -> Result<AccessTokenClaims, AppError>;
        fn mint_refresh_token(&self, user_id: Uuid) -> Result<String, AppError>;
        fn verify_refresh_token<'a>(&self, token: &'a str) -> Result<Uuid, AppError>;
        fn mint_email_token<'a>(&self, user_id: Uuid, purpose: &'a str) -> Result<String, AppError>;
        fn verify_email_token<'a>(&self, token: &'a str, expected_purpose: &'a str) -> Result<Uuid, AppError>;
    }
}
