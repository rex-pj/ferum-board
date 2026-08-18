//! Editing transactional email copy from the admin panel.

use std::sync::Arc;

use ferum_domain::models::email_template::{
    template_def, template_default, EmailTemplateDef, EMAIL_TEMPLATES,
};
use ferum_domain::repositories::email_template_repository::EmailTemplateRepository;
use ferum_domain::{AuthUser, Locale};

use crate::email_template::{validate_template, RenderedEmail};
use crate::permission::PermissionChecker;
use crate::ports::EmailTemplateRenderer;
use crate::shared::AppError;

/// One template in one locale, as the editor sees it.
pub struct TemplateView {
    pub key: String,
    pub locale: String,
    pub subject: String,
    pub body_html: String,
    /// False when the copy shown is the compiled-in default rather than a
    /// stored row. Drives the "Customised" badge and whether Reset does
    /// anything.
    pub customised: bool,
    /// Set when the copy shown belongs to another locale in the fallback
    /// chain — this one ships none of its own. The editor must say so, or it
    /// shows English under a Vietnamese heading with no explanation.
    pub inherited_from: Option<String>,
}

/// A template's catalogue entry plus which locales have been edited.
pub struct TemplateSummary {
    pub key: String,
    pub name: String,
    pub description: String,
    /// The layout is chrome, not a message, and the editor groups it apart.
    pub is_layout: bool,
    pub variables: Vec<VariableView>,
    pub customised_locales: Vec<String>,
}

pub struct VariableView {
    pub name: String,
    /// `text`, `url` or `raw` — shown so the author knows a value will be
    /// escaped rather than rendered.
    pub kind: String,
    pub required: bool,
}

pub struct EmailTemplateUseCase {
    templates: Arc<dyn EmailTemplateRepository>,
    renderer: Arc<dyn EmailTemplateRenderer>,
}

impl EmailTemplateUseCase {
    pub fn new(
        templates: Arc<dyn EmailTemplateRepository>,
        renderer: Arc<dyn EmailTemplateRenderer>,
    ) -> Self {
        Self {
            templates,
            renderer,
        }
    }

