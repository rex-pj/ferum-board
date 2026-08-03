//! Tests for the connection-URL construction in `startup`.
//!
//! These pin one half of the application-wide query ceiling. The other half —
//! that a driver actually forwards `options=` to the server — is verified
//! against a real Postgres in
//! `ferum-infrastructure-tests::repositories::plugin_db_repository::
//! statement_timeout_can_be_set_through_the_connection_url`.
//!
//! Split that way because neither half is convincing alone: a correctly built
//! URL that the driver silently drops leaves every query unbounded, and a
//! driver that forwards a parameter nothing sets protects nothing. Both
//! failures are invisible at runtime — nothing errors, queries merely stop
//! being capped — which is why they are pinned rather than assumed.

use ferum_web::startup::with_server_timeouts;

#[test]
fn timeouts_are_appended_to_a_plain_url() {
    let out = with_server_timeouts("postgres://u:p@localhost:5432/db");
    assert!(out.starts_with("postgres://u:p@localhost:5432/db?options="), "got {out}");
    assert!(out.contains("statement_timeout%3D30000"), "got {out}");
    assert!(out.contains("idle_in_transaction_session_timeout%3D60000"), "got {out}");
}

#[test]
fn an_existing_query_string_is_extended_not_replaced() {
    let out = with_server_timeouts("postgres://localhost/db?sslmode=require");
    assert!(out.contains("sslmode=require"), "existing parameters must survive: {out}");
    assert!(out.contains("&options="), "the separator must be & once a ? is present: {out}");
}

#[test]
fn an_operator_supplied_options_parameter_wins() {
    // Someone who spelled `options=` out has a reason — a lock_timeout, a
    // different search_path, a deliberately higher ceiling for a slow report
    // replica. Appending a second `options=` would be silently ignored by
    // libpq-style parsing, so the safe behaviour is to leave theirs alone.
    let url = "postgres://localhost/db?options=-c%20statement_timeout%3D5000";
    assert_eq!(with_server_timeouts(url), url);
}

#[test]
fn the_ceiling_is_far_above_any_legitimate_request() {
    // Guards against someone "tuning" this down to a value that would start
    // cancelling real work. It is a backstop against pathological queries, not
    // a latency budget — the app's own p95 target is 200ms.
    let out = with_server_timeouts("postgres://localhost/db");
    let ms: u32 = out
        .split("statement_timeout%3D")
        .nth(1)
        .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|s| s.parse().ok())
        .expect("statement_timeout must be present and numeric");
    assert!(
        (10_000..=120_000).contains(&ms),
        "statement_timeout of {ms}ms is outside the sane backstop range"
    );
}
