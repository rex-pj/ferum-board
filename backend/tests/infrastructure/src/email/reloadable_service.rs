//! Provider selection and the `auto_verify` invariant:
//!
//! ```text
//! auto_verify == the selected provider could not be put in place
//! ```
//!
//! A wrong value fails silently — `register()` then marks accounts verified
//! without sending anything, so **nobody's address is ever checked** on a forum
//! where mail works. Note it is NOT "SMTP is absent": the interesting cases are
//! Resend selected without a key, or `Off` chosen over a working relay.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use ferum_application::ports::EmailService;
use ferum_infrastructure::email::{
    MailProvider, MailReload, ReloadableEmailService, SelectedProvider, SmtpEndpoint,
};
use ferum_test_support::mocks::email_service::RecordingEmailService;

const FROM: &str = "noreply@example.com";
const SMTP_HOST: &str = "smtp.example.com";
const SMTP_PORT: u16 = 587;

fn recording() -> Arc<RecordingEmailService> {
    Arc::new(RecordingEmailService::new())
}

fn endpoint() -> SmtpEndpoint {
    SmtpEndpoint {
        host: SMTP_HOST.to_string(),
        port: SMTP_PORT,
        username: Some("u".into()),
        password: Some("p".into()),
    }
}

/// A reload with the given selection, an SMTP endpoint only when asked for, and a
/// Resend stub only when asked for — the three axes every case below varies.
fn reload_with(
    selected: SelectedProvider,
    smtp: Option<SmtpEndpoint>,
    resend: Option<Arc<dyn EmailService>>,
) -> MailReload {
    MailReload {
        selected,
        from: FROM.to_string(),
        smtp,
        resend,
    }
}

/// Case 1 — nothing configured.
#[tokio::test]
async fn auto_verify_is_true_when_nothing_is_configured() {
    let svc = ReloadableEmailService::new(FROM);
    assert!(svc.auto_verify_flag().load(Ordering::Relaxed));
    assert_eq!(svc.provider().await, MailProvider::Disabled);

    // And it stays true after the reload startup performs with an empty table.
    svc.reload(reload_with(SelectedProvider::Smtp, None, None))
        .await
        .unwrap();
    assert!(svc.auto_verify_flag().load(Ordering::Relaxed));
    assert_eq!(svc.provider().await, MailProvider::Disabled);
}

/// Cases 2 and 7 — SMTP arriving, then being cleared from the settings page.
#[tokio::test]
async fn auto_verify_follows_smtp_when_smtp_is_selected() {
    let svc = ReloadableEmailService::new(FROM);

    svc.reload(reload_with(SelectedProvider::Smtp, Some(endpoint()), None))
        .await
        .unwrap();
    assert!(!svc.auto_verify_flag().load(Ordering::Relaxed));
    assert_eq!(svc.provider().await, MailProvider::Smtp);

    // An admin blanking the host in /admin/settings. With nothing else selected,
    // auto-verify must come back on, or registration would break outright.
    svc.reload(reload_with(SelectedProvider::Smtp, None, None))
        .await
        .unwrap();
    assert!(svc.auto_verify_flag().load(Ordering::Relaxed));
    assert_eq!(svc.provider().await, MailProvider::Disabled);
}

/// Selecting Resend, with the env credential present.
#[tokio::test]
async fn selecting_resend_makes_it_the_live_provider() {
    let svc = ReloadableEmailService::new(FROM);
    svc.reload(reload_with(
        SelectedProvider::Resend,
        Some(endpoint()),
        Some(recording()),
    ))
    .await
    .unwrap();

    assert!(!svc.auto_verify_flag().load(Ordering::Relaxed));
    assert_eq!(svc.provider().await, MailProvider::Resend);
}

