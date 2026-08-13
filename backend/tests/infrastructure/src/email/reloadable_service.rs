//! Provider precedence and the `auto_verify` invariant.
//!
//! `auto_verify` is shared by `Arc` with `AuthUseCase`, and when it is true
//! `register()` marks the new account email-verified and promotes it to Basic
//! trust without sending anything. So a wrong value here does not fail loudly —
//! it means **nobody's email address is ever checked**, on a forum where mail is
//! working. This file exists for that one bug.
//!
//! The invariant, stated once in `sync_auto_verify` and asserted here:
//!
//! ```text
//! auto_verify == !(fixed.is_some() || transport.is_some())
//! ```

use std::sync::atomic::Ordering;
use std::sync::Arc;

use ferum_application::ports::EmailService;
use ferum_infrastructure::email::{MailProvider, ReloadableEmailService};
use ferum_test_support::mocks::email_service::RecordingEmailService;

const FROM: &str = "noreply@example.com";
const SMTP_HOST: &str = "smtp.example.com";
const SMTP_PORT: u16 = 587;

fn recording() -> Arc<RecordingEmailService> {
    Arc::new(RecordingEmailService::new())
}

/// Case 1 — nothing configured.
#[tokio::test]
async fn auto_verify_is_true_when_nothing_is_configured() {
    let svc = ReloadableEmailService::new(FROM);
    assert!(svc.auto_verify_flag().load(Ordering::Relaxed));
    assert_eq!(svc.provider().await, MailProvider::Disabled);

    // And it stays true after a reload with no host, which is what startup does.
    svc.reload(None, SMTP_PORT, None, None).await.unwrap();
    assert!(svc.auto_verify_flag().load(Ordering::Relaxed));
    assert_eq!(svc.provider().await, MailProvider::Disabled);
}

/// Cases 2 and 7 — SMTP arriving, then being cleared from the settings page.
#[tokio::test]
async fn auto_verify_follows_smtp_when_no_fixed_provider() {
    let svc = ReloadableEmailService::new(FROM);

    svc.reload(Some(SMTP_HOST), SMTP_PORT, Some("u"), Some("p"))
        .await
        .unwrap();
    assert!(!svc.auto_verify_flag().load(Ordering::Relaxed));
    assert_eq!(svc.provider().await, MailProvider::Smtp);

    // An admin blanking the host in /admin/settings — `update_config` calls
    // `reload(None, …)`. With no other provider, auto-verify must come back on, or
    // registration would break outright.
    svc.reload(None, SMTP_PORT, None, None).await.unwrap();
    assert!(svc.auto_verify_flag().load(Ordering::Relaxed));
    assert_eq!(svc.provider().await, MailProvider::Disabled);
}

/// Case 3 — **the regression this whole invariant exists to prevent.**
#[tokio::test]
async fn a_fixed_provider_keeps_auto_verify_false_when_smtp_is_absent() {
    let svc = ReloadableEmailService::new(FROM).with_fixed_provider(recording());

    // False from construction: a caller that installs a provider and never reaches
    // `reload` must not auto-verify either.
    assert!(!svc.auto_verify_flag().load(Ordering::Relaxed));

    // startup.rs calls `reload(None, …)` whenever site_config has no SMTP host.
    // Deriving the flag from the transport alone would set it back to true here,
    // and the forum would silently stop verifying email while Resend sent
    // perfectly well.
    svc.reload(None, SMTP_PORT, None, None).await.unwrap();
    assert!(
        !svc.auto_verify_flag().load(Ordering::Relaxed),
        "a fixed provider must keep auto-verify off even with no SMTP configured"
    );
    assert_eq!(svc.provider().await, MailProvider::Fixed);
}

/// Case 5 — an admin clears SMTP while a fixed provider is active.
#[tokio::test]
async fn clearing_smtp_does_not_reenable_auto_verify_while_a_fixed_provider_is_active() {
    let svc = ReloadableEmailService::new(FROM).with_fixed_provider(recording());

    svc.reload(Some(SMTP_HOST), SMTP_PORT, Some("u"), Some("p"))
        .await
        .unwrap();
    assert!(!svc.auto_verify_flag().load(Ordering::Relaxed));

    svc.reload(None, SMTP_PORT, None, None).await.unwrap();
    assert!(!svc.auto_verify_flag().load(Ordering::Relaxed));
    // The fixed provider still wins for reporting, too.
    assert_eq!(svc.provider().await, MailProvider::Fixed);
}

