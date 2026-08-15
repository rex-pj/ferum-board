use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use ferum_application::ports::NullPluginRuntime;
use ferum_application::usecases::plugin_usecase::PluginUseCase;
use ferum_domain::models::plugin::{Plugin, PluginHook, PluginStatus, PluginTier};
use ferum_test_support::fixtures::AuthUserBuilder;
use ferum_test_support::mocks::{
    job_queue::NoopJobQueue, plugin_db_repository::MockPluginDbGateway,
    plugin_repository::MockPluginRepository, storage_service::NoopStorageService,
    stored_file_repository::NoopStoredFileRepository,
    webhook_repository::MockWebhookRepository,
};

// A manifest requesting three hooks, but the admin only granted one at install time
// (mirrors the capability-review flow: granted_capabilities is a subset of what the
// manifest's [capabilities] table requested).
fn manifest_requesting_three_hooks() -> serde_json::Value {
    serde_json::json!({
        "capabilities": {
            "hooks": [
                { "name": "before_post_create", "priority": 10 },
                { "name": "before_post_edit", "priority": 10 },
                { "name": "before_thread_create", "priority": 10 },
            ]
        }
    })
}

fn granted_only_before_post_create() -> serde_json::Value {
    serde_json::json!({
        "hooks": [
            { "name": "before_post_create", "priority": 10 },
        ]
    })
}

fn make_plugin(id: Uuid, manifest: serde_json::Value, granted_capabilities: serde_json::Value) -> Plugin {
    Plugin {
        id,
        slug: "com.example.test-plugin".to_string(),
        name: "Test Plugin".to_string(),
        version: "1.0.0".to_string(),
        tier: PluginTier::Script,
        status: PluginStatus::Inactive,
        manifest,
        config: serde_json::json!({}),
        granted_capabilities,
        install_path: "/tmp/plugins/com.example.test-plugin".to_string(),
        db_schema_name: None,
        db_schema_version: 0,
        installed_by: None,
        installed_at: Utc::now(),
        updated_at: Utc::now(),
        activated_at: None,
        error_message: None,
        last_seen_at: None,
        restart_count: 0,
        circuit_open: false,
    }
}

struct Uc {
    plugins: MockPluginRepository,
    webhooks: MockWebhookRepository,
    db_gateway: MockPluginDbGateway,
}

impl Uc {
    fn new() -> Self {
        Self {
            plugins: MockPluginRepository::new(),
            webhooks: MockWebhookRepository::new(),
            db_gateway: MockPluginDbGateway::new(),
        }
    }

    fn build(self) -> PluginUseCase {
        PluginUseCase::new(
            Arc::new(self.plugins),
            Arc::new(self.webhooks),
            Arc::new(NullPluginRuntime),
            Arc::new(self.db_gateway),
            Arc::new(NoopStoredFileRepository),
            Arc::new(NoopStorageService),
            Arc::new(NoopJobQueue),
            PathBuf::from("/tmp/plugins"),
        )
    }
}

#[tokio::test]
async fn activate_only_registers_hooks_that_were_granted_at_install_time() {
    let actor = AuthUserBuilder::admin().build();
    let plugin_id = Uuid::new_v4();
    let manifest = manifest_requesting_three_hooks();
    let granted = granted_only_before_post_create();

    let plugin = make_plugin(plugin_id, manifest.clone(), granted.clone());
    let plugin_after_activate = {
        let mut p = make_plugin(plugin_id, manifest, granted);
        p.status = PluginStatus::Active;
        p
    };

    let mut b = Uc::new();
    b.plugins.expect_find_by_slug().returning({
        let plugin = plugin.clone();
        move |_| Ok(Some(plugin.clone()))
    });
    b.plugins.expect_delete_hooks_for_plugin().returning(|_| Ok(()));
    b.plugins.expect_delete_ui_slots_for_plugin().returning(|_| Ok(()));
    b.plugins.expect_update_status().returning(|_, _, _| Ok(()));
    b.plugins.expect_update_activated_at().returning(|_| Ok(()));
    b.plugins.expect_append_log().returning(|_| Ok(()));
    b.plugins.expect_find_by_id().returning(move |_| Ok(Some(plugin_after_activate.clone())));

    // The security-critical assertion: create_hook must only ever be called for the
    // one hook that was both requested by the manifest AND granted by the admin.
    // If register_hooks_from_manifest regresses to trusting the manifest alone, this
    // closure will be invoked with "before_post_edit" or "before_thread_create" too
    // and the mock will return an error, failing the test.
    b.plugins.expect_create_hook().returning(|data| {
        if data.hook_name != "before_post_create" {
            return Err(ferum_domain::AppError::internal(format!(
                "ungranted hook '{}' must not be registered",
                data.hook_name
            )));
        }
        Ok(PluginHook {
            id: Uuid::new_v4(),
            plugin_id: data.plugin_id,
            hook_name: data.hook_name,
            priority: data.priority,
            is_active: true,
            avg_ms: None,
        })
    });

    let result = b.build().activate(&actor, "com.example.test-plugin").await;
    assert!(result.is_ok(), "activate should succeed: {:?}", result);
}

