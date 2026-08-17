//! Orphan sweep over the configured blob store.
//!
//! Reference counting only holds while every write path plays along, and no
//! write-path fix reaches what already leaked. This is how an operator finds it.

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::{Extension, Json};

use ferum_application::usecases::storage_audit_usecase::DEFAULT_SWEEP_LIMIT;
use ferum_domain::AuthUser;

use crate::app_state::AppState;
use crate::middleware::AuthUserExt;
use crate::view_models::{DataResponse, HandlerResult};

#[derive(serde::Deserialize)]
pub struct SweepQuery {
    /// Resume point from a previous response's `next_after`. Absent starts at
    /// the beginning of the store.
    pub after: Option<String>,
    pub limit: Option<usize>,
}

/// GET /api/admin/storage/sweep — report objects nothing references.
///
/// Never deletes. Paged rather than exhaustive: this runs inside a request, and
/// a bucket holding a hundred thousand objects must not become a ten-minute
/// response. Feed `next_after` back in until it comes back null.
pub async fn report_storage_sweep(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<SweepQuery>,
) -> HandlerResult<impl IntoResponse> {
    run(state, auth_user, q, false).await
}

/// POST /api/admin/storage/sweep — delete what the report found.
///
/// Separate from the GET rather than a query flag on it, and the reason is
/// security, not taste: `middleware/csrf.rs` only inspects non-GET requests,
/// and `SameSite=Lax` attaches the auth cookie to top-level navigations — so a
/// destructive GET is one clicked link away from running. This form sits behind
/// the Origin allowlist.
///
/// Rescans rather than accepting a list of keys from the client. Passing keys
/// in would be faster and would also be an arbitrary-delete endpoint; rescanning
/// means only what the server itself just found unreferenced can go.
pub async fn apply_storage_sweep(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
    Query(q): Query<SweepQuery>,
) -> HandlerResult<impl IntoResponse> {
    run(state, auth_user, q, true).await
}

/// GET /api/admin/storage/audit — rows nothing points at any more.
///
/// The other direction from the sweep, and the half it cannot see: a row at
/// `ref_count = 1` that no column references looks alive to every count-based
/// check. Pure SQL, so it needs no store enumeration and works on backends that
/// cannot list themselves.
pub async fn report_storage_audit(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let report = state.storage_audit.audit(actor, false).await?;
    Ok(Json(DataResponse::new(report)))
}

/// POST /api/admin/storage/audit — release what the audit found.
///
/// Same GET/POST split as the sweep, for the same CSRF reason. Re-runs the audit
/// server-side rather than accepting a key list, so only what the server itself
/// just found unreferenced can be released.
pub async fn apply_storage_audit(
    State(state): State<AppState>,
    Extension(auth_user): Extension<Option<AuthUser>>,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let report = state.storage_audit.audit(actor, true).await?;
    Ok(Json(DataResponse::new(report)))
}

async fn run(
    state: AppState,
    auth_user: Option<AuthUser>,
    q: SweepQuery,
    apply: bool,
) -> HandlerResult<impl IntoResponse> {
    let actor = auth_user.require_auth()?;
    let report = state
        .storage_audit
        .sweep(
            actor,
            q.after.as_deref(),
            q.limit.unwrap_or(DEFAULT_SWEEP_LIMIT),
            apply,
        )
        .await?;
    Ok(Json(DataResponse::new(report)))
}