/// Case 9 — a failed reload must never strand a stale flag.
///
/// **This one cannot be driven end to end, and the reason is worth recording.**
/// `reload`'s error arm needs `LettreEmailService::new` to fail, and it does not:
/// lettre's `starttls_relay` returns `Result` but only stores the domain, leaving
/// hostname validation to connect time (see
/// `lettre_service::a_malformed_hostname_still_builds_because_tls_validates_at_connect_time`).
/// The only build-time failure is loading the root certificate store, which a test
/// cannot provoke.
///
/// So `sync_auto_verify` on the error path is defensive, not a fix for a live bug —
/// an earlier reading of this code claimed otherwise. It is kept because the flag
/// must be a function of state rather than of which path last ran, and because if
/// that arm ever does become reachable the failure is silent in the worst
/// direction: mail visibly "disabled" in the log while every registration is
/// quietly auto-verified.
///
/// What is asserted instead is the invariant itself across every transition that
/// *is* reachable — which is what the error arm would have to preserve too.
#[tokio::test]
async fn auto_verify_is_always_a_function_of_configured_providers() {
    // Without a fixed provider: it tracks the transport, in both directions and
    // repeatedly, so no sequence of admin saves can leave it stranded.
    let svc = ReloadableEmailService::new(FROM);
    for _ in 0..3 {
        svc.reload(Some(SMTP_HOST), SMTP_PORT, Some("u"), Some("p"))
            .await
            .unwrap();
        assert!(!svc.auto_verify_flag().load(Ordering::Relaxed));
        assert_eq!(svc.provider().await, MailProvider::Smtp);

        svc.reload(None, SMTP_PORT, None, None).await.unwrap();
        assert!(svc.auto_verify_flag().load(Ordering::Relaxed));
        assert_eq!(svc.provider().await, MailProvider::Disabled);
    }

    // With one: it is pinned false regardless of what SMTP does.
    let fixed = ReloadableEmailService::new(FROM).with_fixed_provider(recording());
    for host in [Some(SMTP_HOST), None, Some(SMTP_HOST), None] {
        fixed.reload(host, SMTP_PORT, None, None).await.unwrap();
        assert!(
            !fixed.auto_verify_flag().load(Ordering::Relaxed),
            "auto-verify must stay off while a fixed provider is installed (host: {host:?})"
        );
        assert_eq!(fixed.provider().await, MailProvider::Fixed);
    }
}

#[tokio::test]
async fn send_prefers_the_fixed_provider_over_smtp() {
    let recorder = recording();
    let svc = ReloadableEmailService::new(FROM).with_fixed_provider(recorder.clone());

    // A live SMTP transport is configured as well. The fixed provider still takes
    // every send; the transport is kept only so unsetting the env var and
    // restarting falls back to it.
    svc.reload(Some(SMTP_HOST), SMTP_PORT, Some("u"), Some("p"))
        .await
        .unwrap();

    svc.send("member@example.com", "Subject", "<p>Body</p>")
        .await
        .unwrap();

    let sent = recorder.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to, "member@example.com");
    assert_eq!(sent[0].subject, "Subject");
    assert_eq!(sent[0].html_body, "<p>Body</p>");
}

#[tokio::test]
async fn send_without_any_provider_fails_fast() {
    let svc = ReloadableEmailService::new(FROM);
    // Rather than timing out against a placeholder host.
    let err = svc.send("member@example.com", "s", "b").await;
    assert!(err.is_err());
}

#[tokio::test]
async fn a_failing_fixed_provider_propagates_its_message() {
    // The admin test-send endpoint surfaces this string, so it has to survive the
    // hop through `ReloadableEmailService` rather than being replaced.
    let svc = ReloadableEmailService::new(FROM)
        .with_fixed_provider(Arc::new(RecordingEmailService::failing("domain not verified")));
    let err = svc.send("member@example.com", "s", "b").await.unwrap_err();
    assert!(err.to_string().contains("domain not verified"));
}

#[tokio::test]
async fn provider_labels_are_the_strings_health_and_the_template_expect() {
    // These are part of two contracts at once: the `/health/ready` JSON body and
    // the provider banner's `{% if mail_provider == "…" %}` branches. Changing one
    // silently breaks the other.
    assert_eq!(MailProvider::Fixed.label(), "resend");
    assert_eq!(MailProvider::Smtp.label(), "smtp");
    assert_eq!(MailProvider::Disabled.label(), "disabled");
}

#[tokio::test]
async fn describe_reports_the_active_provider_without_secrets() {
    let disabled = ReloadableEmailService::new(FROM);
    assert!(disabled.describe().await.contains("disabled"));

    let smtp = ReloadableEmailService::new(FROM);
    smtp.reload(Some(SMTP_HOST), SMTP_PORT, Some("apikey"), Some("SECRET-PASS"))
        .await
        .unwrap();
    let described = smtp.describe().await;
    assert!(described.contains(SMTP_HOST));
    assert!(described.contains("STARTTLS"));
    // This string goes into the startup log.
    assert!(!described.contains("SECRET-PASS"));
    assert!(!described.contains("apikey"));
}