#[tokio::test]
async fn activate_registers_no_hooks_when_none_were_granted() {
    let actor = AuthUserBuilder::admin().build();
    let plugin_id = Uuid::new_v4();
    let manifest = manifest_requesting_three_hooks();
    let granted = serde_json::json!({ "hooks": [] });

    let plugin = make_plugin(plugin_id, manifest.clone(), granted.clone());
    let plugin_after_activate = {
        let mut p = make_plugin(plugin_id, manifest, granted);
        p.status = PluginStatus::Active;
        p
    };

    let mut b = Uc::new();
    b.plugins.expect_find_by_slug().returning({
        let plugin = plugin.clone();
        move |_| Ok(Some(plugin.clone()))
    });
    b.plugins.expect_delete_hooks_for_plugin().returning(|_| Ok(()));
    b.plugins.expect_delete_ui_slots_for_plugin().returning(|_| Ok(()));
    b.plugins.expect_update_status().returning(|_, _, _| Ok(()));
    b.plugins.expect_update_activated_at().returning(|_| Ok(()));
    b.plugins.expect_append_log().returning(|_| Ok(()));
    b.plugins.expect_find_by_id().returning(move |_| Ok(Some(plugin_after_activate.clone())));
    // No expect_create_hook() at all — if the use case calls it even once, the mock
    // panics with "MockPluginRepository::create_hook: No matching expectation found".

    let result = b.build().activate(&actor, "com.example.test-plugin").await;
    assert!(result.is_ok(), "activate should succeed: {:?}", result);
}

// ─── Config seeding from manifest defaults ───────────────────────────────────
//
// Installing with `config = {}` regardless of the manifest made `home-hero`
// activate cleanly and render nothing — no error, no log line. Every manifest
// under examples/ is written assuming `default` is honoured.
//
// Driven through the real `register_extracted`, so the wiring is covered, not
// just the pure extraction step.

/// Wires up the create → seed → status → reload path shared by the tests below,
/// capturing whatever config the use case decides to write.
fn expect_register(
    b: &mut Uc,
    plugin_id: Uuid,
    manifest: serde_json::Value,
    seen_config: Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
) {
    let created = make_plugin(plugin_id, manifest, serde_json::json!({}));

    b.plugins.expect_find_by_slug().returning(|_| Ok(None));
    b.plugins.expect_create().returning({
        let created = created.clone();
        move |_| Ok(created.clone())
    });
    b.plugins.expect_update_status().returning(|_, _, _| Ok(()));
    b.plugins.expect_append_log().returning(|_| Ok(()));
    b.plugins.expect_find_by_id().returning(move |_| Ok(Some(created.clone())));
    b.plugins.expect_update_config().returning(move |_, config| {
        seen_config.lock().unwrap().push(config);
        Ok(())
    });
}

async fn register(uc: PluginUseCase, manifest: serde_json::Value) {
    let actor = AuthUserBuilder::admin().build();
    uc.register_extracted(
        &actor,
        "com.example.test-plugin".to_string(),
        "Test Plugin".to_string(),
        "1.0.0".to_string(),
        PluginTier::Script,
        manifest,
        "/tmp/plugins/com.example.test-plugin".to_string(),
        serde_json::json!({}),
    )
    .await
    .expect("register_extracted should succeed");
}

