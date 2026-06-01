use std::sync::Arc;

use serde::Serialize;

use crate::application::permission::PermissionChecker;
use crate::application::shared::AppError;
use crate::middleware::auth::AuthUser;

/// Thin stats query — runs raw SQL aggregates rather than pulling full entity lists.
pub struct AdminStatsUseCase {
    db: Arc<sea_orm::DatabaseConnection>,
}

impl AdminStatsUseCase {
    pub fn new(db: Arc<sea_orm::DatabaseConnection>) -> Self {
        Self { db }
    }

    pub async fn dashboard(&self, actor: &AuthUser) -> Result<DashboardStats, AppError> {
        PermissionChecker::can_admin(actor)?;

        use sea_orm::{DbBackend, FromQueryResult, Statement};

        #[derive(FromQueryResult)]
        struct Row {
            users_total: i64,
            threads_total: i64,
            posts_total: i64,
            reports_pending: i64,
            new_users_today: i64,
            new_threads_today: i64,
        }

        let row = Row::find_by_statement(Statement::from_string(
            DbBackend::Postgres,
            r#"SELECT
                (SELECT COUNT(*)        FROM users)                                          AS users_total,
                (SELECT COUNT(*)        FROM threads WHERE status != 'deleted')              AS threads_total,
                (SELECT COUNT(*)        FROM posts   WHERE is_deleted = false)               AS posts_total,
                (SELECT COUNT(*)        FROM reports WHERE status = 'pending')               AS reports_pending,
                (SELECT COUNT(*)        FROM users   WHERE created_at >= now() - INTERVAL '1 day') AS new_users_today,
                (SELECT COUNT(*)        FROM threads WHERE created_at >= now() - INTERVAL '1 day'
                                                      AND status != 'deleted')               AS new_threads_today
            "#,
        ))
        .one(&*self.db)
        .await?
        .ok_or_else(|| AppError::internal("stats query returned no row"))?;

        Ok(DashboardStats {
            users_total: row.users_total as u64,
            threads_total: row.threads_total as u64,
            posts_total: row.posts_total as u64,
            reports_pending: row.reports_pending as u64,
            new_users_today: row.new_users_today as u64,
            new_threads_today: row.new_threads_today as u64,
        })
    }
}

#[derive(Serialize)]
pub struct DashboardStats {
    pub users_total: u64,
    pub threads_total: u64,
    pub posts_total: u64,
    pub reports_pending: u64,
    pub new_users_today: u64,
    pub new_threads_today: u64,
}
