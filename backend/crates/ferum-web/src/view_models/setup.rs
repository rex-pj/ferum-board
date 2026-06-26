use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use crate::view_models::auth::UserResponse;
use ferum_domain::models::user::User;

// ─── Status ───────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SetupStatusResponse {
    pub needs_setup: bool,
}

// ─── Run Setup Request ────────────────────────────────────────────────────────

fn validate_primary_color(val: &str) -> Result<(), ValidationError> {
    crate::view_models::validators::hex_color(val)
}

#[derive(Debug, Deserialize, Validate)]
pub struct RunSetupRequest {
    #[validate(length(min = 3, max = 30, message = "Username must be 3–30 characters"))]
    pub admin_username: String,
    #[validate(email(message = "Invalid email address"))]
    pub admin_email: String,
    #[validate(
        length(min = 8, message = "Password must be at least 8 characters"),
        custom(function = "crate::view_models::validators::password_complexity")
    )]
    pub admin_password: String,
    #[validate(nested)]
    pub config: Option<SetupConfigRequest>,
    #[serde(default)]
    pub seed_example_data: bool,
}

#[derive(Debug, Deserialize, Validate)]
pub struct SetupConfigRequest {
    #[validate(length(max = 100, message = "Site name must be at most 100 characters"))]
    pub site_name: Option<String>,
    #[validate(length(max = 255, message = "Site tagline must be at most 255 characters"))]
    pub site_tagline: Option<String>,
    #[validate(custom(function = "validate_primary_color"))]
    pub primary_color: Option<String>,
    pub registration_open: Option<bool>,
    #[validate(length(max = 253, message = "SMTP host must be at most 253 characters"))]
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    #[validate(length(max = 254, message = "SMTP user must be at most 254 characters"))]
    pub smtp_user: Option<String>,
    #[validate(length(max = 1024, message = "SMTP password must be at most 1024 characters"))]
    pub smtp_pass: Option<String>,
}

// ─── Run Setup Response ───────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SetupRunResponse {
    pub user: UserResponse,
    pub access_token: String,
}

impl SetupRunResponse {
    pub fn new(user: User, access_token: String) -> Self {
        Self {
            user: UserResponse::from(user),
            access_token,
        }
    }
}
