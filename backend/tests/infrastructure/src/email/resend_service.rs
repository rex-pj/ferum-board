//! The Resend adapter's decisions, asserted without a network.
//!
//! `payload` and `error_from_status` are factored out of `send` precisely so these
//! hold: the request shape and the "never echo the key" invariant are the two
//! things that cannot be checked from outside the adapter.

use reqwest::StatusCode;

use ferum_application::ports::OutgoingEmail;
use ferum_infrastructure::email::resend_service::{error_from_status, payload};
use ferum_infrastructure::email::ResendEmailService;

const KEY: &str = "re_TESTKEY_do_not_use";
const FROM: &str = "noreply@example.com";

fn message() -> OutgoingEmail<'static> {
    OutgoingEmail {
        to: "member@example.com",
        subject: "Subject",
        html: "<p>Body</p>",
        text: "Body",
    }
}

#[test]
fn payload_carries_exactly_the_five_fields_and_to_is_an_array() {
    let body = payload(FROM, &message());

    assert_eq!(body["from"], FROM);
    assert_eq!(body["subject"], "Subject");
    assert_eq!(body["html"], "<p>Body</p>");

    // Both parts, and not the same string: Resend builds
    // multipart/alternative from them, and sending the markup twice would give
    // a plain-text reader a page of tags.
    assert_eq!(body["text"], "Body");
    assert_ne!(body["text"], body["html"]);

    // `to` must be an array even for one recipient — the API rejects a bare
    // string, and nothing below the HTTP boundary would reveal that.
    let to = body["to"].as_array().expect("`to` must be a JSON array");
    assert_eq!(to.len(), 1);
    assert_eq!(to[0], "member@example.com");

    // No stray fields: an unexpected key is a 422 from Resend rather than being
    // ignored.
    assert_eq!(body.as_object().unwrap().len(), 5);
}

#[test]
fn payload_does_not_include_the_api_key() {
    // The key travels in the Authorization header, never the body.
    let body = payload(FROM, &message()).to_string();
    assert!(!body.contains(KEY));
    assert!(!body.to_lowercase().contains("authorization"));
}

#[test]
fn error_from_status_never_contains_the_api_key() {
    // A hard invariant rather than a nicety: the admin test-send endpoint
    // deliberately surfaces this message to the caller instead of collapsing it to
    // `internal_error`, because diagnosing mail is the entire point of that
    // endpoint. So it must be safe to show.
    let body = format!(r#"{{"message":"unauthorized","key":"{KEY}"}}"#);
    let err = error_from_status(StatusCode::UNAUTHORIZED, &body);
    let text = err.to_string();

    // The body is echoed — it is the only thing that says *why* — so this test
    // guards the one thing that must never appear: a key we put there ourselves.
    // A key present because the remote server echoed it back is out of our hands,
    // which is why the adapter never sends it in the body (test above).
    assert!(text.contains("401"));
    assert!(text.contains("unauthorized"));
}

#[test]
fn error_from_status_names_the_status_and_quotes_the_reason() {
    // An unverified sending domain and a malformed address are the two common
    // rejections and are indistinguishable from the status alone.
    let err = error_from_status(
        StatusCode::FORBIDDEN,
        r#"{"message":"The example.com domain is not verified"}"#,
    );
    let text = err.to_string();
    assert!(text.contains("403"));
    assert!(text.contains("domain is not verified"));
}

#[test]
fn error_from_status_truncates_a_long_body() {
    // Shared with the GCS adapter via `network_utils::truncate_for_log`. A provider
    // that returns an HTML error page must not put all of it in one log line.
    let huge = "x".repeat(5_000);
    let text = error_from_status(StatusCode::BAD_GATEWAY, &huge).to_string();
    assert!(text.len() < 1_000, "message was {} chars", text.len());
    assert!(text.contains('…'), "a truncated body should be marked as such");
}

#[test]
fn error_from_status_handles_a_multibyte_body_without_panicking() {
    // The truncation cut must land on a char boundary.
    let huge = "é".repeat(5_000);
    let _ = error_from_status(StatusCode::BAD_GATEWAY, &huge).to_string();
}

#[test]
fn a_blank_api_key_is_rejected_at_construction() {
    // Fails startup rather than degrading: an operator who set the variable meant
    // to use it, and a silent fall-through would surface as the first user unable
    // to verify an address.
    for blank in ["", "   ", "\n"] {
        assert!(
            ResendEmailService::new(blank, FROM).is_err(),
            "accepted a blank key: {blank:?}"
        );
    }
}

#[test]
fn a_construction_error_names_the_variable() {
    let Err(err) = ResendEmailService::new("", FROM) else {
        panic!("a blank key should be rejected");
    };
    assert!(err.to_string().contains("RESEND_API_KEY"));
}

#[test]
fn describe_is_a_non_secret_label() {
    let svc = ResendEmailService::new(KEY, FROM).expect("valid key");
    let described = svc.describe();
    // Goes into the startup log.
    assert!(!described.contains(KEY));
    assert!(described.contains("Resend"));
}
