//! `/api/admin/email-templates` — read, edit, preview and test transactional
//! email copy.
//!
//! Gated by `admin.email_templates`, not `admin.config`: the person who owns
//! the site's voice need not be handed the SMTP password on the settings page.

use axum::extract::{Extension, Path, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use validator::Validate;

use ferum_application::ports::EmailService;
use ferum_infrastructure::email::MailProvider;

use crate::app_state::AppState;
use crate::handlers::admin::api::email::{
    delivery_error, MAIL_NOT_CONFIGURED, TEST_EMAIL_COOLDOWN,
};
use crate::middleware::{AuthUser, AuthUserExt};
use crate::view_models::{DataResponse, HandlerResult};
use ferum_application::shared::AppError;

#[derive(Serialize)]
pub struct VariableResponse {
    pub name: String,
    /// `text` values are HTML-escaped, `url` values are scheme-checked, `raw`
    /// is inserted verbatim. Surfaced so an author knows which is which before
    /// wondering why their markup came out as text.
    pub kind: String,
    pub required: bool,
}

#[derive(Serialize)]
pub struct TemplateSummaryResponse {
    pub key: String,
    pub name: String,
    pub description: String,
    pub is_layout: bool,
    pub variables: Vec<VariableResponse>,
    pub customised_locales: Vec<String>,
}

#[derive(Serialize)]
pub struct TemplateResponse {
    pub key: String,
    pub locale: String,
    pub subject: String,
    pub body_html: String,
    pub customised: bool,
    /// The locale this copy actually came from, when the requested one ships
    /// none. `null` means the copy belongs to the locale asked for.
    pub inherited_from: Option<String>,
}

#[derive(Serialize)]
pub struct PreviewResponse {
    pub subject: String,
    pub html: String,
    pub text: String,
}

#[derive(Deserialize, Validate)]
pub struct SaveTemplateRequest {
    #[validate(length(min = 1, max = 500))]
    pub subject: String,
    #[validate(length(min = 1, max = 50_000))]
    pub body_html: String,
}

pub async fn list_templates(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let items = state.email_templates_uc.list(actor).await?;

    let body: Vec<TemplateSummaryResponse> = items
        .into_iter()
        .map(|t| TemplateSummaryResponse {
            key: t.key,
            name: t.name,
            description: t.description,
            is_layout: t.is_layout,
            variables: t
                .variables
                .into_iter()
                .map(|v| VariableResponse {
                    name: v.name,
                    kind: v.kind,
                    required: v.required,
                })
                .collect(),
            customised_locales: t.customised_locales,
        })
        .collect();

    Ok(Json(DataResponse::new(body)))
}

pub async fn get_template(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path((key, locale)): Path<(String, String)>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let t = state.email_templates_uc.get(actor, &key, &locale).await?;

    Ok(Json(DataResponse::new(TemplateResponse {
        key: t.key,
        locale: t.locale,
        subject: t.subject,
        body_html: t.body_html,
        customised: t.customised,
        inherited_from: t.inherited_from,
    })))
}

pub async fn save_template(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path((key, locale)): Path<(String, String)>,
    Json(body): Json<SaveTemplateRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    state
        .email_templates_uc
        .save(actor, &key, &locale, &body.subject, &body.body_html)
        .await?;
    Ok(Json(DataResponse::new(serde_json::json!({ "saved": true }))))
}

/// Drops the stored row so the compiled-in default applies again.
pub async fn reset_template(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path((key, locale)): Path<(String, String)>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    state.email_templates_uc.reset(actor, &key, &locale).await?;
    Ok(Json(DataResponse::new(serde_json::json!({ "reset": true }))))
}

/// Renders the draft in the request body, not the stored row.
///
/// POST because the copy being previewed has not been saved — a GET could not
/// carry it, and putting a 50 KB body in a query string is not an option.
pub async fn preview_template(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path((key, locale)): Path<(String, String)>,
    Json(body): Json<SaveTemplateRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;
    let rendered = state
        .email_templates_uc
        .preview(actor, &key, &locale, &body.subject, &body.body_html)
        .await?;

    Ok(Json(DataResponse::new(PreviewResponse {
        subject: rendered.subject,
        html: rendered.html,
        text: rendered.text,
    })))
}

/// Sends the draft to the acting admin's own address.
///
/// The recipient is never taken from the request, for the reason the settings
/// page's test button gives: an authenticated endpoint that mails an arbitrary
/// address is an open relay wearing an admin session. Shares that button's
/// cooldown, because each call spends real money and sending reputation.
pub async fn test_send_template(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Path((key, locale)): Path<(String, String)>,
    Json(body): Json<SaveTemplateRequest>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(e.to_string()))?;

    // **Rendered before the cooldown is taken.** `preview` is also where the
    // template is validated, and spending the 30s window on a draft that was
    // never going to send means a typo'd variable costs half a minute before it
    // can be corrected. The window still bounds real sends: `set_nx` is atomic,
    // so two valid requests racing here still leave only one holding it.
    let rendered = state
        .email_templates_uc
        .preview(actor, &key, &locale, &body.subject, &body.body_html)
        .await?;

    // Before the cooldown as well as before the transport: refusing to send is
    // not a send, so it must not spend the window either.
    let provider = state.email.provider().await;
    if provider == MailProvider::Disabled {
        return Ok(Json(DataResponse::new(serde_json::json!({
            "success": false, "sent_to": null, "error": MAIL_NOT_CONFIGURED,
        }))));
    }

    let cooldown_key = format!("admin:email-test:{}", actor.id);
    if !state
        .cache
        .set_nx(&cooldown_key, "1", TEST_EMAIL_COOLDOWN)
        .await?
    {
        return Err(AppError::TooManyRequests(TEST_EMAIL_COOLDOWN.as_secs()).into());
    }

    let user = state
        .user_repo
        .find_by_id(actor.id)
        .await?
        .ok_or(AppError::NotFound)?;

    // A failed delivery is a 200 carrying the diagnosis, matching
    // `/api/admin/email/test`: the request succeeded, and a 500 would let the
    // browser's generic handling swallow the provider's own message — which is
    // the one thing the admin pressed the button for.
    let outcome = match state.email.send(rendered.to(&user.email)).await {
        Ok(()) => serde_json::json!({
            "success": true, "sent_to": user.email, "error": null,
        }),
        Err(e) => {
            tracing::warn!(actor_id = %actor.id, template = key, "test send failed: {e}");
            serde_json::json!({
                "success": false, "sent_to": user.email, "error": delivery_error(&e),
            })
        }
    };

    Ok(Json(DataResponse::new(outcome)))
}