/// **The case the two-enum split exists for.**
///
/// `mail_provider = resend` with no `RESEND_API_KEY` is a real state — an operator
/// selects Resend, then the key is removed from the deployment. It must resolve to
/// `Disabled` and turn auto-verify back on, not report `Resend` while sending
/// nothing. Reporting the *selection* here instead of the *effect* would leave the
/// admin panel claiming mail works while no address is ever verified.
#[tokio::test]
async fn resend_selected_without_a_key_is_disabled_not_resend() {
    let svc = ReloadableEmailService::new(FROM);
    svc.reload(reload_with(SelectedProvider::Resend, Some(endpoint()), None))
        .await
        .unwrap();

    assert_eq!(svc.provider().await, MailProvider::Disabled);
    assert!(
        svc.auto_verify_flag().load(Ordering::Relaxed),
        "an unhonourable selection must auto-verify, not pretend to send"
    );
    // Named in the log so an operator can tell this apart from "nothing set up".
    assert!(svc.describe().await.contains("RESEND_API_KEY"));
}

/// `Off` must actually mean off, even with a working relay still stored.
///
/// This is what replaced "blank the host to disable": the stored SMTP settings
/// survive being switched off, so switching back on costs no retyping.
#[tokio::test]
async fn off_disables_mail_without_discarding_the_stored_relay() {
    let svc = ReloadableEmailService::new(FROM);

    svc.reload(reload_with(SelectedProvider::Smtp, Some(endpoint()), None))
        .await
        .unwrap();
    assert_eq!(svc.provider().await, MailProvider::Smtp);

    svc.reload(reload_with(
        SelectedProvider::Off,
        Some(endpoint()),
        Some(recording()),
    ))
    .await
    .unwrap();
    assert_eq!(svc.provider().await, MailProvider::Disabled);
    assert!(svc.auto_verify_flag().load(Ordering::Relaxed));

    // And back on, from the same stored endpoint.
    svc.reload(reload_with(SelectedProvider::Smtp, Some(endpoint()), None))
        .await
        .unwrap();
    assert_eq!(svc.provider().await, MailProvider::Smtp);
    assert!(!svc.auto_verify_flag().load(Ordering::Relaxed));
}

/// The invariant across every reachable transition.
///
/// `reload`'s error arm is not driven here because it cannot be: lettre's
/// `starttls_relay` only stores the domain and validates at connect time, so the
/// sole build-time failure is loading the root certificate store.
///
/// It is structurally safe anyway — `reload` writes state once, at the end, so
/// an early return cannot leave the flag describing an unapplied config.
#[tokio::test]
async fn auto_verify_is_always_a_function_of_what_is_in_place() {
    let svc = ReloadableEmailService::new(FROM);

    // Every (selection, availability) pair, and the effect each must produce.
    let cases: [(SelectedProvider, bool, bool, MailProvider); 6] = [
        (SelectedProvider::Smtp, true, false, MailProvider::Smtp),
        (SelectedProvider::Smtp, false, false, MailProvider::Disabled),
        (SelectedProvider::Resend, false, true, MailProvider::Resend),
        (SelectedProvider::Resend, true, false, MailProvider::Disabled),
        (SelectedProvider::Off, true, true, MailProvider::Disabled),
        (SelectedProvider::Smtp, true, true, MailProvider::Smtp),
    ];

    // Twice through, so no sequence of admin saves can leave the flag stranded.
    for _ in 0..2 {
        for (selected, has_smtp, has_resend, expected) in cases {
            svc.reload(reload_with(
                selected,
                has_smtp.then(endpoint),
                has_resend.then(|| recording() as Arc<dyn EmailService>),
            ))
            .await
            .unwrap();

            assert_eq!(
                svc.provider().await,
                expected,
                "selection {selected:?} (smtp: {has_smtp}, resend: {has_resend})"
            );
            assert_eq!(
                svc.auto_verify_flag().load(Ordering::Relaxed),
                expected == MailProvider::Disabled,
                "auto-verify must be exactly 'nothing is in place' for {selected:?}"
            );
        }
    }
}

