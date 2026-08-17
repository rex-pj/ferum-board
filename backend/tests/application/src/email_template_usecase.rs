//! Tests for [`EmailTemplateUseCase`].
//!
//! The properties worth pinning are the ones a passing build would not reveal:
//! that every entry point checks the permission, that `get` shows the *shipped*
//! copy rather than a sibling locale's, and that a locale tag which no recipient
//! resolves to is refused instead of stored.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use ferum_application::email_template::RenderedEmail;
use ferum_application::ports::EmailTemplateRenderer;
use ferum_application::usecases::email_template_usecase::EmailTemplateUseCase;
use ferum_domain::models::email_template::EmailTemplate;
use ferum_domain::models::role::perm;
use ferum_domain::repositories::email_template_repository::EmailTemplateRepository;
use ferum_domain::{AppError, AuthUser, Locale};
use ferum_test_support::fixtures::AuthUserBuilder;

// ─── Doubles ──────────────────────────────────────────────────────────────────

#[derive(Default)]
struct FakeTemplates {
    rows: Mutex<Vec<EmailTemplate>>,
    upserts: Mutex<Vec<(String, String)>>,
    deletes: Mutex<Vec<(String, String)>>,
}

impl FakeTemplates {
    fn with_row(key: &str, locale: &str, subject: &str) -> Arc<Self> {
        let f = Self::default();
        f.rows.lock().unwrap().push(EmailTemplate {
            key: key.into(),
            locale: locale.into(),
            subject: subject.into(),
            body_html: "<p>stored {{ url }}</p>".into(),
            updated_at: chrono::Utc::now(),
        });
        Arc::new(f)
    }
}

#[async_trait]
impl EmailTemplateRepository for FakeTemplates {
    async fn list(&self) -> Result<Vec<EmailTemplate>, AppError> {
        Ok(self.rows.lock().unwrap().clone())
    }
    async fn find(&self, key: &str, locale: &str) -> Result<Option<EmailTemplate>, AppError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.key == key && r.locale == locale)
            .cloned())
    }
    async fn resolve(&self, _: &str, _: &[String]) -> Result<Option<EmailTemplate>, AppError> {
        Ok(None)
    }
    async fn upsert(&self, key: &str, locale: &str, _: &str, _: &str) -> Result<(), AppError> {
        self.upserts
            .lock()
            .unwrap()
            .push((key.to_string(), locale.to_string()));
        Ok(())
    }
    async fn insert_if_absent(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<bool, AppError> {
        Ok(true)
    }
    async fn delete(&self, key: &str, locale: &str) -> Result<(), AppError> {
        self.deletes
            .lock()
            .unwrap()
            .push((key.to_string(), locale.to_string()));
        Ok(())
    }
}

/// Echoes what it was asked to render, so a test can assert the use case passed
/// the *draft* through rather than the stored row.
struct EchoRenderer;

#[async_trait]
impl EmailTemplateRenderer for EchoRenderer {
    async fn render(
        &self,
        key: &str,
        locale: &Locale,
        values: &[(&str, String)],
    ) -> Result<RenderedEmail, AppError> {
        self.render_draft(key, locale, "stored subject", "stored body", values)
            .await
    }
    async fn render_draft(
        &self,
        _: &str,
        _: &Locale,
        subject: &str,
        body_html: &str,
        values: &[(&str, String)],
    ) -> Result<RenderedEmail, AppError> {
        Ok(RenderedEmail {
            subject: subject.to_string(),
            html: format!("{body_html}|{}", values.len()),
            text: String::new(),
        })
    }
}

fn use_case(templates: Arc<FakeTemplates>) -> EmailTemplateUseCase {
    EmailTemplateUseCase::new(templates, Arc::new(EchoRenderer))
}

fn editor() -> AuthUser {
    AuthUserBuilder::member()
        .with_perms(&[perm::ADMIN_EMAIL_TEMPLATES])
        .build()
}

