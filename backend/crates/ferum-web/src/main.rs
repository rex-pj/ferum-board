use ferum_web::config;
use ferum_web::routings;
use ferum_web::startup;

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

    let addr = format!("{}:{}", config.bind_addr, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    print_ready_banner(config.port);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

/// The one line a developer reads off their terminal after `cargo run`.
///
/// Deliberately not `tracing`: the JSON formatter would strip the colour and
/// bury it among the startup records. Written through a locked handle rather
/// than `println!` so the one thing that can go wrong — a closed or full stdout,
/// which `println!` would panic on — is visibly ignored instead. A banner is
/// never worth failing a boot over.
fn print_ready_banner(port: u16) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = writeln!(
        out,
        "\n  {}  {}\n",
        "▶  Server ready:".bold(),
        format!("http://localhost:{port}")
            .bold()
            .bright_cyan()
            .underline()
    );
    let _ = out.flush();
}

/// Resolves when the process is asked to stop, so `axum::serve` can drain
/// in-flight requests (NF-OP-03).
///
/// **SIGTERM is the one that matters**: `docker stop`, Compose restarts and pod
/// evictions all send it, and nothing in a container ever sends SIGINT. Watching
/// Ctrl-C alone cut in-flight requests on every deploy.
///
/// **A handler that cannot be installed disables that branch; it does not stop
/// the server.** This future is polled by `with_graceful_shutdown`, so it runs
/// inside the serving task rather than before it — a panic here would take down
/// an instance that is already answering requests. If both branches fail,
/// nothing resolves, graceful drain never fires, and `docker stop` escalates to
/// SIGKILL after its grace period. Degraded shutdown beats no server.
async fn shutdown_signal() {
    let ctrl_c = async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => "SIGINT",
            Err(e) => {
                tracing::error!(error = %e, "cannot listen for Ctrl-C; that branch is disabled");
                std::future::pending().await
            }
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
                "SIGTERM"
            }
            Err(e) => {
                tracing::error!(error = %e, "cannot install SIGTERM handler; that branch is disabled");
                std::future::pending().await
            }
        }
    };
    // Windows has no SIGTERM. `pending()` never resolves, so the `select!` below
    // reduces to waiting on Ctrl-C alone — the previous behaviour, which is the
    // correct one on a platform that cannot deliver the other signal.
    #[cfg(not(unix))]
    let terminate = std::future::pending::<&str>();

    let signal = tokio::select! {
        s = ctrl_c => s,
        s = terminate => s,
    };

    tracing::info!(signal, "Shutdown signal received, draining requests…");
}