#[tokio::test]
async fn install_seeds_config_from_declared_defaults() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut b = Uc::new();
    expect_register(
        &mut b,
        Uuid::new_v4(),
        serde_json::json!({
            "config_schema": {
                "type": "object",
                "properties": {
                    "message":     { "type": "string",  "default": "Welcome to the forum!" },
                    "dismissible": { "type": "boolean", "default": true },
                    "max_history": { "type": "integer", "default": 50 }
                }
            }
        }),
        Arc::clone(&seen),
    );

    register(b.build(), serde_json::json!({})).await;

    let written = seen.lock().unwrap();
    assert_eq!(written.len(), 1, "install must write the seed exactly once");
    assert_eq!(
        written[0],
        serde_json::json!({
            "message": "Welcome to the forum!",
            "dismissible": true,
            "max_history": 50
        }),
        "every declared default must be seeded, with its JSON type preserved — \
         a boolean seeded as the string \"true\" is a config a plugin cannot read"
    );
}

/// `home-hero` seeds an object of per-locale copy and an empty tile array. The
/// nested structure has to survive intact: a seeder that only copied scalars
/// would leave the masthead with no text, which is the exact bug this replaced.
#[tokio::test]
async fn structured_defaults_are_seeded_whole() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut b = Uc::new();
    expect_register(
        &mut b,
        Uuid::new_v4(),
        serde_json::json!({
            "config_schema": {
                "properties": {
                    "tiles":   { "type": "array",  "default": [] },
                    "locales": {
                        "type": "object",
                        "default": { "en": { "title": "Hello", "primary_cta": { "label": "Go", "href": "/x" } } }
                    }
                }
            }
        }),
        Arc::clone(&seen),
    );

    register(b.build(), serde_json::json!({})).await;

    let written = seen.lock().unwrap();
    assert_eq!(
        written[0]["locales"]["en"]["primary_cta"]["href"],
        serde_json::json!("/x"),
        "a nested default must be seeded whole, not flattened or dropped"
    );
    assert_eq!(
        written[0]["tiles"],
        serde_json::json!([]),
        "an empty array is a meaningful default and must not be treated as absent"
    );
}

/// A property with no `default` stays absent rather than becoming `null`: for a
/// plugin reading `Ferum.config.x` those are different answers, and a seeded
/// null would also fail a `required` check the manifest never intended to fail.
#[tokio::test]
async fn properties_without_a_default_are_not_seeded_as_null() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut b = Uc::new();
    expect_register(
        &mut b,
        Uuid::new_v4(),
        serde_json::json!({
            "config_schema": {
                "properties": {
                    "webhook_url": { "type": "string" },
                    "enabled":     { "type": "boolean", "default": false }
                }
            }
        }),
        Arc::clone(&seen),
    );

    register(b.build(), serde_json::json!({})).await;

    let written = seen.lock().unwrap();
    assert_eq!(written[0], serde_json::json!({ "enabled": false }));
    assert!(
        written[0].get("webhook_url").is_none(),
        "a property with no declared default must be absent, not null"
    );
    // `false` is a legitimate default and must not be skipped as "empty".
    assert_eq!(written[0]["enabled"], serde_json::json!(false));
}

/// Most plugins declare no config at all. No write, so nothing overwrites the
/// DB's own `'{}'` default and no log line claims a seed that did not happen.
#[tokio::test]
async fn nothing_is_written_when_the_manifest_declares_no_defaults() {
    for manifest in [
        serde_json::json!({}),
        serde_json::json!({ "config_schema": { "type": "object", "required": [] } }),
        serde_json::json!({ "config_schema": { "properties": { "url": { "type": "string" } } } }),
    ] {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut b = Uc::new();
        expect_register(&mut b, Uuid::new_v4(), manifest.clone(), Arc::clone(&seen));

        register(b.build(), serde_json::json!({})).await;

        assert!(
            seen.lock().unwrap().is_empty(),
            "update_config must not be called for manifest {manifest}"
        );
    }
}
