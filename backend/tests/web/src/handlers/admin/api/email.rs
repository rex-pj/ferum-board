//! What the two mail-test endpoints are allowed to say back.
//!
//! Both deliberately surface the provider's own words — that is why they exist
//! — which puts them outside the path where `status_and_code` collapses an
//! `Internal` into `internal_error`. So the thing worth pinning is what gets
//! stripped on the way out.

use ferum_application::shared::AppError;
use ferum_web::handlers::admin::api::email::{delivery_error, MAIL_NOT_CONFIGURED};

/// `AppError::internal` is `#[track_caller]` and bakes `[file:line]` into the
/// message. Useful in a log, and exactly the kind of thing NF-SC-11 says must
/// not reach a response — an admin was being shown
/// `[crates\ferum-infrastructure\src\email\reloadable_service.rs:295]`.
#[test]
fn a_source_location_never_reaches_the_admin() {
    let e = AppError::internal("connection refused");
    let raw = e.to_string();
    assert!(
        raw.contains(".rs:"),
        "this test is pointless if `internal` stopped recording a location: {raw}"
    );

    let shown = delivery_error(&e);
    assert_eq!(shown, "connection refused");
    assert!(!shown.contains(".rs:"));
    assert!(!shown.contains("crates"));
}

/// The provider's message is the payload, so it must survive intact — including
/// brackets that are part of what the provider said.
#[test]
fn the_providers_own_words_survive() {
    for message in [
        "Resend rejected the send (403 Forbidden): domain not verified",
        "email send error: Connection refused (os error 111)",
    ] {
        let shown = delivery_error(&AppError::internal(message));
        assert_eq!(shown, message, "the diagnosis must not be truncated");
    }
}

/// Only a trailing `[...]` is stripped, and only the last one.
#[test]
fn a_bracket_that_is_not_a_location_is_left_alone() {
    let e = AppError::UnprocessableEntity("field [subject] is required".to_string());
    assert!(delivery_error(&e).ends_with("is required"));
}

/// "No provider configured" is a state we can name, not a fault to diagnose.
#[test]
fn the_not_configured_message_says_where_to_go() {
    assert!(MAIL_NOT_CONFIGURED.contains("Settings"));
    assert!(
        !MAIL_NOT_CONFIGURED.contains("mail_not_configured"),
        "an error code is not an explanation"
    );
}
