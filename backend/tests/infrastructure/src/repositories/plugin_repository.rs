//! Integration tests for [`PgPluginRepository`].
//!
//! Covers plugin CRUD, hook management, UI slot management, and log operations.

use ferum_domain::models::plugin::{
    NewPlugin, NewPluginHook, NewPluginLog, NewPluginUiSlot, PluginStatus, PluginTier,
    PluginLogQuery,
};
use ferum_domain::repositories::plugin_repository::PluginRepository;
use ferum_infrastructure::repositories::PgPluginRepository;

use crate::common::TestDb;

fn new_plugin(slug: &str) -> NewPlugin {
    NewPlugin {
        slug: slug.to_string(),
        name: format!("{slug} plugin"),
        version: "1.0.0".to_string(),
        tier: PluginTier::Manifest,
        manifest: serde_json::json!({"name": slug, "version": "1.0.0", "tier": "manifest"}),
        granted_capabilities: serde_json::json!({}),
        install_path: format!("/plugins/{slug}"),
        installed_by: None,
    }
}

// ─── Plugin CRUD ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_on_empty_returns_empty() {
    let db = TestDb::new("plg_list_empty").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    assert!(repo.list().await.expect("list").is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn create_and_find_by_slug() {
    let db = TestDb::new("plg_create_find_slug").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("discord-notifier")).await.expect("create");

    let found = repo.find_by_slug("discord-notifier").await.expect("find_by_slug").unwrap();
    assert_eq!(found.id, plugin.id);
    assert_eq!(found.tier, PluginTier::Manifest);
    assert_eq!(found.status, PluginStatus::Installing);
    db.teardown().await;
}

#[tokio::test]
async fn find_by_id_returns_plugin() {
    let db = TestDb::new("plg_find_by_id").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("test-plugin")).await.expect("create");
    let found = repo.find_by_id(plugin.id).await.expect("find_by_id").unwrap();
    assert_eq!(found.id, plugin.id);
    db.teardown().await;
}

#[tokio::test]
async fn update_status_to_active() {
    let db = TestDb::new("plg_update_status").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("my-plugin")).await.expect("create");

    repo.update_status(plugin.id, PluginStatus::Active, None).await.expect("update_status");
    let found = repo.find_by_id(plugin.id).await.expect("find_by_id").unwrap();
    assert_eq!(found.status, PluginStatus::Active);
    assert!(found.error_message.is_none());
    db.teardown().await;
}

#[tokio::test]
async fn update_status_to_error_stores_message() {
    let db = TestDb::new("plg_update_status_error").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("bad-plugin")).await.expect("create");

    repo.update_status(plugin.id, PluginStatus::Error, Some("init failed".to_string()))
        .await.expect("update_status");
    let found = repo.find_by_id(plugin.id).await.expect("find_by_id").unwrap();
    assert_eq!(found.status, PluginStatus::Error);
    assert_eq!(found.error_message.as_deref(), Some("init failed"));
    db.teardown().await;
}

#[tokio::test]
async fn update_config_stores_json() {
    let db = TestDb::new("plg_update_config").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("configurable")).await.expect("create");

    let config = serde_json::json!({"webhook_url": "https://discord.com/api/webhooks/123"});
    repo.update_config(plugin.id, config.clone()).await.expect("update_config");

    let found = repo.find_by_id(plugin.id).await.expect("find_by_id").unwrap();
    assert_eq!(found.config["webhook_url"], "https://discord.com/api/webhooks/123");
    db.teardown().await;
}

#[tokio::test]
async fn update_circuit_open_flag() {
    let db = TestDb::new("plg_circuit_open").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("circuit-test")).await.expect("create");

    repo.update_circuit_open(plugin.id, true).await.expect("open circuit");
    let found = repo.find_by_id(plugin.id).await.expect("find_by_id").unwrap();
    assert!(found.circuit_open);

    repo.update_circuit_open(plugin.id, false).await.expect("close circuit");
    let found = repo.find_by_id(plugin.id).await.expect("find_by_id").unwrap();
    assert!(!found.circuit_open);
    db.teardown().await;
}

