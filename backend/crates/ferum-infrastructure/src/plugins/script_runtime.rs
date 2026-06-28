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
    use ferum_domain::models::plugin::NewPluginLog;
    use ferum_domain::repositories::plugin_repository::PluginRepository;

    use crate::network_utils::assert_no_private_ip;

    // ─── Message protocol ─────────────────────────────────────────────────────────

    pub enum ScriptMessage {
        ExecuteHook {
            hook_name: String,
            ctx_json: String,
            reply: oneshot::Sender<HookDecision>,
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
        plugin_repo: Arc<dyn PluginRepository>,
        cache: Arc<dyn CacheService>,
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
    };
})();

// Hook registry — plugin bundle registers handlers here
var __ferum_hooks = {};
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

                    // Async write to plugin_logs — block briefly
                    let plugin_id = s.plugin_id;
                    let repo = s.plugin_repo.clone();
                    let level_c = level.clone();
                    let msg_c = message.clone();
                    let ctx_c = context.clone();
                    s.rt_handle.spawn(async move {
                        let _ = repo.append_log(NewPluginLog {
                            plugin_id,
                            level: level_c,
                            hook_name: None,
                            duration_ms: None,
                            message: msg_c,
                            context: ctx_c,
                        }).await;
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
                        assert_no_private_ip(&url).await.map_err(|e| e)?;
                        let client = reqwest::Client::new();
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
                        assert_no_private_ip(&url).await.map_err(|e| e)?;
                        let client = reqwest::Client::new();
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
        pub fn new(
            slug: String,
            bundle_js: String,
            plugin_id: Uuid,
            config: serde_json::Value,
            plugin_repo: Arc<dyn PluginRepository>,
            cache: Arc<dyn CacheService>,
            http_allowlist: Vec<String>,
        ) -> Result<Self, AppError> {
            let rt_handle = tokio::runtime::Handle::current();

            let state = Arc::new(JsThreadState {
                plugin_id,
                plugin_slug: slug.clone(),
                config,
                plugin_repo,
                cache,
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
                .ok_or_else(|| AppError::unprocessable("Script plugin manifest missing script.bundle_file"))?;

            let path = PathBuf::from(install_path).join(bundle_file);
            std::fs::read_to_string(&path).map_err(|e| {
                AppError::internal(format!("Cannot read plugin bundle '{}': {}", path.display(), e))
            })
        }

        /// Extract HTTP allowlist from manifest.
        pub fn http_allowlist_from_manifest(manifest: &serde_json::Value) -> Vec<String> {
            manifest
                .get("capabilities")
                .and_then(|c| c.get("http_allowlist"))
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
