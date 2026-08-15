//! SRP: all cross-cutting observability helpers live here.
//! Business logic never imports `std::time::Instant` or formats log strings directly.
//!
//! DIP: every helper depends on the `tracing` abstraction.
//! The concrete subscriber (stdout / JSON / OTLP) is wired once in `main::init_tracing` —
//! no code change needed here when the destination changes.

/// RAII guard logging elapsed time at `debug!` when dropped.
///
/// # Example
/// ```rust
/// use ferum_web::telemetry::Latency;
/// let _lat = Latency::start("site_ctx");
/// ```
pub struct Latency {
    op: &'static str,
    start: std::time::Instant,
}

impl Latency {
    pub fn start(op: &'static str) -> Self {
        Self {
            op,
            start: std::time::Instant::now(),
        }
    }
}

impl Drop for Latency {
    fn drop(&mut self) {
        tracing::debug!(
            op = self.op,
            elapsed_us = self.start.elapsed().as_micros() as u64,
            "latency"
        );
    }
}

/// Emits a structured debug event for a cache lookup result.
/// Callers communicate the source ("rwlock" / "redis" / "db") so the log is unambiguous.
///
/// SRP: cache observability is expressed once here, not duplicated at every call site.
pub fn record_cache_result(op: &'static str, source: &'static str, hit: bool) {
    tracing::debug!(op, source, hit, "cache_lookup");
}
