//! Tier 2 Script Plugin Runtime — boa_engine (pure Rust JS engine)
//!
//! Each Script plugin runs in its own dedicated OS thread owning a boa_engine Context.
//! Communication with the main tokio runtime uses std channels + oneshot for replies.
//!
//! Plugin JS API note: hooks are synchronous from the JS perspective.
//! Cache and HTTP operations block the JS thread briefly via tokio Handle::block_on.
//!
//! Architecture:
//!   Main tokio runtime  ──(std::sync::mpsc + oneshot)──►  Plugin JS thread
//!                                                           └── boa_engine Context

#[cfg(feature = "script_plugins")]
mod inner {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use boa_engine::{
        Context, JsArgs, JsNativeError,
        JsValue, NativeFunction, Source,
    };
    use tokio::sync::oneshot;
    use uuid::Uuid;

    use ferum_application::ports::{CacheService, HookDecision};
    use ferum_application::shared::AppError;
    use ferum_domain::models::notification::NotificationKind;
    use ferum_domain::models::plugin::NewPluginLog;
    use ferum_domain::repositories::notification_repository::NotificationRepository;
    use ferum_domain::repositories::plugin_db_repository::PluginDbGateway;
    use ferum_domain::repositories::plugin_storage_repository::PluginStorageRepository;
    use ferum_domain::repositories::user_repository::UserRepository;

    // ─── Message protocol ─────────────────────────────────────────────────────────

    pub enum ScriptMessage {
        ExecuteHook {
            hook_name: String,
            ctx_json: String,
            reply: oneshot::Sender<HookDecision>,
        },
        /// Unlike ExecuteHook (fail-open, Allow on any error), RPC results propagate
        /// real errors to the HTTP caller — the request only exists because a client
        /// is waiting on a genuine response, not observing a side effect.
        ExecuteRpc {
            action: String,
            ctx_json: String,
            reply: oneshot::Sender<Result<serde_json::Value, String>>,
        },
        DispatchEvent {
            event_type: String,
            payload_json: String,
        },
        Shutdown,
    }

    // ─── Thread-local shared state ────────────────────────────────────────────────
    // Used by JS native callbacks to access Rust services without Arc cloning issues.

    struct JsThreadState {
        plugin_id: Uuid,
        plugin_slug: String,
        config: serde_json::Value,
        /// Buffered, batched writer for `plugin_logs`. Shared process-wide, so
        /// total logging cost stays at one connection however many plugins run.
        ///
        /// Replaces the `PluginRepository` this struct used to hold: logging was
        /// the only thing the JS thread ever did with it, and it now goes
        /// through the sink instead.
        log_sink: crate::plugins::log_sink::PluginLogSink,
        cache: Arc<dyn CacheService>,
        /// Durable KV store — the persistent counterpart to `cache` (which is TTL-bound).
        storage: Arc<dyn PluginStorageRepository>,
        user_repo: Arc<dyn UserRepository>,
        notification_repo: Arc<dyn NotificationRepository>,
        /// Names of `Ferum.forum.*` functions this plugin is allowed to call —
        /// intersection of manifest-requested and admin-granted, same trust
        /// model as http_allowlist and RPC actions.
        granted_api: Vec<String>,
        /// Scoped Postgres access to the plugin's own `plugin_{slug}` schema.
        db_gateway: Arc<dyn PluginDbGateway>,
        db_enabled: bool,
        http_allowlist: Vec<String>,
        /// Handle to the main tokio runtime — allows blocking async calls from sync JS callbacks
        rt_handle: tokio::runtime::Handle,
    }

    // ─── JS bootstrap — defines Ferum.* globals ───────────────────────────────────

