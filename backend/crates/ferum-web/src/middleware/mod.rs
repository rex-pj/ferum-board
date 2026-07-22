pub mod auth;
pub mod csrf;
pub mod locale;
pub mod rate_limit;
pub mod security_headers;
pub mod setup_guard;

pub use ferum_domain::AuthUser;

/// Extension trait that unwraps `Option<AuthUser>` with a typed 401 error.
/// Import it alongside `AuthUser` in any API handler that requires authentication.
pub trait AuthUserExt {
    fn require_auth(&self) -> Result<&AuthUser, ferum_domain::AppError>;
}

impl AuthUserExt for Option<AuthUser> {
    fn require_auth(&self) -> Result<&AuthUser, ferum_domain::AppError> {
        self.as_ref().ok_or(ferum_domain::AppError::Unauthorized)
    }
}
