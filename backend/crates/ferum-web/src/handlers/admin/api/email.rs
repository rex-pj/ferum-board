//! `POST /api/admin/email/test` — prove the configured mail provider works.
//!
//! **A failed delivery is a 200 with `success: false`, never an error status.**
//! The request succeeded; what it carries is a diagnosis, and a 500 would let
//! the browser's generic handling swallow the one thing the admin came for.
//!
//! Exists because `/health/ready` deliberately never probes the provider.

use axum::extract::{Extension, State};
use axum::response::IntoResponse;
use axum::Json;
use std::time::Duration;

use ferum_application::constants::DEFAULT_SITE_NAME;
use ferum_application::permission::PermissionChecker;
use ferum_application::ports::EmailService;
use ferum_application::shared::AppError;

use crate::app_state::AppState;
use crate::middleware::locale::RequestLocale;
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::{DataResponse, HandlerResult};

/// How long an admin must wait between test sends.
///
/// The webhook test endpoint has no cooldown, and this deliberately deviates:
/// a webhook test hits the operator's own server, while a mail test costs money
/// per message and spends the sending domain's reputation. A held-down button is
/// not a threat model, it is a Tuesday.
const TEST_EMAIL_COOLDOWN: Duration = Duration::from_secs(30);

pub async fn test_email(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<RequestLocale>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    // Same gate as writing the SMTP settings this button exercises.
    PermissionChecker::can_manage_config(actor)?;

    // Per-admin, not per-IP: this is about bounding cost, and the cost follows
    // the account that triggered it.
    let cooldown_key = format!("admin:email-test:{}", actor.id);
    let fresh = state
        .cache
        .set_nx(&cooldown_key, "1", TEST_EMAIL_COOLDOWN)
        .await?;
    if !fresh {
        return Err(AppError::TooManyRequests(TEST_EMAIL_COOLDOWN.as_secs()).into());
    }

    // The recipient is the acting admin's own address, never taken from the
    // request body. An authenticated endpoint that sends mail to an arbitrary
    // address is an open relay wearing an admin session, and it would let one
    // compromised admin account burn the domain's sending reputation.
    //
    // `AuthUser` carries permissions and trust level but not the email address,
    // so this is a lookup rather than a field read.
    let user = state
        .user_repo
        .find_by_id(actor.id)
        .await?
        .ok_or(AppError::NotFound)?;

    let provider = state.email.provider().await;
    let site_name = state
        .site_config_cache
        .read()
        .await
        .get("site_name")
        .filter(|v| !v.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_SITE_NAME.to_string());
    let message = state
        .email_renderer
        .render(
            "email-test",
            &req_locale.locale,
            &[
                ("site_name", site_name),
                ("provider", provider.label().to_string()),
            ],
        )
        .await?;

    // `AppError::internal`'s message is normally invisible to clients —
    // `status_and_code` collapses every `Internal` to `internal_error`. It is
    // surfaced here on purpose, because diagnosing mail is the entire point of
    // the endpoint and the provider's own words ("domain not verified",
    // "connection refused") are what identify the fault.
    //
    // That makes "no adapter's error message contains a credential" a hard
    // invariant rather than a nicety. `resend_service::error_from_status` has a
    // test for exactly this, and the SMTP adapter never formats its password.
    let outcome = match state
        .email
        .send(&user.email, &message.subject, &message.html)
        .await
    {
        Ok(()) => serde_json::json!({
            "success": true,
            "provider": provider.label(),
            "sent_to": user.email,
            "error": null,
        }),
        Err(e) => {
            // WARN, not ERROR: an admin deliberately probing a misconfigured
            // provider is expected operation, not a fault in this process.
            tracing::warn!(actor_id = %actor.id, provider = provider.label(), "test email failed: {e}");
            serde_json::json!({
                "success": false,
                "provider": provider.label(),
                "sent_to": user.email,
                "error": e.to_string(),
            })
        }
    };

    Ok(Json(DataResponse::new(outcome)))
}