    const FERUM_API_BOOTSTRAP: &str = r#"
var Ferum = (function() {
    'use strict';

    var _config = null;
    function getConfig() {
        if (_config === null) {
            try { _config = JSON.parse(__ferum_get_config()); } catch(e) { _config = {}; }
        }
        return _config;
    }

    return {
        get config() { return getConfig(); },

        log: {
            info:  function(msg, ctx) { __ferum_log('info',  String(msg), ctx != null ? JSON.stringify(ctx) : null); },
            warn:  function(msg, ctx) { __ferum_log('warn',  String(msg), ctx != null ? JSON.stringify(ctx) : null); },
            error: function(msg, ctx) { __ferum_log('error', String(msg), ctx != null ? JSON.stringify(ctx) : null); },
            trace: function(msg, ctx) { __ferum_log('trace', String(msg), ctx != null ? JSON.stringify(ctx) : null); },
        },

        cache: {
            get: function(key) {
                var r = __ferum_cache_get(String(key));
                return r != null ? r : null;
            },
            set: function(key, value, ttlSecs) {
                __ferum_cache_set(String(key), String(value), ttlSecs >>> 0);
            },
            del: function(key) {
                __ferum_cache_del(String(key));
            },
        },

        // Durable KV store — unlike cache (TTL-bound, may be evicted), storage
        // persists until the plugin explicitly deletes it or is uninstalled.
        // Values are arbitrary JSON (objects/arrays/strings/numbers), not just strings.
        storage: {
            get: function(key) {
                var r = __ferum_storage_get(String(key));
                return r != null ? JSON.parse(r) : null;
            },
            set: function(key, value) {
                __ferum_storage_set(String(key), JSON.stringify(value === undefined ? null : value));
            },
            del: function(key) {
                __ferum_storage_del(String(key));
            },
            list: function(prefix, limit) {
                var r = __ferum_storage_list(prefix ? String(prefix) : '', (limit >>> 0) || 100);
                return JSON.parse(r);
            },
        },

        http: {
            get: function(url, headers) {
                var r = __ferum_http_get(String(url), headers ? JSON.stringify(headers) : '{}');
                return r != null ? JSON.parse(r) : null;
            },
            post: function(url, body, headers) {
                var r = __ferum_http_post(
                    String(url),
                    body ? JSON.stringify(body) : '{}',
                    headers ? JSON.stringify(headers) : '{}'
                );
                return r != null ? JSON.parse(r) : null;
            },
        },

        utils: {
            sha256:  function(input) { return __ferum_sha256(String(input)); },
            now:     function() { return new Date().toISOString(); },
            slugify: function(s) {
                return String(s).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
            },
        },

        // Curated, capability-gated calls into core use cases. Each function is
        // only reachable if its name is granted via granted_capabilities.api —
        // see registry.rs's manifest ∩ granted intersection for hooks/rpc for
        // the same pattern. Throws if the plugin wasn't granted the capability.
        forum: {
            getUserPublic: function(userId) {
                var r = __ferum_forum_get_user_public(String(userId));
                return r != null ? JSON.parse(r) : null;
            },
            createNotification: function(userId, message) {
                __ferum_forum_create_notification(String(userId), String(message));
            },
        },

        // Scoped SQL access to this plugin's own Postgres schema (plugin_{slug}).
        // Only available when granted_capabilities.db === true. SELECT and WITH
        // (writable CTE — e.g. `WITH x AS (INSERT ... RETURNING ...) SELECT
        // row_to_json(x) FROM x`) statements must return a single JSON/JSONB
        // column (wrap with row_to_json/jsonb_agg); other statements return
        // { rows_affected }. Params are positional ($1, $2, ...), always bound —
        // never string-interpolate values into sql.
        db: {
            query: function(sql, params) {
                var r = __ferum_db_query(String(sql), params ? JSON.stringify(params) : '[]');
                return JSON.parse(r);
            },
        },
    };
})();

// Hook registry — plugin bundle registers handlers here
var __ferum_hooks = {};

// RPC registry — plugin bundle registers request handlers here.
// Handler signature: function(ctx) -> { ok: true, data: <any> } | { ok: false, error: "..." }
// Invoked via POST /api/plugins/:slug/rpc/:action (see ferum-web handlers/api/plugin_rpc.rs).
var __ferum_rpc = {};
"#;

    // ─── Register native functions into boa_engine Context ────────────────────────

