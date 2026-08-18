//! [`DbEmailTemplateRenderer`] over an empty store, so the compiled-in catalogue
//! is what resolves.
//!
//! No database: the renderer's job is substitution, escaping and layout
//! wrapping, and a repository returning `None` exercises all three against the
//! copy that actually ships.

use std::sync::Arc;

use async_trait::async_trait;

use ferum_application::ports::EmailTemplateRenderer;
use ferum_domain::models::email_template::{template_default, EmailTemplate, LAYOUT_KEY};
use ferum_domain::repositories::email_template_repository::EmailTemplateRepository;
use ferum_domain::{AppError, Locale};
use ferum_infrastructure::email::DbEmailTemplateRenderer;

/// Nothing stored, so every lookup falls through to the shipped defaults.
struct NoRows;

#[async_trait]
impl EmailTemplateRepository for NoRows {
    async fn list(&self) -> Result<Vec<EmailTemplate>, AppError> {
        Ok(vec![])
    }
    async fn find(&self, _: &str, _: &str) -> Result<Option<EmailTemplate>, AppError> {
        Ok(None)
    }
    async fn resolve(&self, _: &str, _: &[String]) -> Result<Option<EmailTemplate>, AppError> {
        Ok(None)
    }
    async fn upsert(&self, _: &str, _: &str, _: &str, _: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn insert_if_absent(&self, _: &str, _: &str, _: &str, _: &str) -> Result<bool, AppError> {
        Ok(true)
    }
    async fn delete(&self, _: &str, _: &str) -> Result<(), AppError> {
        Ok(())
    }
}

fn renderer() -> DbEmailTemplateRenderer {
    DbEmailTemplateRenderer::new(Arc::new(NoRows))
}

/// The layout is an editable template, and the admin editor previews it through
/// the same path a message takes — which wrapped it in itself, producing a
/// document nested inside a second copy of the same document.
#[tokio::test]
async fn the_layout_is_not_wrapped_in_itself() {
    let d = template_default(LAYOUT_KEY, "en").expect("the layout ships a default");
    let out = renderer()
        .render_draft(
            LAYOUT_KEY,
            &Locale::default_locale(),
            d.subject,
            d.body_html,
            &[
                ("site_name", "Ferum".to_string()),
                ("content", "<p>BODY</p>".to_string()),
            ],
        )
        .await
        .expect("the layout renders");

    assert_eq!(
        out.html.to_lowercase().matches("<!doctype html>").count(),
        1,
        "two <html> roots is not what any message looks like:\n{}",
        out.html
    );
    assert_eq!(out.html.matches("<body").count(), 1);
}

/// The other half: a message must still get the layout, or the fix above would
/// have disabled branding for every email.
#[tokio::test]
async fn a_message_is_still_wrapped_in_the_layout() {
    let out = renderer()
        .render("email-verify", &Locale::default_locale(), &[
            ("site_name", "Ferum".to_string()),
            ("url", "https://example.test/v/TOKEN".to_string()),
        ])
        .await
        .expect("a message renders");

    assert!(
        out.html.to_lowercase().contains("<!doctype html>"),
        "the layout must still wrap a message:\n{}",
        out.html
    );
    assert!(out.html.contains("https://example.test/v/TOKEN"));
}

/// Resolution walks the recipient's chain, so a locale shipping no copy of its
/// own still sends — in the language it inherits rather than not at all.
#[tokio::test]
async fn an_unknown_locale_falls_through_to_the_default_copy() {
    let de = Locale::parse("de").expect("a well-formed tag");
    let out = renderer()
        .render("email-verify", &de, &[
            ("site_name", "Ferum".to_string()),
            ("url", "https://example.test/v/TOKEN".to_string()),
        ])
        .await
        .expect("an unshipped locale still renders");

    assert_eq!(out.subject, "Verify your email");
}
