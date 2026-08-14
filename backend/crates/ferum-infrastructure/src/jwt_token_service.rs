use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ferum_application::ports::{AccessTokenClaims, TokenService};
use ferum_application::shared::AppError;

pub struct JwtTokenService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    access_ttl_secs: u64,
    refresh_ttl_secs: u64,
}

impl JwtTokenService {
    pub fn new(secret: &str, access_ttl_secs: u64, refresh_ttl_secs: u64) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            access_ttl_secs,
            refresh_ttl_secs,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct RefreshClaims {
    sub: String,
    purpose: String,
    exp: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct EmailClaims {
    sub: String,
    purpose: String,
    exp: i64,
}

impl TokenService for JwtTokenService {
    fn mint_access_token(&self, claims: &AccessTokenClaims) -> Result<String, AppError> {
        encode(&Header::default(), claims, &self.encoding_key)
            .map_err(|e| AppError::internal(format!("token mint error: {}", e)))
    }

    fn verify_access_token(&self, token: &str) -> Result<AccessTokenClaims, AppError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;

        decode::<AccessTokenClaims>(token, &self.decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(|_| AppError::Unauthorized)
    }

    fn mint_refresh_token(&self, user_id: Uuid) -> Result<String, AppError> {
        let claims = RefreshClaims {
            sub: user_id.to_string(),
            purpose: "refresh".to_string(),
            exp: (Utc::now() + Duration::seconds(self.refresh_ttl_secs as i64)).timestamp(),
        };
        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| AppError::internal(format!("refresh token error: {}", e)))
    }

    fn verify_refresh_token(&self, token: &str) -> Result<Uuid, AppError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;

        let data = decode::<RefreshClaims>(token, &self.decoding_key, &validation)
            .map_err(|_| AppError::Unauthorized)?;

        if data.claims.purpose != "refresh" {
            return Err(AppError::Unauthorized);
        }

        Uuid::parse_str(&data.claims.sub).map_err(|_| AppError::Unauthorized)
    }

    fn mint_email_token(
        &self,
        user_id: Uuid,
        purpose: &str,
        ttl_secs: u64,
    ) -> Result<String, AppError> {
        let claims = EmailClaims {
            sub: user_id.to_string(),
            purpose: purpose.to_string(),
            exp: (Utc::now() + Duration::seconds(ttl_secs as i64)).timestamp(),
        };
        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| AppError::internal(format!("email token error: {}", e)))
    }

    fn verify_email_token<'a>(&self, token: &'a str, expected_purpose: &'a str) -> Result<Uuid, AppError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;

        let data = decode::<EmailClaims>(token, &self.decoding_key, &validation)
            .map_err(|_| AppError::forbidden("invalid_or_expired_token"))?;

        if data.claims.purpose != expected_purpose {
            return Err(AppError::forbidden("invalid_or_expired_token"));
        }

        Uuid::parse_str(&data.claims.sub)
            .map_err(|_| AppError::forbidden("invalid_or_expired_token"))
    }

    fn access_token_ttl_secs(&self) -> u64 {
        self.access_ttl_secs
    }

    fn refresh_token_ttl_secs(&self) -> u64 {
        self.refresh_ttl_secs
    }
}