    // Safety: all closures capture only Arc<JsThreadState> which holds no GC-managed objects.
    unsafe fn register_natives(ctx: &mut Context, state: Arc<JsThreadState>) {
        // __ferum_get_config() → String
        {
            let s = state.clone();
            ctx.register_global_callable(
                "__ferum_get_config".into(),
                0,
                NativeFunction::from_closure(move |_, _, _| {
                    Ok(JsValue::from(
                        boa_engine::JsString::from(
                            serde_json::to_string(&s.config).unwrap_or_else(|_| "{}".into()),
                        ),
                    ))
                }),
            )
            .expect("register __ferum_get_config");
        }

        // __ferum_log(level, message, contextJson?)
        {
            let s = state.clone();
            ctx.register_global_callable(
                "__ferum_log".into(),
                3,
                NativeFunction::from_closure(move |_, args, ctx| {
                    let level = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let message = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
                    let context_str = args.get_or_undefined(2).to_string(ctx)?.to_std_string_escaped();
                    let context: Option<serde_json::Value> =
                        if context_str == "null" || context_str.is_empty() {
                            None
                        } else {
                            serde_json::from_str(&context_str).ok()
                        };

                    // Mirror to tracing
                    match level.as_str() {
                        "error" => tracing::error!(plugin = %s.plugin_slug, "{}", message),
                        "warn"  => tracing::warn!(plugin = %s.plugin_slug, "{}", message),
                        _       => tracing::debug!(plugin = %s.plugin_slug, "[{}] {}", level, message),
                    }

                    // Hand off to the buffered sink rather than spawning a task
                    // that owns its own INSERT. A plugin controls how often
                    // this runs, so the per-call cost has to be a channel send:
                    // the previous shape turned one logging loop into thousands
                    // of concurrent writers and starved the pool. Overflow is
                    // dropped and counted inside the sink.
                    s.log_sink.try_log(NewPluginLog {
                        plugin_id: s.plugin_id,
                        level: level.clone(),
                        hook_name: None,
                        duration_ms: None,
                        message: message.clone(),
                        context: context.clone(),
                    });

                    Ok(JsValue::undefined())
                }),
            )
            .expect("register __ferum_log");
        }

        // __ferum_cache_get(key) → String | null
        {
            let s = state.clone();
            ctx.register_global_callable(
                "__ferum_cache_get".into(),
                1,
                NativeFunction::from_closure(move |_, args, ctx| {
                    let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let namespaced = format!("plugin:{}:{}", s.plugin_id, key);
                    let cache = s.cache.clone();
                    let result = s.rt_handle.block_on(cache.get(&namespaced));
                    match result {
                        Some(v) => Ok(JsValue::from(boa_engine::JsString::from(v))),
                        None => Ok(JsValue::null()),
                    }
                }),
            )
            .expect("register __ferum_cache_get");
        }

        // __ferum_cache_set(key, value, ttlSecs)
        {
            let s = state.clone();
            ctx.register_global_callable(
                "__ferum_cache_set".into(),
                3,
                NativeFunction::from_closure(move |_, args, ctx| {
                    let key   = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
                    let ttl_secs = args.get_or_undefined(2)
                        .to_u32(ctx)
                        .unwrap_or(300) as u64;
                    let namespaced = format!("plugin:{}:{}", s.plugin_id, key);
                    let cache = s.cache.clone();
                    let _ = s.rt_handle.block_on(
                        cache.set(&namespaced, &value, Duration::from_secs(ttl_secs))
                    );
                    Ok(JsValue::undefined())
                }),
            )
            .expect("register __ferum_cache_set");
        }

        // __ferum_cache_del(key)
        {
            let s = state.clone();
            ctx.register_global_callable(
                "__ferum_cache_del".into(),
                1,
                NativeFunction::from_closure(move |_, args, ctx| {
                    let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let namespaced = format!("plugin:{}:{}", s.plugin_id, key);
                    let cache = s.cache.clone();
                    let _ = s.rt_handle.block_on(cache.del(&namespaced));
                    Ok(JsValue::undefined())
                }),
            )
            .expect("register __ferum_cache_del");
        }

        // __ferum_storage_get(key) → String (JSON) | null
        {
            let s = state.clone();
            ctx.register_global_callable(
                "__ferum_storage_get".into(),
                1,
                NativeFunction::from_closure(move |_, args, ctx| {
                    let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let storage = s.storage.clone();
                    let plugin_id = s.plugin_id;
                    let result = s.rt_handle.block_on(storage.get(plugin_id, &key));
                    match result {
                        Ok(Some(v)) => Ok(JsValue::from(boa_engine::JsString::from(
                            serde_json::to_string(&v).unwrap_or_else(|_| "null".into()),
                        ))),
                        _ => Ok(JsValue::null()),
                    }
                }),
            )
            .expect("register __ferum_storage_get");
        }

        // __ferum_storage_set(key, valueJson)
        {
            let s = state.clone();
            ctx.register_global_callable(
                "__ferum_storage_set".into(),
                2,
                NativeFunction::from_closure(move |_, args, ctx| {
                    let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let value_json = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
                    let value: serde_json::Value =
                        serde_json::from_str(&value_json).unwrap_or(serde_json::Value::Null);
                    let storage = s.storage.clone();
                    let plugin_id = s.plugin_id;
                    let _ = s.rt_handle.block_on(storage.set(plugin_id, &key, value));
                    Ok(JsValue::undefined())
                }),
            )
            .expect("register __ferum_storage_set");
        }

        // __ferum_storage_del(key)
        {
            let s = state.clone();
            ctx.register_global_callable(
                "__ferum_storage_del".into(),
                1,
                NativeFunction::from_closure(move |_, args, ctx| {
                    let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let storage = s.storage.clone();
                    let plugin_id = s.plugin_id;
                    let _ = s.rt_handle.block_on(storage.delete(plugin_id, &key));
                    Ok(JsValue::undefined())
                }),
            )
            .expect("register __ferum_storage_del");
        }

        // __ferum_storage_list(prefix, limit) → String (JSON array of keys)
        {
            let s = state.clone();
            ctx.register_global_callable(
                "__ferum_storage_list".into(),
                2,
                NativeFunction::from_closure(move |_, args, ctx| {
                    let prefix = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let limit = args.get_or_undefined(1).to_u32(ctx).unwrap_or(100) as u64;
                    let storage = s.storage.clone();
                    let plugin_id = s.plugin_id;
                    let keys = s.rt_handle
                        .block_on(storage.list_keys(plugin_id, &prefix, limit.min(1000)))
                        .unwrap_or_default();
                    Ok(JsValue::from(boa_engine::JsString::from(
                        serde_json::to_string(&keys).unwrap_or_else(|_| "[]".into()),
                    )))
                }),
            )
            .expect("register __ferum_storage_list");
        }

        // __ferum_http_get(url, headersJson) → responseJson | null
        {
            let s = state.clone();
            ctx.register_global_callable(
                "__ferum_http_get".into(),
                2,
                NativeFunction::from_closure(move |_, args, ctx| {
                    let url = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let headers_json = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
                    let headers: HashMap<String, String> =
                        serde_json::from_str(&headers_json).unwrap_or_default();

                    if let Err(e) = validate_allowlist(&url, &s.http_allowlist) {
                        return Err(JsNativeError::error().with_message(e).into());
                    }

                    let result: Result<serde_json::Value, String> = s.rt_handle.block_on(async move {
                        let client = crate::network_utils::build_pinned_client(&url).await?;
                        let mut req = client.get(&url).timeout(Duration::from_secs(10));
                        for (k, v) in &headers { req = req.header(k, v); }
                        req.send().await.map_err(|e| e.to_string())?
                            .json::<serde_json::Value>().await.map_err(|e| e.to_string())
                    });

                    match result {
                        Ok(v) => Ok(JsValue::from(boa_engine::JsString::from(
                            serde_json::to_string(&v).unwrap_or_else(|_| "null".into()),
                        ))),
                        Err(e) => Err(JsNativeError::error().with_message(e).into()),
                    }
                }),
            )
            .expect("register __ferum_http_get");
        }

        // __ferum_http_post(url, bodyJson, headersJson) → responseJson | null
        {
            let s = state.clone();
            ctx.register_global_callable(
                "__ferum_http_post".into(),
                3,
                NativeFunction::from_closure(move |_, args, ctx| {
                    let url = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let body_json = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
                    let headers_json = args.get_or_undefined(2).to_string(ctx)?.to_std_string_escaped();

                    let body: serde_json::Value =
                        serde_json::from_str(&body_json).unwrap_or(serde_json::json!({}));
                    let headers: HashMap<String, String> =
                        serde_json::from_str(&headers_json).unwrap_or_default();

                    if let Err(e) = validate_allowlist(&url, &s.http_allowlist) {
                        return Err(JsNativeError::error().with_message(e).into());
                    }

                    let result: Result<serde_json::Value, String> = s.rt_handle.block_on(async move {
                        let client = crate::network_utils::build_pinned_client(&url).await?;
                        let mut req = client.post(&url).json(&body).timeout(Duration::from_secs(10));
                        for (k, v) in &headers { req = req.header(k, v); }
                        req.send().await.map_err(|e| e.to_string())?
                            .json::<serde_json::Value>().await.map_err(|e| e.to_string())
                    });

                    match result {
                        Ok(v) => Ok(JsValue::from(boa_engine::JsString::from(
                            serde_json::to_string(&v).unwrap_or_else(|_| "null".into()),
                        ))),
                        Err(e) => Err(JsNativeError::error().with_message(e).into()),
                    }
                }),
            )
            .expect("register __ferum_http_post");
        }

        // __ferum_sha256(input) → hex string
        {
            ctx.register_global_callable(
                "__ferum_sha256".into(),
                1,
                NativeFunction::from_closure(move |_, args, ctx| {
                    use sha2::{Digest, Sha256};
                    let input = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let hash = hex::encode(Sha256::digest(input.as_bytes()));
                    Ok(JsValue::from(boa_engine::JsString::from(hash)))
                }),
            )
            .expect("register __ferum_sha256");
        }

        // __ferum_forum_get_user_public(userId) → String (JSON) | null
        {
            let s = state.clone();
            ctx.register_global_callable(
                "__ferum_forum_get_user_public".into(),
                1,
                NativeFunction::from_closure(move |_, args, ctx| {
                    if !s.granted_api.iter().any(|a| a == "forum.getUserPublic") {
                        return Err(JsNativeError::error()
                            .with_message("Capability 'forum.getUserPublic' is not granted for this plugin")
                            .into());
                    }
                    let id_str = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let Ok(user_id) = id_str.parse::<Uuid>() else {
                        return Ok(JsValue::null());
                    };
                    let user_repo = s.user_repo.clone();
                    let result = s.rt_handle.block_on(user_repo.find_by_id(user_id));
                    match result {
                        Ok(Some(u)) => {
                            let public = serde_json::json!({
                                "id": u.id,
                                "username": u.username,
                                "display_name": u.display_name,
                                "avatar_url": u.avatar_url,
                                "trust_level": format!("{:?}", u.trust_level).to_lowercase(),
                            });
                            Ok(JsValue::from(boa_engine::JsString::from(
                                serde_json::to_string(&public).unwrap_or_else(|_| "null".into()),
                            )))
                        }
                        _ => Ok(JsValue::null()),
                    }
                }),
            )
            .expect("register __ferum_forum_get_user_public");
        }

        // __ferum_forum_create_notification(userId, message)
        {
            let s = state.clone();
            ctx.register_global_callable(
                "__ferum_forum_create_notification".into(),
                2,
                NativeFunction::from_closure(move |_, args, ctx| {
                    if !s.granted_api.iter().any(|a| a == "forum.createNotification") {
                        return Err(JsNativeError::error()
                            .with_message("Capability 'forum.createNotification' is not granted for this plugin")
                            .into());
                    }
                    let id_str = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let message = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
                    let Ok(user_id) = id_str.parse::<Uuid>() else {
                        return Ok(JsValue::undefined());
                    };
                    // Always kind=System — a plugin can never forge a "mention"/"reply"
                    // notification impersonating real user activity, only post its own
                    // clearly-plugin-sourced system message.
                    let payload = serde_json::json!({
                        "message": message,
                        "source_plugin": s.plugin_slug,
                    });
                    let notification_repo = s.notification_repo.clone();
                    let _ = s.rt_handle.block_on(
                        notification_repo.create(user_id, NotificationKind::System, payload)
                    );
                    Ok(JsValue::undefined())
                }),
            )
            .expect("register __ferum_forum_create_notification");
        }

        // __ferum_db_query(sql, paramsJson) → String (JSON)
        {
            let s = state.clone();
            ctx.register_global_callable(
                "__ferum_db_query".into(),
                2,
                NativeFunction::from_closure(move |_, args, ctx| {
                    if !s.db_enabled {
                        return Err(JsNativeError::error()
                            .with_message("Capability 'db' is not granted for this plugin")
                            .into());
                    }
                    let sql = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let params_json = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
                    let params: Vec<serde_json::Value> =
                        serde_json::from_str(&params_json).unwrap_or_default();

                    let gateway = s.db_gateway.clone();
                    let slug = s.plugin_slug.clone();
                    let result = s.rt_handle.block_on(gateway.query(&slug, &sql, params));

                    match result {
                        Ok(v) => Ok(JsValue::from(boa_engine::JsString::from(
                            serde_json::to_string(&v).unwrap_or_else(|_| "null".into()),
                        ))),
                        Err(e) => Err(JsNativeError::error()
                            .with_message(format!("{e}"))
                            .into()),
                    }
                }),
            )
            .expect("register __ferum_db_query");
        }
    }

