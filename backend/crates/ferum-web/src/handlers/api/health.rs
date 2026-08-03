//! Liveness and readiness probes.
//!
//! Split because an orchestrator asks two different questions and a single
//! endpoint can only answer one of them well:
//!
//!   * *Should I restart this container?* — liveness. Answered by the process
//!     being able to reply at all. It must not depend on the database, or a
//!     database outage turns into every instance being killed and restarted
//!     into the same outage.
//!   * *Should I send it traffic?* — readiness. This one must check
//!     dependencies, so an instance that cannot reach Postgres is taken out of
//!     rotation instead of serving errors.
//!
//! The previous single `/health` answered `{"status":"ok"}` unconditionally,
//! which is the correct liveness answer and a wrong readiness one: it reported
//! healthy while the database was down and every request was failing.

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
        })),
    )
}
