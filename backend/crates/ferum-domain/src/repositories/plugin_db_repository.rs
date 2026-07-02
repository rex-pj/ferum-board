use async_trait::async_trait;

use crate::AppError;

/// Grants a Script-tier plugin its own real Postgres schema (`plugin_{slug}`),
/// isolated from core tables by schema boundary + a restrictive search_path
/// set for the duration of each query (never by trusting the plugin's SQL text
/// alone). Schema DDL only runs at install time, from the manifest's `[schema]`
/// section reviewed alongside other capabilities — never from a runtime query.
///
/// NOTE: this uses the same Postgres role as the core app, not a dedicated
/// least-privilege role per plugin. `query()` therefore layers defense-in-depth
/// (statement validation, forced search_path) on top of schema separation, but
/// is not a hard security boundary against a plugin author who deliberately
/// tries to escape it. A production deployment wanting a true boundary should
/// additionally provision a dedicated Postgres role per plugin with REVOKEd
/// access outside its own schema — not implemented here.
#[async_trait]
pub trait PluginDbGateway: Send + Sync {
    /// Create the plugin's schema (if missing) and run each DDL statement once,
    /// scoped to that schema. Called only at install time.
    async fn provision_schema(&self, slug: &str, statements: &[String]) -> Result<(), AppError>;
    /// Drop the plugin's entire schema (and everything in it). Called at uninstall.
    async fn drop_schema(&self, slug: &str) -> Result<(), AppError>;
    /// Execute a single plugin-authored SQL statement scoped to the plugin's schema.
    /// Statements starting with SELECT or WITH (a writable CTE — the standard way
    /// to get an INSERT/UPDATE/DELETE's RETURNING into a readable result in
    /// Postgres) must return a single JSON/JSONB column (e.g. via
    /// `row_to_json`/`jsonb_agg`) — the result is that column's value, or `null`
    /// if no row matched. All other statements return `{"rows_affected": n}`.
    async fn query(
        &self,
        slug: &str,
        sql: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value, AppError>;
}
