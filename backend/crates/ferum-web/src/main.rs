mod app_state;
mod config;
mod utils;
mod handlers;
mod middleware;
mod routings;
mod startup;
mod telemetry;
mod tera_engine;
mod view_models;

use colored::Colorize;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

// Use jemalloc on non-MSVC targets (production Linux/macOS). On Windows/MSVC the
// dependency is absent and the system allocator is used instead.
#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// Returns a _guard that must be kept alive for the duration of the process.
// Dropping the guard flushes and closes the file appender.
fn init_tracing(
    log_level: &str,
    log_format: &str,
    log_dir: Option<&str>,
) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| log_level.parse().unwrap_or_else(|_| EnvFilter::new("info")));

    let is_json = log_format == "json";

    // A stdout layer is ALWAYS installed so logs are visible in the terminal during
    // `cargo run`. It writes synchronously, so events are flushed on Ctrl-C — unlike
    // the non-blocking file appender below, whose buffer can be lost on a hard kill.
    let stdout_layer = if is_json {
        tracing_subscriber::fmt::layer().json().boxed()
    } else {
        tracing_subscriber::fmt::layer().boxed()
    };

    // When LOG_DIR is set, ALSO write to a daily-rotating file for durable history.
    let (file_layer, guard) = match log_dir {
        Some(dir) => {
            let appender = tracing_appender::rolling::daily(dir, "ferum-board.log");
            let (non_blocking, guard) = tracing_appender::non_blocking(appender);
            let layer = if is_json {
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_writer(non_blocking)
                    .boxed()
            } else {
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(non_blocking)
                    .boxed()
            };
            (Some(layer), Some(guard))
        }
        None => (None, None),
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(stdout_layer)
        .with(file_layer)
        .init();

    guard
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config =
        config::Config::from_env().map_err(|e| anyhow::anyhow!("Configuration error: {}", e))?;

    let _log_guard = init_tracing(
        &config.log_level,
        &config.log_format,
        config.log_dir.as_deref(),
    );

    // Log panics through tracing so they appear in the log file.
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown".to_string());
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("unknown panic payload");
        tracing::error!(panic.message = msg, panic.location = %location, "PANIC");
    }));

    tracing::info!("Starting Ferum Board API at {}", config.app_url);

    let state = startup::build_app_state(&config).await?;
    startup::maybe_run_headless_setup(&config, &state).await?;

    let app = routings::build_router(
        state,
        &config.cors_origins,
        &config.app_url,
        config.max_upload_size_mb,
    );

    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr).await?;

    println!(
        "\n  {}  {}\n",
        "▶  Server ready:".bold(),
        "http://localhost:8080".bold().bright_cyan().underline()
    );

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
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