    // ─── Hook execution helpers ───────────────────────────────────────────────────

    fn call_hook_sync(
        ctx: &mut Context,
        hook_name: &str,
        ctx_json: &str,
    ) -> HookDecision {
        let script = format!(
            r#"(function() {{
                try {{
                    var handler = (typeof __ferum_hooks === 'object') && __ferum_hooks[{hook_name:?}];
                    if (typeof handler !== 'function') {{
                        return JSON.stringify({{ allow: true }});
                    }}
                    var hookCtx = JSON.parse({ctx_json:?});
                    var result = handler(hookCtx);
                    if (!result) return JSON.stringify({{ allow: true }});
                    return JSON.stringify(result);
                }} catch (e) {{
                    try {{ Ferum.log.error('Hook error: ' + String(e)); }} catch(_) {{}}
                    return JSON.stringify({{ allow: true }});
                }}
            }})()"#
        );

        let result = ctx.eval(Source::from_bytes(&script));
        match result {
            Ok(JsValue::String(s)) => {
                let json = s.to_std_string_escaped();
                parse_hook_result_json(&json)
            }
            Ok(other) => {
                if let Ok(s) = other.to_string(ctx) {
                    parse_hook_result_json(&s.to_std_string_escaped())
                } else {
                    HookDecision::Allow
                }
            }
            Err(e) => {
                tracing::warn!("JS hook eval error: {:?}", e);
                HookDecision::Allow
            }
        }
    }

    fn call_event_sync(ctx: &mut Context, event_type: &str, payload_json: &str) {
        let script = format!(
            r#"(function() {{
                try {{
                    var handler = (typeof __ferum_hooks === 'object') && __ferum_hooks[{event_type:?}];
                    if (typeof handler === 'function') {{
                        var payload = JSON.parse({payload_json:?});
                        handler(payload);
                    }}
                }} catch (e) {{
                    try {{ Ferum.log.error('Event handler error: ' + String(e)); }} catch(_) {{}}
                }}
            }})()"#
        );
        let _ = ctx.eval(Source::from_bytes(&script));
    }

    /// Invoke a plugin's registered RPC handler (`__ferum_rpc[action]`).
    /// Contract: the handler returns `{ ok: true, data: <any> }` or
    /// `{ ok: false, error: "..." }`. Unlike hooks, RPC does not fail open —
    /// a missing handler, a thrown exception, or an `ok: false` result all
    /// surface as a real `Err` to the HTTP caller.
    fn call_rpc_sync(
        ctx: &mut Context,
        action: &str,
        ctx_json: &str,
    ) -> Result<serde_json::Value, String> {
        let script = format!(
            r#"(function() {{
                try {{
                    var handler = (typeof __ferum_rpc === 'object') && __ferum_rpc[{action:?}];
                    if (typeof handler !== 'function') {{
                        return JSON.stringify({{ ok: false, error: 'Unknown RPC action: ' + {action:?} }});
                    }}
                    var rpcCtx = JSON.parse({ctx_json:?});
                    var result = handler(rpcCtx);
                    if (!result || typeof result !== 'object') {{
                        return JSON.stringify({{ ok: false, error: 'RPC handler returned no result' }});
                    }}
                    return JSON.stringify(result);
                }} catch (e) {{
                    return JSON.stringify({{ ok: false, error: 'RPC handler threw: ' + String(e) }});
                }}
            }})()"#
        );

        let result = ctx.eval(Source::from_bytes(&script));
        let raw = match result {
            Ok(JsValue::String(s)) => s.to_std_string_escaped(),
            Ok(other) => other.to_string(ctx).map(|s| s.to_std_string_escaped())
                .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"non-string RPC result\"}".to_string()),
            Err(e) => return Err(format!("JS eval error: {e:?}")),
        };

        let parsed: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| format!("RPC handler returned invalid JSON: {e}"))?;

        if parsed.get("ok").and_then(|v| v.as_bool()) == Some(true) {
            Ok(parsed.get("data").cloned().unwrap_or(serde_json::Value::Null))
        } else {
            Err(parsed
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("RPC handler failed")
                .to_string())
        }
    }

    fn parse_hook_result_json(json: &str) -> HookDecision {
        let v: serde_json::Value =
            serde_json::from_str(json).unwrap_or(serde_json::json!({"allow": true}));

        if let Some(deny) = v.get("deny") {
            return HookDecision::Deny {
                reason: deny
                    .get("reason")
                    .and_then(|r| r.as_str())
                    .unwrap_or("blocked by plugin")
                    .to_string(),
                error_code: deny
                    .get("error_code")
                    .and_then(|c| c.as_str())
                    .unwrap_or("plugin_blocked")
                    .to_string(),
            };
        }
        HookDecision::Allow
    }

    // ─── SSRF allowlist validation ────────────────────────────────────────────────

    fn validate_allowlist(url: &str, allowlist: &[String]) -> Result<(), String> {
        if allowlist.is_empty() {
            return Err("outbound_http is not enabled for this plugin".into());
        }
        let parsed: reqwest::Url = url
            .parse()
            .map_err(|_| format!("Invalid URL: {}", url))?;
        let host = parsed.host_str().unwrap_or("");

        let allowed = allowlist.iter().any(|a| {
            host == a.as_str() || host.ends_with(&format!(".{}", a))
        });

        if !allowed {
            return Err(format!(
                "HTTP call to '{}' is not in the plugin's http_allowlist",
                host
            ));
        }
        Ok(())
    }

    // ─── JS thread event loop ─────────────────────────────────────────────────────

    fn run_js_thread(
        slug: String,
        bundle_js: String,
        state: Arc<JsThreadState>,
        rx: std::sync::mpsc::Receiver<ScriptMessage>,
    ) {
        let mut ctx = Context::default();

        // Register all Ferum.* native functions
        // Safety: see register_natives declaration
        unsafe { register_natives(&mut ctx, state) };

        // Bootstrap Ferum.* globals
        if let Err(e) = ctx.eval(Source::from_bytes(FERUM_API_BOOTSTRAP)) {
            tracing::error!(plugin = %slug, "Ferum API bootstrap failed: {:?}", e);
            return;
        }

        // Load the plugin bundle (registers __ferum_hooks)
        if let Err(e) = ctx.eval(Source::from_bytes(&bundle_js)) {
            tracing::error!(plugin = %slug, "Plugin bundle load failed: {:?}", e);
            return;
        }

        tracing::info!(plugin = %slug, "Script plugin runtime ready");

        // Process messages until Shutdown
        while let Ok(msg) = rx.recv() {
            match msg {
                ScriptMessage::ExecuteHook { hook_name, ctx_json, reply } => {
                    let decision = call_hook_sync(&mut ctx, &hook_name, &ctx_json);
                    let _ = reply.send(decision);
                }
                ScriptMessage::ExecuteRpc { action, ctx_json, reply } => {
                    let result = call_rpc_sync(&mut ctx, &action, &ctx_json);
                    let _ = reply.send(result);
                }
                ScriptMessage::DispatchEvent { event_type, payload_json } => {
                    call_event_sync(&mut ctx, &event_type, &payload_json);
                }
                ScriptMessage::Shutdown => {
                    tracing::info!(plugin = %slug, "Script plugin shutting down");
                    break;
                }
            }
        }
    }

    // ─── Public API ───────────────────────────────────────────────────────────────

    /// Manages a single script plugin's boa_engine Context on a dedicated OS thread.
    pub struct ScriptPluginRuntime {
        pub slug: String,
        tx: std::sync::mpsc::SyncSender<ScriptMessage>,
        _thread: std::thread::JoinHandle<()>,
    }

    impl ScriptPluginRuntime {
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            slug: String,
            bundle_js: String,
            plugin_id: Uuid,
            config: serde_json::Value,
            log_sink: crate::plugins::log_sink::PluginLogSink,
            cache: Arc<dyn CacheService>,
            storage: Arc<dyn PluginStorageRepository>,
            user_repo: Arc<dyn UserRepository>,
            notification_repo: Arc<dyn NotificationRepository>,
            granted_api: Vec<String>,
            db_gateway: Arc<dyn PluginDbGateway>,
            db_enabled: bool,
            http_allowlist: Vec<String>,
        ) -> Result<Self, AppError> {
            let rt_handle = tokio::runtime::Handle::current();

            let state = Arc::new(JsThreadState {
                plugin_id,
                plugin_slug: slug.clone(),
                config,
                log_sink,
                cache,
                storage,
                user_repo,
                notification_repo,
                granted_api,
                db_gateway,
                db_enabled,
                http_allowlist,
                rt_handle,
            });

            // Bounded sync channel — backpressure if JS thread is slow
            let (tx, rx) = std::sync::mpsc::sync_channel::<ScriptMessage>(32);
            let slug_clone = slug.clone();

            let thread = std::thread::Builder::new()
                .name(format!("plugin-js-{}", slug))
                .stack_size(4 * 1024 * 1024) // 4MB stack for JS execution
                .spawn(move || run_js_thread(slug_clone, bundle_js, state, rx))
                .map_err(|e| AppError::internal(format!("Failed to spawn plugin JS thread: {}", e)))?;

            Ok(Self { slug, tx, _thread: thread })
        }

        /// Read bundle JS from the plugin's install directory.
        pub fn load_bundle(install_path: &str, manifest: &serde_json::Value) -> Result<String, AppError> {
            let bundle_file = manifest
                .get("script")
                .and_then(|s| s.get("bundle_file"))
                .and_then(|f| f.as_str())
                .ok_or_else(|| AppError::invalid("plugin_manifest_missing_bundle_file"))?;

            let path = PathBuf::from(install_path).join(bundle_file);
            std::fs::read_to_string(&path).map_err(|e| {
                AppError::internal(format!("Cannot read plugin bundle '{}': {}", path.display(), e))
            })
        }

        /// Extract HTTP allowlist declared by the plugin's manifest.
        pub fn http_allowlist_from_manifest(manifest: &serde_json::Value) -> Vec<String> {
            manifest
                .get("capabilities")
                .and_then(|c| c.get("http_allowlist"))
                .and_then(|a| a.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default()
        }

        /// Extract HTTP allowlist the admin actually granted at install time.
        /// `granted_capabilities` mirrors a manifest `[capabilities]` table directly
        /// (i.e. `{"hooks": [...], "http_allowlist": [...]}`), not manifest-nested.
        pub fn http_allowlist_from_capabilities(granted_capabilities: &serde_json::Value) -> Vec<String> {
            granted_capabilities
                .get("http_allowlist")
                .and_then(|a| a.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default()
        }

        /// Execute a before-hook. Returns Allow on any error (fail-open).
        pub async fn execute_hook(&self, hook_name: &str, ctx_json: String) -> HookDecision {
            let (reply_tx, reply_rx) = oneshot::channel();
            let msg = ScriptMessage::ExecuteHook {
                hook_name: hook_name.to_string(),
                ctx_json,
                reply: reply_tx,
            };

            // SyncSender::send blocks until the channel has capacity. Calling it
            // directly inside an async fn stalls a tokio worker thread. Offload to
            // a blocking thread so the runtime stays responsive under back-pressure.
            let tx = self.tx.clone();
            let send_ok = tokio::task::spawn_blocking(move || tx.send(msg).is_ok())
                .await
                .unwrap_or(false);

            if !send_ok {
                tracing::warn!(plugin = %self.slug, "Script plugin channel closed — allowing");
                return HookDecision::Allow;
            }

            reply_rx.await.unwrap_or(HookDecision::Allow)
        }

        /// Invoke a registered RPC action. Unlike `execute_hook`, this does NOT
        /// fail open — timeouts, closed channels, and handler errors all surface
        /// as `Err` so the HTTP layer can return a real error to the client.
        pub async fn execute_rpc(&self, action: &str, ctx_json: String) -> Result<serde_json::Value, AppError> {
            let (reply_tx, reply_rx) = oneshot::channel();
            let msg = ScriptMessage::ExecuteRpc {
                action: action.to_string(),
                ctx_json,
                reply: reply_tx,
            };

            let tx = self.tx.clone();
            let send_ok = tokio::task::spawn_blocking(move || tx.send(msg).is_ok())
                .await
                .unwrap_or(false);

            if !send_ok {
                return Err(AppError::internal("Plugin runtime channel closed"));
            }

            match reply_rx.await {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(msg)) => Err(AppError::UnprocessableEntity(msg)),
                Err(_) => Err(AppError::internal("Plugin runtime dropped the RPC reply")),
            }
        }

        /// Extract RPC action names declared by the plugin's manifest.
        pub fn rpc_actions_from_manifest(manifest: &serde_json::Value) -> Vec<String> {
            manifest
                .get("capabilities")
                .and_then(|c| c.get("rpc"))
                .and_then(|a| a.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default()
        }

        /// Extract RPC action names the admin actually granted at install time.
        pub fn rpc_actions_from_capabilities(granted_capabilities: &serde_json::Value) -> Vec<String> {
            granted_capabilities
                .get("rpc")
                .and_then(|a| a.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default()
        }

        /// Extract `Ferum.forum.*` function names declared by the plugin's manifest.
        pub fn api_names_from_manifest(manifest: &serde_json::Value) -> Vec<String> {
            manifest
                .get("capabilities")
                .and_then(|c| c.get("api"))
                .and_then(|a| a.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default()
        }

        /// Extract `Ferum.forum.*` function names the admin actually granted at install time.
        pub fn api_names_from_capabilities(granted_capabilities: &serde_json::Value) -> Vec<String> {
            granted_capabilities
                .get("api")
                .and_then(|a| a.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default()
        }

        /// Whether the plugin's manifest requests the `db` capability
        /// (its own Postgres schema, see plugin_db_repository.rs).
        pub fn db_requested_by_manifest(manifest: &serde_json::Value) -> bool {
            manifest
                .get("capabilities")
                .and_then(|c| c.get("db"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        }

        /// Whether the admin actually granted the `db` capability at install time.
        pub fn db_granted_by_capabilities(granted_capabilities: &serde_json::Value) -> bool {
            granted_capabilities
                .get("db")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        }

        /// Dispatch an after-event (fire-and-forget from caller's perspective).
        pub async fn dispatch_event(&self, event_type: &str, payload_json: String) {
            if self.tx.try_send(ScriptMessage::DispatchEvent {
                event_type: event_type.to_string(),
                payload_json,
            }).is_err() {
                tracing::warn!(
                    plugin = %self.slug,
                    event = %event_type,
                    "After-event dropped — channel full or closed"
                );
            }
        }
    }

    impl Drop for ScriptPluginRuntime {
        fn drop(&mut self) {
            let _ = self.tx.try_send(ScriptMessage::Shutdown);
        }
    }
}

// ─── Re-export ────────────────────────────────────────────────────────────────

#[cfg(feature = "script_plugins")]
pub use inner::ScriptPluginRuntime;