#[tokio::test]
async fn send_goes_to_the_selected_provider() {
    let recorder = recording();
    let svc = ReloadableEmailService::new(FROM);

    // An SMTP endpoint is configured as well; selecting Resend must send there and
    // leave the stored relay untouched.
    svc.reload(reload_with(
        SelectedProvider::Resend,
        Some(endpoint()),
        Some(recorder.clone()),
    ))
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
async fn a_provider_that_is_no_longer_selected_stops_receiving_sends() {
    let recorder = recording();
    let svc = ReloadableEmailService::new(FROM);

    svc.reload(reload_with(
        SelectedProvider::Resend,
        Some(endpoint()),
        Some(recorder.clone()),
    ))
    .await
    .unwrap();
    svc.send("a@example.com", "s", "b").await.unwrap();

    // Switching to Off must take the previous provider out of the send path
    // entirely — the Arc is still alive in this test, which is exactly why the
    // active slot rather than the caller decides.
    svc.reload(reload_with(SelectedProvider::Off, Some(endpoint()), Some(recorder.clone())))
        .await
        .unwrap();
    assert!(svc.send("b@example.com", "s", "b").await.is_err());
    assert_eq!(recorder.sent().len(), 1, "no send may reach a deselected provider");
}

#[tokio::test]
async fn send_without_any_provider_fails_fast() {
    let svc = ReloadableEmailService::new(FROM);
    // Rather than timing out against a placeholder host.
    let err = svc.send("member@example.com", "s", "b").await;
    assert!(err.is_err());
}

#[tokio::test]
async fn a_failing_provider_propagates_its_message() {
    // The admin test-send endpoint surfaces this string, so it has to survive the
    // hop through `ReloadableEmailService` rather than being replaced.
    let svc = ReloadableEmailService::new(FROM);
    svc.reload(reload_with(
        SelectedProvider::Resend,
        None,
        Some(Arc::new(RecordingEmailService::failing("domain not verified"))),
    ))
    .await
    .unwrap();

    let err = svc.send("member@example.com", "s", "b").await.unwrap_err();
    assert!(err.to_string().contains("domain not verified"));
}

#[tokio::test]
async fn provider_labels_are_the_strings_health_and_the_template_expect() {
    // These are part of two contracts at once: the `/health/ready` JSON body and
    // the settings page's `{% if mail_provider == "…" %}` branches. Changing one
    // silently breaks the other.
    assert_eq!(MailProvider::Resend.label(), "resend");
    assert_eq!(MailProvider::Smtp.label(), "smtp");
    assert_eq!(MailProvider::Disabled.label(), "disabled");
}

#[tokio::test]
async fn stored_selection_values_round_trip_and_unknown_falls_back_to_smtp() {
    for expected in [
        SelectedProvider::Smtp,
        SelectedProvider::Resend,
        SelectedProvider::Off,
    ] {
        assert_eq!(SelectedProvider::from_label(expected.label()), expected);
    }

    // Unknown falls to Smtp, deliberately NOT to Off: both are safe, but `Smtp`
    // then sends when a host is stored, whereas `Off` would silently stop a
    // working forum's mail because one row was mistyped.
    for raw in ["", "  ", "sendgrid", "SMTP", "Resend", "OFF"] {
        assert_eq!(
            SelectedProvider::from_label(raw),
            SelectedProvider::Smtp,
            "unrecognised {raw:?} must fall back to SMTP"
        );
    }
}

#[tokio::test]
async fn describe_reports_the_active_provider_without_secrets() {
    let disabled = ReloadableEmailService::new(FROM);
    assert!(disabled.describe().await.contains("disabled"));

    let smtp = ReloadableEmailService::new(FROM);
    smtp.reload(reload_with(
        SelectedProvider::Smtp,
        Some(SmtpEndpoint {
            host: SMTP_HOST.to_string(),
            port: SMTP_PORT,
            username: Some("apikey".into()),
            password: Some("SECRET-PASS".into()),
        }),
        None,
    ))
    .await
    .unwrap();

    let described = smtp.describe().await;
    assert!(described.contains(SMTP_HOST));
    assert!(described.contains("STARTTLS"));
    // This string goes into the startup log.
    assert!(!described.contains("SECRET-PASS"));
    assert!(!described.contains("apikey"));
}

#[tokio::test]
async fn the_from_address_is_reported_and_follows_a_reload() {
    // The settings page reads this to show the real sender rather than the name of
    // an environment variable, and it changes with `from_email`.
    let svc = ReloadableEmailService::new(FROM);
    assert_eq!(svc.from().await, FROM);

    svc.reload(MailReload {
        selected: SelectedProvider::Smtp,
        from: "hello@forum.example".to_string(),
        smtp: Some(endpoint()),
        resend: None,
    })
    .await
    .unwrap();
    assert_eq!(svc.from().await, "hello@forum.example");
}
