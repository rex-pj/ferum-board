//! Tests for [`PluginLogSink`].
//!
//! The property under test is the one the sink exists for: a plugin logging
//! faster than the database can absorb must lose *log lines*, never the
//! connection pool. Before this existed, each `Ferum.log` call spawned its own
//! INSERT task, so a loop in a hook queued thousands of concurrent writers.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;

use ferum_application::shared::AppError;
use ferum_domain::models::plugin::{
    NewPlugin, NewPluginHook, NewPluginLog, NewPluginUiSlot, Plugin, PluginHook, PluginLog,
    PluginLogQuery, PluginStatus, PluginUiSlot,
};
use ferum_domain::repositories::plugin_repository::PluginRepository;
use ferum_infrastructure::plugins::log_sink::PluginLogSink;

/// Counts rows written and batches issued, and can be made slow so the buffer
/// is guaranteed to fill during a burst.
struct CountingRepo {
    rows: Arc<AtomicUsize>,
    batches: Arc<AtomicUsize>,
    write_delay: Duration,
}

#[async_trait]
impl PluginRepository for CountingRepo {
    async fn append_logs_batch(&self, entries: Vec<NewPluginLog>) -> Result<(), AppError> {
        self.batches.fetch_add(1, Ordering::SeqCst);
        self.rows.fetch_add(entries.len(), Ordering::SeqCst);
        if !self.write_delay.is_zero() {
            tokio::time::sleep(self.write_delay).await;
        }
        Ok(())
    }

    async fn append_log(&self, _: NewPluginLog) -> Result<(), AppError> {
        unreachable!("the sink must batch, never write single rows")
    }

    // ── Everything below is irrelevant to this test ──────────────────────────
    async fn list(&self) -> Result<Vec<Plugin>, AppError> { Ok(vec![]) }
    async fn find_by_id(&self, _: Uuid) -> Result<Option<Plugin>, AppError> { Ok(None) }
    async fn find_by_slug(&self, _: &str) -> Result<Option<Plugin>, AppError> { Ok(None) }
    async fn create(&self, _: NewPlugin) -> Result<Plugin, AppError> { unimplemented!() }
    async fn update_status(&self, _: Uuid, _: PluginStatus, _: Option<String>) -> Result<(), AppError> { unimplemented!() }
    async fn update_config(&self, _: Uuid, _: serde_json::Value) -> Result<(), AppError> { unimplemented!() }
    async fn update_activated_at(&self, _: Uuid) -> Result<(), AppError> { unimplemented!() }
    async fn update_circuit_open(&self, _: Uuid, _: bool) -> Result<(), AppError> { unimplemented!() }
    async fn delete(&self, _: Uuid) -> Result<(), AppError> { unimplemented!() }
    async fn hooks_for_plugin(&self, _: Uuid) -> Result<Vec<PluginHook>, AppError> { Ok(vec![]) }
    async fn active_hooks_for(&self, _: &str) -> Result<Vec<PluginHook>, AppError> { Ok(vec![]) }
    async fn create_hook(&self, _: NewPluginHook) -> Result<PluginHook, AppError> { unimplemented!() }
    async fn delete_hooks_for_plugin(&self, _: Uuid) -> Result<(), AppError> { unimplemented!() }
    async fn update_hook_avg_ms(&self, _: Uuid, _: i32) -> Result<(), AppError> { unimplemented!() }
    async fn active_ui_slots(&self) -> Result<Vec<PluginUiSlot>, AppError> { Ok(vec![]) }
    async fn ui_slots_for_plugin(&self, _: Uuid) -> Result<Vec<PluginUiSlot>, AppError> { Ok(vec![]) }
    async fn create_ui_slot(&self, _: NewPluginUiSlot) -> Result<PluginUiSlot, AppError> { unimplemented!() }
    async fn update_ui_slot(&self, _: Uuid, _: String, _: i32) -> Result<PluginUiSlot, AppError> { unimplemented!() }
    async fn delete_ui_slots_for_plugin(&self, _: Uuid) -> Result<(), AppError> { unimplemented!() }
    async fn get_logs(&self, _: Uuid, _: PluginLogQuery) -> Result<Vec<PluginLog>, AppError> { Ok(vec![]) }
    async fn delete_old_logs(&self, _: u32) -> Result<u64, AppError> { Ok(0) }
}

fn entry(message: &str) -> NewPluginLog {
    NewPluginLog {
        plugin_id: Uuid::nil(),
        level: "info".to_string(),
        hook_name: None,
        duration_ms: None,
        message: message.to_string(),
        context: None,
    }
}

#[tokio::test]
async fn buffered_entries_are_written_in_batches_not_one_row_at_a_time() {
    let rows = Arc::new(AtomicUsize::new(0));
    let batches = Arc::new(AtomicUsize::new(0));
    let sink = PluginLogSink::start(Arc::new(CountingRepo {
        rows: rows.clone(),
        batches: batches.clone(),
        write_delay: Duration::ZERO,
    }));

    for i in 0..50 {
        sink.try_log(entry(&format!("line {i}")));
    }
    // Long enough for at least one flush tick.
    tokio::time::sleep(Duration::from_millis(600)).await;

    assert_eq!(rows.load(Ordering::SeqCst), 50, "no entry may be lost below capacity");
    assert_eq!(sink.dropped_count(), 0, "50 entries are well inside the buffer");
    let batch_count = batches.load(Ordering::SeqCst);
    assert!(
        batch_count < 50,
        "50 entries must not cost 50 statements — got {batch_count}"
    );
}

#[tokio::test]
async fn a_runaway_producer_drops_entries_instead_of_growing_without_bound() {
    // The writer is deliberately slow, so the channel fills and stays full —
    // exactly the shape of a plugin looping over Ferum.log inside a hook.
    let rows = Arc::new(AtomicUsize::new(0));
    let batches = Arc::new(AtomicUsize::new(0));
    let sink = PluginLogSink::start(Arc::new(CountingRepo {
        rows: rows.clone(),
        batches: batches.clone(),
        write_delay: Duration::from_millis(50),
    }));

    for i in 0..20_000 {
        sink.try_log(entry(&format!("flood {i}")));
    }

    assert!(
        sink.dropped_count() > 0,
        "a 20k burst past a slow writer must be shed, not queued"
    );
    // The real guarantee: what was accepted is bounded by the buffer, so memory
    // and pending database work stay finite no matter how long the loop runs.
    let accepted = 20_000 - sink.dropped_count();
    assert!(
        accepted <= 1_024 + 100,
        "accepted {accepted} entries — buffer bound was not enforced"
    );
}

#[tokio::test]
async fn try_log_never_blocks_the_caller() {
    // Called from the plugin's JS thread, where blocking would stall the hook
    // and, through it, the user request that triggered it.
    let sink = PluginLogSink::start(Arc::new(CountingRepo {
        rows: Arc::new(AtomicUsize::new(0)),
        batches: Arc::new(AtomicUsize::new(0)),
        write_delay: Duration::from_millis(200),
    }));

    let start = std::time::Instant::now();
    for i in 0..5_000 {
        sink.try_log(entry(&format!("n {i}")));
    }
    assert!(
        start.elapsed() < Duration::from_millis(500),
        "5k sends took {:?} — try_log is not supposed to wait on the writer",
        start.elapsed()
    );
}
