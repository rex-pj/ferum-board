use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct Config {
    pub database_url: String,
    pub database_read_url: Option<String>,
    pub app_url: String,
    pub jwt_secret: String,
    pub from_email: String,

    pub smtp_host: String,
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
    #[serde(default = "default_cors")]
    pub cors_origins: String,
    #[allow(dead_code)]
    #[serde(default = "default_true")]
    pub registration_open: bool,
    #[allow(dead_code)]
    #[serde(default = "default_max_upload_mb")]
    pub max_upload_size_mb: u64,

    // Headless first-run setup — set all three to auto-create the first admin on startup.
    pub setup_admin_username: Option<String>,
    pub setup_admin_email: Option<String>,
    pub setup_admin_password: Option<String>,
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
            .field("cors_origins", &self.cors_origins)
            .field("registration_open", &self.registration_open)
            .field("max_upload_size_mb", &self.max_upload_size_mb)
            .field("setup_admin_username", &self.setup_admin_username)
            .field("setup_admin_email", &self.setup_admin_email)
            .field(
                "setup_admin_password",
                &self.setup_admin_password.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
}

fn default_smtp_port() -> u16 {
    587
}
fn default_true() -> bool {
    true
}
fn default_cors() -> String {
    "http://localhost:5173".to_string()
}
fn default_max_upload_mb() -> u64 {
    5
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
