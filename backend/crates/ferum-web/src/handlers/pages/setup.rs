use axum::extract::State;
use axum::response::{Html, IntoResponse};
use tera::Context;

use crate::app_state::AppState;

use super::PageError;

pub async fn setup(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, PageError> {
    let html = state.tera.render("setup.html", &Context::new()).await?;
    Ok(Html(html))
}
