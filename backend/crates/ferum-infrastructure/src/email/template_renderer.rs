//! [`EmailTemplateRenderer`] over the `email_templates` table.
//!
//! Reads a row per message sent, with no cache. Deliberate: the lookup is a
//! primary-key hit on a table with a handful of rows, and the SMTP round trip
//! that follows costs orders of magnitude more. A cache here would buy nothing
//! measurable and would need invalidating from the admin save path — a moving
//! part that can go wrong in the direction of sending stale copy.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use ferum_application::email_template::{html_to_text, substitute, RenderedEmail};
use ferum_application::ports::EmailTemplateRenderer;
use ferum_application::shared::AppError;
use ferum_domain::models::email_template::{template_def, template_default, LAYOUT_KEY};
use ferum_domain::repositories::email_template_repository::EmailTemplateRepository;
use ferum_domain::Locale;

pub struct DbEmailTemplateRenderer {
    templates: Arc<dyn EmailTemplateRepository>,
}

/// One template's two fields, from whichever source answered.
struct Copy {
    subject: String,
    body_html: String,
}

impl DbEmailTemplateRenderer {
    pub fn new(templates: Arc<dyn EmailTemplateRepository>) -> Self {
        Self { templates }
    }

    /// Stored row first, compiled-in default second, both over the same chain.
    ///
    /// A database error is logged and treated as "no row" rather than
    /// propagated: falling back to the shipped copy sends a correct email in the
    /// site's default language, while failing sends nothing at all — and the
    /// two most common messages here are the ones a locked-out user is waiting
    /// for.
    async fn resolve(&self, key: &str, locale: &Locale) -> Option<Copy> {
        let chain: Vec<String> = locale
            .fallback_chain()
            .into_iter()
            .map(|l| l.to_string())
            .collect();

        match self.templates.resolve(key, &chain).await {
            Ok(Some(row)) => {
                return Some(Copy {
                    subject: row.subject,
                    body_html: row.body_html,
                })
            }
            Ok(None) => {}
            Err(e) => tracing::warn!(
                template = key,
                error = ?e,
                "email template lookup failed; falling back to the compiled-in copy"
            ),
        }

        chain.iter().find_map(|candidate| {
            template_default(key, candidate).map(|d| Copy {
                subject: d.subject.to_string(),
                body_html: d.body_html.to_string(),
            })
        })
    }
}

#[async_trait]
impl EmailTemplateRenderer for DbEmailTemplateRenderer {
    async fn render(
        &self,
        key: &str,
        locale: &Locale,
        values: &[(&str, String)],
    ) -> Result<RenderedEmail, AppError> {
        let copy = self
            .resolve(key, locale)
            .await
            .ok_or_else(|| AppError::internal(format!("no copy for email template {key}")))?;

        self.render_draft(key, locale, &copy.subject, &copy.body_html, values)
            .await
    }

    async fn render_draft(
        &self,
        key: &str,
        locale: &Locale,
        subject_tpl: &str,
        body_tpl: &str,
        values: &[(&str, String)],
    ) -> Result<RenderedEmail, AppError> {
        let def = template_def(key)
            .ok_or_else(|| AppError::internal(format!("unknown email template {key}")))?;
        let map: HashMap<&str, String> = values.iter().cloned().collect();

        // Subject unescaped, body escaped per variable kind — see `substitute`.
        let subject = substitute(subject_tpl, &map, def.vars, false);
        let body = substitute(body_tpl, &map, def.vars, true);

        // From the message body, not the wrapped document: the layout is chrome,
        // and its markup adds nothing a plain-text reader wants.
        let text = html_to_text(&body);

        // **The layout is never wrapped in itself.** It is an editable template
        // like any other, and the admin editor previews it through this same
        // path — wrapping produced a document nested inside a second copy of the
        // same document, two `<html>` roots, which is not what any message looks
        // like. Rendering it alone is also the honest preview: what the admin is
        // editing *is* the outer document.
        let html = if key == LAYOUT_KEY {
            body
        } else {
            match self.resolve(LAYOUT_KEY, locale).await {
                Some(layout) => {
                    let layout_def = template_def(LAYOUT_KEY).ok_or_else(|| {
                        AppError::internal("the layout is missing from the template catalogue")
                    })?;
                    let mut layout_values: HashMap<&str, String> = HashMap::new();
                    if let Some(site_name) = map.get("site_name") {
                        layout_values.insert("site_name", site_name.clone());
                    }
                    // `content` is declared `Raw`, so the already-escaped body is
                    // inserted verbatim. Escaping again would show the recipient
                    // the markup of their own email.
                    layout_values.insert("content", body);
                    substitute(&layout.body_html, &layout_values, layout_def.vars, true)
                }
                // A missing layout must not cost the message. The body alone is a
                // complete, if unbranded, email.
                None => body,
            }
        };

        Ok(RenderedEmail {
            subject,
            html,
            text,
        })
    }
}
