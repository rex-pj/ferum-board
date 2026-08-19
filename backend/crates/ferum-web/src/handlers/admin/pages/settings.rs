use axum::extract::State;
use axum::response::IntoResponse;
use axum::Extension;
use serde::Serialize;
use std::collections::HashMap;
use tera::Context;

use super::super::{render_admin, require_admin, site_ctx};
use crate::app_state::AppState;
use crate::handlers::admin::api::config::{split_secrets, SMTP_PASS_KEY};
use crate::handlers::pages::{require_page_auth, PageError};
use crate::middleware::AuthUser;
use crate::view_models::page_context::CurrentUserCtx;
use ferum_domain::site_text;

/// One editable copy field inside one language pane.
///
/// The key is composed here rather than in the template because
/// `site_text::localized_key` owns that rule, and a template concatenating it itself
/// would be a second definition of it — the failure mode being a field that saves to
/// a key nothing reads.
#[derive(Serialize)]
struct CopyField {
    /// Full storage key; also the `cfg-` id suffix, so `saveSettings` collects it
    /// with no per-field JS.
    key: String,
    value: String,
    /// The source-locale copy, rendered as this field's placeholder so a blank
    /// translation shows the text it will actually fall back to. Empty on the source
    /// pane, which has no fallback of its own.
    fallback: String,
}

/// One language pane of the Site Copy card.
#[derive(Serialize)]
struct LocaleCopy {
    tag: String,
    /// The language in its own language, for an admin who does not read it.
    name: String,
    /// True for `Locale::DEFAULT_TAG`. Its pane holds the bare keys, and every other
    /// pane falls back to it — the tab strip badges it so that is visible.
    is_source: bool,
    /// Keyed by base config key, so a template addresses one field by name
    /// (`loc.fields.site_tagline`) and keeps that field's label, help text and length
    /// cap beside it instead of driving them from data.
    fields: HashMap<&'static str, CopyField>,
}

/// The Site Copy card's language panes, source locale first.
///
/// One pane per **installed** locale, not per *enabled* one — `negotiate_locale`
/// resolves against the installed roster, so a disabled locale can still be served
/// from `Accept-Language`, and offering no pane for it would leave copy that is
/// reachable but not editable. The email-template editor lists locales the same way.
///
/// Order follows `available_locales`, which is source-first, so the pane that opens is
/// the one every other falls back to.
fn site_copy_locales(state: &AppState, configs: &HashMap<String, String>) -> Vec<LocaleCopy> {
    let value_of = |key: &str| configs.get(key).cloned().unwrap_or_default();
    state
        .translator
        .available_locales()
        .iter()
        .map(|locale| {
            let is_source = locale.is_source_locale();
            let fields = site_text::LOCALIZED_CONFIG_KEYS
                .iter()
                .map(|base| {
                    let key = site_text::localized_key(base, locale);
                    let field = CopyField {
                        value: value_of(&key),
                        fallback: if is_source {
                            String::new()
                        } else {
                            value_of(base)
                        },
                        key,
                    };
                    (*base, field)
                })
                .collect();
            LocaleCopy {
                tag: locale.to_string(),
                name: state
                    .translator
                    .translate(locale, &format!("language-name-{locale}"), &[]),
                is_source,
                fields,
            }
        })
        .collect()
}

#[tracing::instrument(skip_all)]
pub async fn settings(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Extension(req_locale): Extension<crate::middleware::locale::RequestLocale>,
) -> Result<impl IntoResponse, PageError> {
    let auth_user = require_page_auth(auth_user)?;
    require_admin(&auth_user)?;

    // The cache is the whole `site_config` table, secrets included, and this page
    // renders it wholesale rather than through `CONFIG_READABLE_KEYS`. Strip the
    // secrets here and hand the template a presence flag instead, so no future
    // edit to settings.html can print one by accident.
    let (configs, secrets_set) = split_secrets(state.site_config_cache.read().await.clone());

    let mut ctx = Context::new();
    ctx.insert("site", &site_ctx(&state, &req_locale.locale).await);
    ctx.insert(
        "current_user",
        &crate::handlers::pages::with_viewer_timezone(&state, &auth_user, CurrentUserCtx::from(&auth_user)).await,
    );
    ctx.insert("site_copy", &site_copy_locales(&state, &configs));
    ctx.insert("config", &configs);
    ctx.insert(
        "smtp_pass_set",
        secrets_set.get(SMTP_PASS_KEY).unwrap_or(&false),
    );
    // Read from the live service rather than derived from the stored settings, so
    // the page reports the provider mail actually goes through. The two disagree
    // whenever RESEND_API_KEY is set: `config.smtp_host` is still populated and
    // still shown, but nothing sends through it.
    ctx.insert("mail_provider", state.email.provider().await.label());
    // Whether Resend *can* be selected. Its credential is env-only, so this is a
    // presence flag and never the key itself — the same treatment `smtp_pass_set`
    // gets. Without it the panel could not tell "Resend is available but not
    // chosen" from "choosing Resend would send nothing".
    ctx.insert("resend_key_present", &state.resend_api_key.is_some());
    // Read-only: encryption at rest is an env-level decision, so the page reports
    // it rather than offering a control that could not take effect.
    ctx.insert("secrets_encrypted", &state.secrets_encrypted);

    render_admin(&state, &req_locale, "admin/settings.html", ctx).await
}
