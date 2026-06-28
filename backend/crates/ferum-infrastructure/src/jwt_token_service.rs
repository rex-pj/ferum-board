use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ferum_application::constants::{PASSWORD_RESET_TOKEN_TTL_SECS, REFRESH_TOKEN_TTL_SECS};
use ferum_application::ports::{AccessTokenClaims, TokenService};
use ferum_application::shared::AppError;

pub struct JwtTokenService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl JwtTokenService {
    pub fn new(secret: &str) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
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
            exp: (Utc::now() + Duration::seconds(REFRESH_TOKEN_TTL_SECS as i64)).timestamp(),
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

    fn mint_email_token(&self, user_id: Uuid, purpose: &str) -> Result<String, AppError> {
        let claims = EmailClaims {
            sub: user_id.to_string(),
            purpose: purpose.to_string(),
            exp: (Utc::now() + Duration::seconds(PASSWORD_RESET_TOKEN_TTL_SECS as i64)).timestamp(),
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
}
