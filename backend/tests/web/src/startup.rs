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
//!
//! The same split now applies to `TimeZone`, with a sharper failure mode: a
//! dropped timeout costs a backstop, a dropped `TimeZone` changes what
//! `CURRENT_DATE` *means* and silently re-buckets every daily statistic.

use ferum_web::startup::with_session_settings;

#[test]
fn session_settings_are_appended_to_a_plain_url() {
    let out = with_session_settings("postgres://u:p@localhost:5432/db");
    assert!(out.starts_with("postgres://u:p@localhost:5432/db?options="), "got {out}");
    assert!(out.contains("statement_timeout%3D30000"), "got {out}");
    assert!(out.contains("idle_in_transaction_session_timeout%3D60000"), "got {out}");
}

#[test]
fn the_session_timezone_is_requested_as_utc() {
    // Asserts intent, not effect. `sqlx-postgres` already forces
    // `TimeZone=UTC` in its startup packet and that wins over `options`, so this
    // is currently redundant — but it is redundancy against a dependency's
    // undocumented internals, and it is what a non-sqlx client or a
    // transaction-mode pooler would need. See the note on SESSION_TIME_ZONE.
    let out = with_session_settings("postgres://localhost/db");
    assert!(
        out.contains("TimeZone%3DUTC"),
        "the connection URL must state TimeZone=UTC: {out}"
    );
}

#[test]
fn an_existing_query_string_is_extended_not_replaced() {
    let out = with_session_settings("postgres://localhost/db?sslmode=require");
    assert!(out.contains("sslmode=require"), "existing parameters must survive: {out}");
    assert!(out.contains("&options="), "the separator must be & once a ? is present: {out}");
}

#[test]
fn an_operator_supplied_options_parameter_wins() {
    // Someone who spelled `options=` out has a reason — a lock_timeout, a
    // different search_path, a deliberately higher ceiling for a slow report
    // replica. Appending a second `options=` would be silently ignored by
    // libpq-style parsing, so the safe behaviour is to leave theirs alone.
    //
    // This is deliberately still an all-or-nothing hand-off, including for
    // TimeZone. Merging our settings into someone else's `options=` string
    // would mean parsing and rewriting it, and getting that subtly wrong is a
    // worse failure than the documented one: the code WARNs and names the
    // setting they have to carry over.
    let url = "postgres://localhost/db?options=-c%20statement_timeout%3D5000";
    assert_eq!(with_session_settings(url), url);
}

#[test]
fn the_ceiling_is_far_above_any_legitimate_request() {
    // Guards against someone "tuning" this down to a value that would start
    // cancelling real work. It is a backstop against pathological queries, not
    // a latency budget — the app's own p95 target is 200ms.
    let out = with_session_settings("postgres://localhost/db");
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

#[test]
fn every_setting_is_space_separated_within_one_options_parameter() {
    // libpq takes a single `options=` value; the individual `-c` flags inside it
    // are separated by (URL-encoded) spaces. Getting this wrong does not error —
    // the server rejects or ignores the malformed tail, so the settings that
    // came after the mistake simply never apply.
    let out = with_session_settings("postgres://localhost/db");
    let options = out.split("options=").nth(1).expect("options= must be present");
    assert_eq!(
        options.matches("-c%20").count(),
        3,
        "expected exactly three -c settings (statement_timeout, \
         idle_in_transaction_session_timeout, TimeZone): {options}"
    );
    assert_eq!(out.matches("options=").count(), 1, "only one options= is legal: {out}");
}
