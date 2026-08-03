use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};

use crate::app_state::AppState;

/// Redirect to /setup on first run, redirect away from /setup once done.
/// Skips all /api/**, /static/**, /themes/**, /health, /files/** paths.
#[tracing::instrument(skip_all, fields(path = %req.uri().path()))]
pub async fn setup_guard(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_owned();
    tracing::debug!("setup_guard_enter");

    let is_asset = path.starts_with("/api/")
        || path.starts_with("/static/")
        || path.starts_with("/themes/")
        || path.starts_with("/files/")   // files are DB blobs — not page routes
        // Probes, including /health/live and /health/ready. An orchestrator
        // must get a real answer during first-run setup too — redirecting a
        // liveness probe to /setup would read as unhealthy and restart-loop
        // the container before an admin can finish the wizard.
        || path == "/health"
        || path.starts_with("/health/");

    if is_asset {
        tracing::debug!("setup_guard_asset_bypass");
        return next.run(req).await;
    }

    // Once setup is complete it can never revert within this process — skip the DB
    // round-trip on the hot path (every guest page view) after the first observation.
    use std::sync::atomic::Ordering;
    if state.setup_complete.load(Ordering::Relaxed) {
        if path == "/setup" {
            return Redirect::to("/").into_response();
        }
        return next.run(req).await;
    }

    tracing::debug!("setup_guard_checking_needs_setup");
    // IMPORTANT: only latch on a definitive `Ok(false)`. A transient DB error before
    // setup completes must NOT flip the latch permanently — otherwise a momentary
    // outage during first-run would disable the /setup redirect for the whole process
    // lifetime. On error we preserve the prior fail-through behavior and re-check next
    // request (no latch).
    let needs_setup = match state.setup.needs_setup().await {
        Ok(false) => {
            state.setup_complete.store(true, Ordering::Relaxed);
            false
        }
        Ok(true) => true,
        Err(e) => {
            tracing::warn!("needs_setup check failed, not latching: {e:?}");
            false
        }
    };
    tracing::debug!(needs_setup, "setup_guard_needs_setup_done");

    if needs_setup && path != "/setup" {
        return Redirect::to("/setup").into_response();
    }

    if !needs_setup && path == "/setup" {
        return Redirect::to("/").into_response();
    }

    next.run(req).await
}