/// Holds the settings permission and nothing else — the case the split exists
/// to distinguish.
fn outsider() -> AuthUser {
    AuthUserBuilder::member()
        .with_perms(&[perm::ADMIN_CONFIG])
        .build()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// `admin.config` must not carry email editing, or splitting the permission
/// bought nothing.
#[tokio::test]
async fn admin_config_alone_does_not_grant_email_editing() {
    let uc = use_case(Arc::new(FakeTemplates::default()));
    let who = outsider();

    assert!(uc.list(&who).await.is_err(), "list must be gated");
    assert!(uc.get(&who, "email-verify", "en").await.is_err(), "get must be gated");
    assert!(
        uc.save(&who, "email-verify", "en", "s", "<p>{{ url }}</p>").await.is_err(),
        "save must be gated"
    );
    assert!(uc.reset(&who, "email-verify", "en").await.is_err(), "reset must be gated");
    assert!(
        uc.preview(&who, "email-verify", "en", "s", "<p>{{ url }}</p>").await.is_err(),
        "preview must be gated"
    );
}

#[tokio::test]
async fn get_returns_the_stored_row_when_there_is_one() {
    let uc = use_case(FakeTemplates::with_row("email-verify", "en", "Edited"));

    let view = uc.get(&editor(), "email-verify", "en").await.unwrap();
    assert_eq!(view.subject, "Edited");
    assert!(view.customised, "a stored row must be reported as customised");
}

/// Shows this locale's shipped copy, never a sibling's. Falling back to `vi`
/// here would make Save silently fork the two languages.
#[tokio::test]
async fn get_falls_back_to_the_shipped_default_for_this_locale_only() {
    let uc = use_case(Arc::new(FakeTemplates::default()));

    let en = uc.get(&editor(), "email-verify", "en").await.unwrap();
    assert_eq!(en.subject, "Verify your email");
    assert!(!en.customised);

    let vi = uc.get(&editor(), "email-verify", "vi").await.unwrap();
    assert_eq!(vi.subject, "Xác minh địa chỉ email của bạn");
}

#[tokio::test]
async fn get_refuses_a_key_that_is_not_in_the_catalogue() {
    let uc = use_case(Arc::new(FakeTemplates::default()));
    assert!(uc.get(&editor(), "email-nope", "en").await.is_err());
}

#[tokio::test]
async fn save_validates_before_it_writes() {
    let templates = Arc::new(FakeTemplates::default());
    let uc = use_case(templates.clone());

    let err = uc
        .save(&editor(), "email-notify-reply", "en", "s", "<p><a href=\"{{ url }}\">x</a></p>")
        .await
        .expect_err("a missing unsubscribe link must be refused");
    assert!(format!("{err:?}").contains("missing_variable"), "got: {err:?}");
    assert!(
        templates.upserts.lock().unwrap().is_empty(),
        "nothing may be written when validation fails"
    );
}

/// A tag stored in a form no recipient resolves to looks saved and is never
/// used, so it is refused rather than normalised away silently.
#[tokio::test]
async fn save_refuses_a_locale_that_is_not_a_language_tag() {
    let templates = Arc::new(FakeTemplates::default());
    let uc = use_case(templates.clone());

    assert!(
        uc.save(&editor(), "email-verify", "not a locale", "s", "<p>{{ url }}</p>")
            .await
            .is_err(),
        "a malformed tag must be refused"
    );
    assert!(templates.upserts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn save_stores_the_canonical_locale_form() {
    let templates = Arc::new(FakeTemplates::default());
    let uc = use_case(templates.clone());

    uc.save(&editor(), "email-verify", "VI", "s", "<p>{{ url }}</p>")
        .await
        .expect("a well-formed tag saves");

    let written = templates.upserts.lock().unwrap().clone();
    assert_eq!(
        written,
        vec![("email-verify".to_string(), "vi".to_string())],
        "the tag must be canonicalised, or the row matches no recipient"
    );
}

#[tokio::test]
async fn reset_deletes_the_row_for_that_locale() {
    let templates = Arc::new(FakeTemplates::default());
    let uc = use_case(templates.clone());

    uc.reset(&editor(), "email-verify", "vi").await.unwrap();

    assert_eq!(
        templates.deletes.lock().unwrap().clone(),
        vec![("email-verify".to_string(), "vi".to_string())]
    );
}

/// The preview must render what is in the textarea, not what is stored — that
/// is the entire reason it exists.
#[tokio::test]
async fn preview_renders_the_draft_not_the_stored_row() {
    let uc = use_case(FakeTemplates::with_row("email-verify", "en", "Stored"));

    let out = uc
        .preview(&editor(), "email-verify", "en", "Draft subject", "<p>draft {{ url }}</p>")
        .await
        .unwrap();

    assert_eq!(out.subject, "Draft subject");
    assert!(out.html.starts_with("<p>draft {{ url }}</p>"), "got: {}", out.html);
}

#[tokio::test]
async fn preview_supplies_a_sample_for_every_declared_variable() {
    let uc = use_case(Arc::new(FakeTemplates::default()));

    let out = uc
        .preview(
            &editor(),
            "email-notify-reply",
            "en",
            "s",
            "<p><a href=\"{{ url }}\">x</a> {{ unsubscribe_url }}</p>",
        )
        .await
        .unwrap();

    // `email-notify-reply` declares six variables; the echo renderer reports
    // how many values it was handed.
    assert!(out.html.ends_with("|6"), "got: {}", out.html);
}

#[tokio::test]
async fn list_marks_which_locales_have_been_edited() {
    let uc = use_case(FakeTemplates::with_row("email-verify", "vi", "Đã sửa"));

    let items = uc.list(&editor()).await.unwrap();
    let verify = items.iter().find(|t| t.key == "email-verify").unwrap();
    assert_eq!(verify.customised_locales, vec!["vi".to_string()]);

    let reset = items.iter().find(|t| t.key == "email-reset").unwrap();
    assert!(reset.customised_locales.is_empty());
}
