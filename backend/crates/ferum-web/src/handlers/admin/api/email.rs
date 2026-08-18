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
use ferum_infrastructure::email::MailProvider;
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
pub(crate) const TEST_EMAIL_COOLDOWN: Duration = Duration::from_secs(30);

/// What to tell an admin when there is no provider to test.
///
/// A known configuration state, not a fault, so it never reaches the transport
/// — `ReloadableEmailService` would answer with an `AppError::internal`, and an
/// admin reading "internal error: mail_not_configured" learns neither what is
/// wrong nor where to fix it.
pub const MAIL_NOT_CONFIGURED: &str =
    "No mail provider is configured, so nothing can be sent. Set one up under Settings → Email.";

/// Reduces a send failure to the part an admin can act on.
///
/// **Strips the `[file:line]` that `AppError::internal` appends.** These two
/// endpoints deliberately surface the provider's own words — that is the whole
/// reason they exist — which also means `status_and_code` is not in the path to
/// collapse an `Internal` into `internal_error`. Without this the admin is
/// shown our source coordinates: the leak NF-SC-11 exists to prevent, and
/// useless to them besides.
///
/// `pub` so the web test suite can pin it, the way `resend_service::payload`
/// and `lettre::build_message` are — the invariant is about what leaves the
/// process, which nothing inside this module can observe.
pub fn delivery_error(e: &AppError) -> String {
    let text = e.to_string();
    // The `internal error: ` prefix goes too. A refused connection or an
    // unverified sending domain is the provider's verdict, not a fault in this
    // process, and labelling it as ours sends the admin looking in the wrong
    // place.
    let text = text.strip_prefix("internal error: ").unwrap_or(&text);
    match (text.rfind(" ["), text.ends_with(']')) {
        (Some(at), true) => text[..at].to_string(),
        _ => text.to_string(),
    }
}

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

    // Answered before the transport is touched: with nothing configured the
    // service returns an `AppError::internal`, and "internal error:
    // mail_not_configured" tells an admin neither what is wrong nor where to
    // fix it.
    if provider == MailProvider::Disabled {
        return Ok(Json(DataResponse::new(serde_json::json!({
            "success": false,
            "provider": provider.label(),
            "sent_to": user.email,
            "error": MAIL_NOT_CONFIGURED,
        }))));
    }

    // `AppError::internal`'s message is normally invisible to clients —
    // `status_and_code` collapses every `Internal` to `internal_error`. The
    // provider's own words ("domain not verified", "connection refused") are
    // surfaced here on purpose, because identifying the fault is the entire
    // point of the endpoint — see `delivery_error` for what is stripped first.
    //
    // That makes "no adapter's error message contains a credential" a hard
    // invariant rather than a nicety. `resend_service::error_from_status` has a
    // test for exactly this, and the SMTP adapter never formats its password.
    let outcome = match state.email.send(message.to(&user.email)).await {
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
                "error": delivery_error(&e),
            })
        }
    };

    Ok(Json(DataResponse::new(outcome)))
}
