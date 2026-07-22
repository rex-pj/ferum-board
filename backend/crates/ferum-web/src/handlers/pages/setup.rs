use axum::extract::State;
use axum::response::{Html, IntoResponse};
use tera::Context;

use crate::app_state::AppState;

use super::PageError;

pub async fn setup(
    State(state): State<AppState>,
    axum::Extension(locale): axum::Extension<ferum_domain::Locale>,
) -> Result<impl IntoResponse, PageError> {
    // The first-run wizard honours the negotiated locale so an admin installing
    // in a non-English environment isn't forced through setup in English before
    // they can reach the language setting.
    let html = state
        .tera
        .render(&locale, "setup.html", &Context::new())
        .await?;
    Ok(Html(html))
}
