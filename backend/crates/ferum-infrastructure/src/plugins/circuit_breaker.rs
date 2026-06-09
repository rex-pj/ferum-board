use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Per-plugin circuit breaker.
///
/// If `failure_count` reaches `threshold` within `window`, the circuit opens.
/// The circuit resets automatically after `reset_secs` (half-open probe on next call).
pub struct CircuitBreaker {
    failure_count: AtomicU32,
    last_failure_at: Mutex<Option<Instant>>,
    circuit_opened_at: Mutex<Option<Instant>>,
    threshold: u32,
    window: Duration,
    reset: Duration,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, window_secs: u64, reset_secs: u64) -> Self {
        Self {
            failure_count: AtomicU32::new(0),
            last_failure_at: Mutex::new(None),
            circuit_opened_at: Mutex::new(None),
            threshold,
            window: Duration::from_secs(window_secs),
            reset: Duration::from_secs(reset_secs),
        }
    }

    /// Returns true if the circuit is open (plugin should be skipped).
    pub fn is_open(&self) -> bool {
        let opened_at = self.circuit_opened_at.lock().unwrap();
        if let Some(t) = *opened_at {
            // Auto-reset after reset_secs — next call becomes a probe
            if t.elapsed() < self.reset {
                return true;
            }
        }
        false
    }

    /// Record a successful hook call.
    pub fn record_success(&self) {
        self.failure_count.store(0, Ordering::SeqCst);
        *self.last_failure_at.lock().unwrap() = None;
        *self.circuit_opened_at.lock().unwrap() = None;
    }

    /// Record a failed hook call. Returns true if the circuit just opened.
    pub fn record_failure(&self) -> bool {
        let now = Instant::now();
        let mut last = self.last_failure_at.lock().unwrap();

        // Reset counter if failures are outside the window
        if last.map_or(true, |t: Instant| t.elapsed() > self.window) {
            self.failure_count.store(0, Ordering::SeqCst);
        }

        *last = Some(now);
        drop(last);

        let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;

        if count >= self.threshold {
            let mut opened = self.circuit_opened_at.lock().unwrap();
            if opened.is_none() {
                *opened = Some(now);
                tracing::warn!(
                    threshold = self.threshold,
                    "Plugin circuit opened after {} failures",
                    count
                );
                return true;
            }
        }

        false
    }
}
