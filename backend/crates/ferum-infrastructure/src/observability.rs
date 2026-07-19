use std::sync::atomic::{AtomicU64, Ordering};

/// Threshold above which repository queries emit a `slow_query` WARN.
/// Set once at startup from `Config::slow_query_ms` (SLOW_QUERY_MS env var);
/// the initial value here is only the fallback if startup never calls the setter.
static SLOW_QUERY_THRESHOLD_MS: AtomicU64 = AtomicU64::new(500);

pub fn set_slow_query_threshold_ms(ms: u64) {
    SLOW_QUERY_THRESHOLD_MS.store(ms, Ordering::Relaxed);
}

pub fn slow_query_threshold_ms() -> u128 {
    SLOW_QUERY_THRESHOLD_MS.load(Ordering::Relaxed) as u128
}
