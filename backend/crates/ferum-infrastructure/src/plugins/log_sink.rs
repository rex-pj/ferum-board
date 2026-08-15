//! Buffered writer for `plugin_logs` — the one database write whose volume a
//! plugin author controls without limit.
//!
//! Bounded channel, one writer task, batched inserts, and **dropping on
//! overflow** (counted, not silent). A task-per-call once let a 10k-iteration
//! logging loop exhaust the pool and fail every unrelated request.
//!
//! Losing diagnostic output is preferable to stalling the site.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use ferum_domain::models::plugin::NewPluginLog;
use ferum_domain::repositories::plugin_repository::PluginRepository;

/// Entries buffered before new ones are dropped.
///
/// Measured rather than guessed: batched writes to `plugin_logs` sustain about
/// 24,000 rows/s (see `batched_log_writes_drain_fast_enough_for_the_sink_buffer`),
/// so a completely full buffer clears in roughly 42ms. That makes 1024 a
/// generous cushion for any ordinary burst — shedding only begins once a
/// producer outruns 24k lines/s, which no legitimate plugin does — while
/// costing on the order of 100 KB of memory at worst.
const CHANNEL_CAPACITY: usize = 1_024;

/// Largest single INSERT. Keeps one statement's parameter count well inside
/// Postgres' limit while still amortising the round trip.
const MAX_BATCH: usize = 100;

/// How long a partial batch waits for company before being written anyway, so
/// a quiet plugin's log line still appears promptly in the admin UI.
const FLUSH_INTERVAL: Duration = Duration::from_millis(200);

/// How often the dropped-entry counter is reported, when it is non-zero.
const DROP_REPORT_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct PluginLogSink {
    tx: mpsc::Sender<NewPluginLog>,
    dropped: Arc<AtomicU64>,
}

impl PluginLogSink {
    /// Starts the writer task on the current runtime and returns a handle to it.
    pub fn start(repo: Arc<dyn PluginRepository>) -> Self {
        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        tokio::spawn(run_writer(rx, repo, dropped.clone()));
        Self { tx, dropped }
    }

    /// Queues an entry, dropping it if the buffer is full.
    ///
    /// Never blocks and never fails to the caller: this is invoked from the
    /// plugin JS thread, where waiting would convert log pressure into paused
    /// plugin execution — and, through the hook it is running inside, into a
    /// slower user request.
    pub fn try_log(&self, entry: NewPluginLog) {
        if self.tx.try_send(entry).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Total entries discarded since startup. Exposed for tests and diagnostics.
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

async fn run_writer(
    mut rx: mpsc::Receiver<NewPluginLog>,
    repo: Arc<dyn PluginRepository>,
    dropped: Arc<AtomicU64>,
) {
    let mut buf: Vec<NewPluginLog> = Vec::with_capacity(MAX_BATCH);
    let mut flush = tokio::time::interval(FLUSH_INTERVAL);
    flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut drop_report = tokio::time::interval(DROP_REPORT_INTERVAL);
    drop_report.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_reported_drops = 0u64;

    loop {
        tokio::select! {
            received = rx.recv_many(&mut buf, MAX_BATCH) => {
                // 0 means every sender is gone — drain what is buffered and stop.
                if received == 0 {
                    write_batch(&repo, &mut buf).await;
                    return;
                }
                if buf.len() >= MAX_BATCH {
                    write_batch(&repo, &mut buf).await;
                }
            }
            _ = flush.tick() => {
                write_batch(&repo, &mut buf).await;
            }
            _ = drop_report.tick() => {
                let total = dropped.load(Ordering::Relaxed);
                if total > last_reported_drops {
                    tracing::warn!(
                        dropped_total = total,
                        dropped_since_last_report = total - last_reported_drops,
                        "plugin log entries discarded: a plugin is logging faster than \
                         they can be persisted"
                    );
                    last_reported_drops = total;
                }
            }
        }
    }
}

async fn write_batch(repo: &Arc<dyn PluginRepository>, buf: &mut Vec<NewPluginLog>) {
    if buf.is_empty() {
        return;
    }
    let batch = std::mem::take(buf);
    let count = batch.len();
    if let Err(e) = repo.append_logs_batch(batch).await {
        // Swallowed on purpose: this task is the only writer, and returning
        // early would end it and silently stop all plugin logging for the
        // process lifetime. A failed batch is lost; the next one is attempted.
        tracing::warn!(error = %e, count, "failed to persist plugin log batch");
    }
}