#[tokio::test]
async fn delete_removes_plugin() {
    let db = TestDb::new("plg_delete").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("temp")).await.expect("create");
    repo.delete(plugin.id).await.expect("delete");
    assert!(repo.find_by_id(plugin.id).await.expect("find").is_none());
    db.teardown().await;
}

// ─── Hook management ──────────────────────────────────────────────────────────

#[tokio::test]
async fn create_hook_and_list_for_plugin() {
    let db = TestDb::new("plg_hooks_list").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("hook-plugin")).await.expect("create");

    repo.create_hook(NewPluginHook {
        plugin_id: plugin.id,
        hook_name: "before_post_create".to_string(),
        priority: 100,
    }).await.expect("create_hook");

    let hooks = repo.hooks_for_plugin(plugin.id).await.expect("hooks_for_plugin");
    assert_eq!(hooks.len(), 1);
    assert_eq!(hooks[0].hook_name, "before_post_create");
    db.teardown().await;
}

#[tokio::test]
async fn delete_hooks_for_plugin_removes_all() {
    let db = TestDb::new("plg_delete_hooks").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("multi-hook")).await.expect("create");
    repo.create_hook(NewPluginHook { plugin_id: plugin.id, hook_name: "h1".to_string(), priority: 10 }).await.expect("h1");
    repo.create_hook(NewPluginHook { plugin_id: plugin.id, hook_name: "h2".to_string(), priority: 20 }).await.expect("h2");

    repo.delete_hooks_for_plugin(plugin.id).await.expect("delete_hooks");
    assert!(repo.hooks_for_plugin(plugin.id).await.expect("list").is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn update_hook_avg_ms_stores_value() {
    let db = TestDb::new("plg_hook_avg_ms").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("timing-plugin")).await.expect("create");
    let hook = repo.create_hook(NewPluginHook { plugin_id: plugin.id, hook_name: "before_post_create".to_string(), priority: 100 }).await.expect("create_hook");

    repo.update_hook_avg_ms(hook.id, 42).await.expect("update_avg_ms");
    let hooks = repo.hooks_for_plugin(plugin.id).await.expect("list hooks");
    assert_eq!(hooks[0].avg_ms, Some(42));
    db.teardown().await;
}

// ─── UI Slot management ───────────────────────────────────────────────────────

#[tokio::test]
async fn create_ui_slot_and_active_ui_slots() {
    let db = TestDb::new("plg_ui_slots").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("banner-plugin")).await.expect("create");
    repo.update_status(plugin.id, PluginStatus::Active, None).await.expect("activate");

    repo.create_ui_slot(NewPluginUiSlot {
        plugin_id: plugin.id,
        slot_name: "header".to_string(),
        asset_url: "/plugins/banner/bundle.js".to_string(),
        custom_element_tag: "banner-widget".to_string(),
        props: vec!["message".to_string()],
        load_order: 0,
    }).await.expect("create_ui_slot");

    let slots = repo.active_ui_slots().await.expect("active_ui_slots");
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].slot_name, "header");
    db.teardown().await;
}

#[tokio::test]
async fn delete_ui_slots_for_plugin_removes_all() {
    let db = TestDb::new("plg_delete_ui_slots").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("ui-plugin")).await.expect("create");
    repo.create_ui_slot(NewPluginUiSlot {
        plugin_id: plugin.id,
        slot_name: "footer".to_string(),
        asset_url: "/plugins/ui/bundle.js".to_string(),
        custom_element_tag: "footer-widget".to_string(),
        props: vec![],
        load_order: 0,
    }).await.expect("create_ui_slot");

    repo.delete_ui_slots_for_plugin(plugin.id).await.expect("delete_ui_slots");
    let slots = repo.active_ui_slots().await.expect("active_ui_slots");
    assert!(slots.is_empty());
    db.teardown().await;
}

// ─── Log management ───────────────────────────────────────────────────────────

