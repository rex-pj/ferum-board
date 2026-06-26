use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct Config {
    pub database_url: String,
    pub database_read_url: Option<String>,
    pub app_url: String,
    pub jwt_secret: String,
    pub from_email: String,

    pub smtp_host: Option<String>,
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    pub smtp_user: Option<String>,
    pub smtp_pass: Option<String>,

    pub redis_url: Option<String>,
    pub s3_endpoint: Option<String>,
    pub s3_bucket: Option<String>,
    pub s3_access_key: Option<String>,
    pub s3_secret_key: Option<String>,
    pub cdn_base_url: Option<String>,
    pub meilisearch_url: Option<String>,
    pub meilisearch_key: Option<String>,

    #[serde(default = "default_true")]
    pub rate_limit_enabled: bool,
    /// Bật dedup view count qua cache (set_nx 24h/user/thread).
    /// Chỉ có tác dụng với user đã đăng nhập; guest không được tính khi bật.
    /// Nên bật khi đã cấu hình REDIS_URL. Mặc định: false.
    #[serde(default = "default_false")]
    pub dedup_view_counts: bool,
    #[serde(default = "default_cors")]
    pub cors_origins: String,
    #[serde(default = "default_max_upload_mb")]
    pub max_upload_size_mb: u64,

    // Headless first-run setup — set all three to auto-create the first admin on startup.
    pub setup_admin_username: Option<String>,
    pub setup_admin_email: Option<String>,
    pub setup_admin_password: Option<String>,

    // Theme system
    #[serde(default = "default_themes_dir")]
    pub themes_dir: String,
    #[serde(default = "default_admin_templates_dir")]
    pub admin_templates_dir: String,
    #[serde(default = "default_static_dir")]
    pub static_dir: String,

    // Plugin system
    #[serde(default = "default_plugins_dir")]
    pub plugins_dir: String,
    #[serde(default = "default_plugin_hook_timeout_ms")]
    pub plugin_hook_timeout_ms: u64,
    #[serde(default = "default_plugin_circuit_threshold")]
    pub plugin_circuit_threshold: u32,

    /// Number of trusted reverse proxies in front of this app.
    /// When > 0, X-Forwarded-For is read; the real client IP is the Nth-from-last entry.
    /// When 0 (default), X-Forwarded-For is ignored and the TCP peer address is used directly.
    /// Set to 1 when deployed behind a single Nginx/Cloudflare proxy.
    #[serde(default = "default_trusted_proxy_count")]
    pub trusted_proxy_count: u32,

    // Observability
    /// "pretty" (default, human-readable) or "json" (structured, for production log aggregators)
    #[serde(default = "default_log_format")]
    pub log_format: String,
    /// Fallback log level filter when RUST_LOG is not set (e.g. "info", "debug")
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Emit a WARN log for any repository query taking longer than this many ms
    #[serde(default = "default_slow_query_ms")]
    pub slow_query_ms: u64,
    /// Directory to write rotating log files. If unset, logs go to stdout only.
    pub log_dir: Option<String>,
}

fn default_log_format() -> String {
    "pretty".to_string()
}
fn default_log_level() -> String {
    "ferum_web=info,ferum_application=info,ferum_domain=info,ferum_infrastructure=info,tower_http=info".to_string()
}
fn default_slow_query_ms() -> u64 {
    500
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"[redacted]")
            .field(
                "database_read_url",
                &self.database_read_url.as_ref().map(|_| "[redacted]"),
            )
            .field("app_url", &self.app_url)
            .field("jwt_secret", &"[redacted]")
            .field("from_email", &self.from_email)
            .field("smtp_host", &self.smtp_host)
            .field("smtp_port", &self.smtp_port)
            .field("smtp_user", &self.smtp_user)
            .field("smtp_pass", &self.smtp_pass.as_ref().map(|_| "[redacted]"))
            .field("redis_url", &self.redis_url.as_ref().map(|_| "[redacted]"))
            .field("s3_endpoint", &self.s3_endpoint)
            .field("s3_bucket", &self.s3_bucket)
            .field(
                "s3_access_key",
                &self.s3_access_key.as_ref().map(|_| "[redacted]"),
            )
            .field(
                "s3_secret_key",
                &self.s3_secret_key.as_ref().map(|_| "[redacted]"),
            )
            .field("cdn_base_url", &self.cdn_base_url)
            .field("meilisearch_url", &self.meilisearch_url)
            .field(
                "meilisearch_key",
                &self.meilisearch_key.as_ref().map(|_| "[redacted]"),
            )
            .field("rate_limit_enabled", &self.rate_limit_enabled)
            .field("dedup_view_counts", &self.dedup_view_counts)
            .field("cors_origins", &self.cors_origins)
            .field("max_upload_size_mb", &self.max_upload_size_mb)
            .field("themes_dir", &self.themes_dir)
            .field("admin_templates_dir", &self.admin_templates_dir)
            .field("static_dir", &self.static_dir)
            .field("plugins_dir", &self.plugins_dir)
            .field("plugin_hook_timeout_ms", &self.plugin_hook_timeout_ms)
            .field("plugin_circuit_threshold", &self.plugin_circuit_threshold)
            .field("log_format", &self.log_format)
            .field("log_level", &self.log_level)
            .field("slow_query_ms", &self.slow_query_ms)
            .field("setup_admin_username", &self.setup_admin_username)
            .field("setup_admin_email", &self.setup_admin_email)
            .field(
                "setup_admin_password",
                &self.setup_admin_password.as_ref().map(|_| "[redacted]"),
            )
            .field("trusted_proxy_count", &self.trusted_proxy_count)
            .finish()
    }
}

fn default_smtp_port() -> u16 {
    587
}
fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_cors() -> String {
    "http://localhost:5173".to_string()
}
fn default_max_upload_mb() -> u64 {
    5
}
fn default_themes_dir() -> String {
    "./frontend/themes".to_string()
}
fn default_admin_templates_dir() -> String {
    "./frontend/templates".to_string()
}
fn default_static_dir() -> String {
    "./frontend/static".to_string()
}
fn default_plugins_dir() -> String {
    "./plugins".to_string()
}
fn default_plugin_hook_timeout_ms() -> u64 {
    500
}
fn default_plugin_circuit_threshold() -> u32 {
    10
}
fn default_trusted_proxy_count() -> u32 {
    0
}

impl Config {
    pub fn from_env() -> Result<Self, config::ConfigError> {
        dotenvy::dotenv().ok();
        config::Config::builder()
            .add_source(config::Environment::default().try_parsing(true))
            .build()?
            .try_deserialize()
    }
}
