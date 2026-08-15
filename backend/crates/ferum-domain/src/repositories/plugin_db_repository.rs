use async_trait::async_trait;

use crate::AppError;

/// Gives a Script-tier plugin its own Postgres schema (`plugin_{slug}`),
/// isolated by schema boundary plus a per-query `search_path` — never by
/// trusting the plugin's SQL text. DDL runs only at install, from the reviewed
/// manifest, never from a runtime query.
///
/// The hard boundary is the `ferum_plugin` role (migration 000015), which holds
/// no grant on any application table. Statement validation is a second layer,
/// and the only one left if the role could not be created — on a Postgres user
/// without `CREATEROLE` the gateway warns and falls back to it.
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
