mod app_state;
mod config;
mod handlers;
mod middleware;
mod routings;
mod startup;
mod view_models;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ferum_board=info,ferum_domain=info,ferum_application=info,ferum_infrastructure=info,tower_http=info".parse().unwrap()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config =
        config::Config::from_env().map_err(|e| anyhow::anyhow!("Configuration error: {}", e))?;

    tracing::info!("Starting Ferum Board API at {}", config.app_url);

    let state = startup::build_app_state(&config).await?;
    startup::maybe_run_headless_setup(&config, &state).await?;
    let app = routings::build_router(state, &config.cors_origins);

    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Listening on http://{}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C handler");
    tracing::info!("Shutdown signal received, draining requests…");
}
