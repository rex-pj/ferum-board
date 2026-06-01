use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::view_models::auth::UserResponse;
use ferum_domain::models::user::User;

// ─── Status ───────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SetupStatusResponse {
    pub needs_setup: bool,
}

// ─── Run Setup Request ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Validate)]
pub struct RunSetupRequest {
    #[validate(length(min = 3, max = 30, message = "Username must be 3–30 characters"))]
    pub admin_username: String,
    #[validate(email(message = "Invalid email address"))]
    pub admin_email: String,
    #[validate(length(min = 8, message = "Password must be at least 8 characters"))]
    pub admin_password: String,
    pub config: Option<SetupConfigRequest>,
    #[serde(default)]
    pub seed_example_data: bool,
}

#[derive(Debug, Deserialize)]
pub struct SetupConfigRequest {
    pub site_name: Option<String>,
    pub site_tagline: Option<String>,
    pub primary_color: Option<String>,
    pub registration_open: Option<bool>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_user: Option<String>,
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