    /// The catalogue, annotated with which locales carry edited copy.
    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id))]
    pub async fn list(&self, actor: &AuthUser) -> Result<Vec<TemplateSummary>, AppError> {
        PermissionChecker::can_manage_email_templates(actor)?;

        let stored = self.templates.list().await?;
        Ok(EMAIL_TEMPLATES
            .iter()
            .map(|def| TemplateSummary {
                key: def.key.to_string(),
                name: def.name.to_string(),
                description: def.description.to_string(),
                is_layout: def.key == ferum_domain::models::email_template::LAYOUT_KEY,
                variables: variable_views(def),
                customised_locales: stored
                    .iter()
                    .filter(|t| t.key == def.key)
                    .map(|t| t.locale.clone())
                    .collect(),
            })
            .collect())
    }

    /// The copy for one locale — the stored row, or the compiled-in default
    /// when there is none.
    ///
    /// **Falls back to the default rather than 404ing on a missing row.** The
    /// editor's job is to show what would be sent, and with no row that is the
    /// shipped copy; a 404 would suggest the email does not exist.
    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id))]
    pub async fn get(
        &self,
        actor: &AuthUser,
        key: &str,
        locale: &str,
    ) -> Result<TemplateView, AppError> {
        PermissionChecker::can_manage_email_templates(actor)?;
        if template_def(key).is_none() {
            return Err(AppError::invalid("unknown_email_template"));
        }
        let locale = canonical_locale(locale)?;

        if let Some(row) = self.templates.find(key, &locale).await? {
            return Ok(TemplateView {
                key: row.key,
                locale: row.locale,
                subject: row.subject,
                body_html: row.body_html,
                customised: true,
                inherited_from: None,
            });
        }

        if let Some(d) = template_default(key, &locale) {
            return Ok(TemplateView {
                key: key.to_string(),
                locale,
                subject: d.subject.to_string(),
                body_html: d.body_html.to_string(),
                customised: false,
                inherited_from: None,
            });
        }

        // **This locale ships no copy of its own, so show what would actually be
        // sent.** Returning blanks here was worse than useless: the editor said
        // "Default" over two empty boxes while a real send fell through the chain
        // and delivered English. `inherited_from` names the locale the copy came
        // from so the editor can say so — saving still writes *this* locale,
        // which is how an inherited template gets overridden.
        let chain = Locale::parse(&locale)
            .map(|l| l.fallback_chain())
            .unwrap_or_default();
        let chain: Vec<String> = chain.into_iter().map(|l| l.to_string()).collect();

        if let Some(row) = self.templates.resolve(key, &chain).await? {
            return Ok(TemplateView {
                key: key.to_string(),
                locale,
                subject: row.subject,
                body_html: row.body_html,
                customised: false,
                inherited_from: Some(row.locale),
            });
        }

        let inherited = chain
            .iter()
            .find_map(|tag| template_default(key, tag).map(|d| (tag.clone(), d)));

        Ok(match inherited {
            Some((tag, d)) => TemplateView {
                key: key.to_string(),
                locale,
                subject: d.subject.to_string(),
                body_html: d.body_html.to_string(),
                customised: false,
                inherited_from: Some(tag),
            },
            // Nothing anywhere in the chain. Only reachable for a template added
            // to the catalogue with no default at all, which the
            // `every_declared_template_has_english_copy` test forbids.
            None => TemplateView {
                key: key.to_string(),
                locale,
                subject: String::new(),
                body_html: String::new(),
                customised: false,
                inherited_from: None,
            },
        })
    }

    #[tracing::instrument(skip(self, actor, subject, body_html), fields(user_id = %actor.id))]
    pub async fn save(
        &self,
        actor: &AuthUser,
        key: &str,
        locale: &str,
        subject: &str,
        body_html: &str,
    ) -> Result<(), AppError> {
        PermissionChecker::can_manage_email_templates(actor)?;
        // On save, never on send: a template that cannot render is a mistake to
        // reject while its author is looking at it.
        validate_template(key, subject, body_html)?;
        self.templates
            .upsert(key, &canonical_locale(locale)?, subject, body_html)
            .await
    }

    /// Drops the stored row so the compiled-in default takes over again.
    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id))]
    pub async fn reset(
        &self,
        actor: &AuthUser,
        key: &str,
        locale: &str,
    ) -> Result<(), AppError> {
        PermissionChecker::can_manage_email_templates(actor)?;
        // Same guard as `get`: a typo'd key silently deleting nothing, and
        // reporting success for it, is the shape that hides a broken caller.
        if template_def(key).is_none() {
            return Err(AppError::invalid("unknown_email_template"));
        }
        self.templates.delete(key, &canonical_locale(locale)?).await
    }

    /// Renders draft copy with the catalogue's sample values.
    ///
    /// Goes through the same renderer a real send uses, so what the admin sees
    /// is evidence rather than an approximation.
    #[tracing::instrument(skip(self, actor, subject, body_html), fields(user_id = %actor.id))]
    pub async fn preview(
        &self,
        actor: &AuthUser,
        key: &str,
        locale: &str,
        subject: &str,
        body_html: &str,
    ) -> Result<RenderedEmail, AppError> {
        PermissionChecker::can_manage_email_templates(actor)?;
        validate_template(key, subject, body_html)?;

        let def = template_def(key).ok_or_else(|| AppError::invalid("unknown_email_template"))?;
        let parsed =
            Locale::parse(locale).ok_or_else(|| AppError::invalid("invalid_locale"))?;

        self.renderer
            .render_draft(key, &parsed, subject, body_html, &sample_values(def))
            .await
    }
}

/// Sample values for every variable a template declares.
pub fn sample_values(def: &EmailTemplateDef) -> Vec<(&'static str, String)> {
    def.vars
        .iter()
        .map(|v| (v.name, v.sample.to_string()))
        .collect()
}

fn variable_views(def: &EmailTemplateDef) -> Vec<VariableView> {
    def.vars
        .iter()
        .map(|v| VariableView {
            name: v.name.to_string(),
            kind: match v.kind {
                ferum_domain::models::email_template::VarKind::Text => "text",
                ferum_domain::models::email_template::VarKind::Url => "url",
                ferum_domain::models::email_template::VarKind::Raw => "raw",
            }
            .to_string(),
            required: v.required_in_body,
        })
        .collect()
}

/// Normalises a locale tag before it is stored, rejecting one that is not a
/// tag at all.
///
/// **Rejected rather than stored verbatim.** A row under a tag no recipient
/// ever resolves to looks saved in the editor and is never used — the failure
/// gives no signal at the moment it is caused, and the next signal is an email
/// that went out in the wrong language.
fn canonical_locale(locale: &str) -> Result<String, AppError> {
    Locale::parse(locale)
        .map(|l| l.to_string())
        .ok_or_else(|| AppError::invalid("invalid_locale"))
}