#[tokio::test]
async fn append_log_and_get_logs() {
    let db = TestDb::new("plg_logs").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("logging-plugin")).await.expect("create");

    repo.append_log(NewPluginLog {
        plugin_id: plugin.id,
        level: "info".to_string(),
        hook_name: Some("before_post_create".to_string()),
        duration_ms: Some(15),
        message: "hook executed".to_string(),
        context: None,
    }).await.expect("append_log");

    let logs = repo.get_logs(plugin.id, PluginLogQuery { limit: 20, ..Default::default() })
        .await.expect("get_logs");
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].level, "info");
    assert_eq!(logs[0].message, "hook executed");
    db.teardown().await;
}

#[tokio::test]
async fn get_logs_filters_by_level() {
    let db = TestDb::new("plg_logs_filter_level").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("log-filter-test")).await.expect("create");

    repo.append_log(NewPluginLog { plugin_id: plugin.id, level: "info".to_string(), hook_name: None, duration_ms: None, message: "info msg".to_string(), context: None }).await.expect("info");
    repo.append_log(NewPluginLog { plugin_id: plugin.id, level: "error".to_string(), hook_name: None, duration_ms: None, message: "error msg".to_string(), context: None }).await.expect("error");

    let logs = repo.get_logs(plugin.id, PluginLogQuery { level: Some("error".to_string()), limit: 20, ..Default::default() })
        .await.expect("get_logs");
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].level, "error");
    db.teardown().await;
}

#[tokio::test]
async fn delete_old_logs_returns_count() {
    // With retention_days=0, all logs older than today would be deleted.
    // In practice, just-inserted logs are not old enough, so count=0.
    let db = TestDb::new("plg_delete_old_logs").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("log-gc")).await.expect("create");
    repo.append_log(NewPluginLog { plugin_id: plugin.id, level: "info".to_string(), hook_name: None, duration_ms: None, message: "msg".to_string(), context: None }).await.expect("append");

    // retention_days=30 should not delete recent logs
    let deleted = repo.delete_old_logs(30).await.expect("delete_old_logs");
    assert_eq!(deleted, 0, "recent logs should not be deleted");
    db.teardown().await;
}

/// Measures how fast the batched log path actually drains, which is what
/// decides whether `PluginLogSink`'s 1024-entry buffer is the right size.
///
/// The buffer only has to cover the gap between a burst arriving and the writer
/// clearing it. Sizing it by intuition is guesswork in either direction: too
/// small and ordinary logging is shed, too large and a runaway plugin parks
/// megabytes of strings in memory before backpressure engages. Reported rather
/// than asserted tightly, because absolute throughput is hardware-dependent —
/// the assertion is only that it is fast enough for 1024 to be a short backlog.
#[tokio::test]
async fn batched_log_writes_drain_fast_enough_for_the_sink_buffer() {
    let db = TestDb::new("plg_log_throughput").await;
    let repo = PgPluginRepository::new(db.conn.clone());
    let plugin = repo.create(new_plugin("throughput-probe")).await.expect("create plugin");

    const BATCH: usize = 100;
    const BATCHES: usize = 20;

    let started = std::time::Instant::now();
    for _ in 0..BATCHES {
        let entries: Vec<NewPluginLog> = (0..BATCH)
            .map(|i| NewPluginLog {
                plugin_id: plugin.id,
                level: "info".to_string(),
                hook_name: None,
                duration_ms: None,
                message: format!("throughput probe line {i}"),
                context: None,
            })
            .collect();
        repo.append_logs_batch(entries).await.expect("batch insert");
    }
    let elapsed = started.elapsed();

    let rows = (BATCH * BATCHES) as f64;
    let per_sec = rows / elapsed.as_secs_f64();
    let buffer_drain_ms = (1024.0 / per_sec) * 1000.0;
    println!(
        "  plugin_logs batched write: {rows:.0} rows in {elapsed:?} = {per_sec:.0} rows/s; \
         a full 1024-entry buffer drains in ~{buffer_drain_ms:.0}ms"
    );

    assert!(
        per_sec > 1_000.0,
        "batched writes managed only {per_sec:.0} rows/s — at that rate the sink's \
         1024-entry buffer would take over a second to drain and normal logging would shed"
    );

    db.teardown().await;
}
