//! Liveness and readiness probes, split because they answer different questions.
//!
//! Liveness ("restart me?") must NOT touch the database, or an outage becomes
//! every instance being killed and restarted into the same outage. Readiness
//! ("send me traffic?") must, so an instance that cannot reach Postgres leaves
//! rotation instead of serving errors.

use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::app_state::AppState;

/// How long a readiness check waits on a dependency.
///
/// Shorter than the pool's 5s `acquire_timeout` on purpose. A saturated pool
/// makes `ping()` queue, and a probe that waits the full acquire timeout would
/// report "not ready" for an instance whose database is perfectly healthy and
/// merely busy. Failing fast here reports *slow*, and because readiness only
/// gates new traffic, shedding load from a saturated instance is the right
/// response anyway.
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

/// GET /health — liveness. Deliberately checks nothing.
///
/// Also the legacy path, kept so existing container and load-balancer configs
/// do not break; its meaning is unchanged from what the old handler actually
/// did.
pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// GET /health/live — liveness, explicitly named.
pub async fn live() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// GET /health/ready — readiness. 200 only when dependencies answer.
///
/// Reports each dependency separately so an operator reading the body can tell
/// a database outage from a Redis one without consulting logs. Redis is
/// optional infrastructure — its absence is a configuration, not a fault — so
/// only the database can make this fail.
pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let db_ok = matches!(
        tokio::time::timeout(PROBE_TIMEOUT, state.db.ping()).await,
        Ok(Ok(()))
    );

    // `exists` on a key nobody writes is a cheap round trip that still proves
    // the connection works. `CacheService` has no dedicated ping, and adding
    // one to the port for this alone would not earn its keep.
    let cache_ok = tokio::time::timeout(
        PROBE_TIMEOUT,
        state.cache.exists("health:probe"),
    )
    .await
    .is_ok();

    // Read from the cached background probe — never probed here. A readiness
    // endpoint that makes an outbound request per call is an amplifier: anyone
    // who can reach it can make this process hammer the object store.
    //
    // `"checking"` until the first probe lands, which is a real state and not a
    // failure. Reported but deliberately NOT part of `db_ok`: unreadable uploads
    // are a degraded site, not an unusable one. Text, login and moderation all
    // still work, and pulling the instance out of rotation over broken images
    // would convert a partial outage into a total one.
    let uploads = state
        .upload_read_status
        .read()
        .await
        .as_ref()
        .map(|probe| probe.label())
        .unwrap_or("checking");

    // An in-process read, never a test send: that costs money and burns the
    // sending domain's reputation, so a readiness endpoint doing one is a
    // billing amplifier. Use POST /api/admin/email/test to check delivery.
    //
    // Reported but NOT part of `db_ok` — a forum that cannot mail still serves
    // every page, and `disabled` may be deliberate.
    let mail = state.email.provider().await.label();

    let status = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        Json(json!({
            "status": if db_ok { "ready" } else { "unavailable" },
            "database": if db_ok { "ok" } else { "unreachable" },
            "cache": if cache_ok { "ok" } else { "unreachable" },
            // "same-origin" | "readable" | "forbidden" | "unknown" | "checking".
            // `forbidden` means every uploaded image on the site is a broken
            // link — worth alerting on, but see above for why it is not a 503.
            "uploads": uploads,
            // "resend" | "smtp" | "disabled". `disabled` also means new
            // registrations are being auto-verified.
            "mail": mail,
        })),
    )
}
