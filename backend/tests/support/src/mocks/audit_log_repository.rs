use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use ferum_domain::models::audit_log::AuditLog;
use ferum_domain::repositories::audit_log_repository::AuditLogRepository;
use ferum_domain::AppError;

pub struct NoopAuditLogRepository;

#[async_trait]
impl AuditLogRepository for NoopAuditLogRepository {
    async fn append(&self, _log: AuditLog) -> Result<(), AppError> { Ok(()) }
    async fn list<'a>(&self, _actor_id: Option<Uuid>, _target_type: Option<&'a str>, _action_contains: Option<&'a str>, _created_from: Option<DateTime<Utc>>, _created_to: Option<DateTime<Utc>>, _page: u64, _per_page: u64) -> Result<(Vec<AuditLog>, u64), AppError> {
        Ok((vec![], 0))
    }
}

/// Keeps every appended entry so a test can assert what was recorded.
///
/// Not a mockall mock: these assertions are about the *content* of the entry
/// (action, target, metadata), and expressing that through `withf` predicates
/// reads far worse than inspecting the captured value.
#[derive(Default)]
pub struct SpyAuditLog {
    entries: std::sync::Mutex<Vec<AuditLog>>,
}

impl SpyAuditLog {
    pub fn entries(&self) -> Vec<AuditLog> {
        self.entries.lock().unwrap().clone()
    }

    /// The single entry recorded, panicking if there was not exactly one — the
    /// usual shape, and it makes "nothing was logged" a clear failure rather
    /// than an index panic.
    pub fn only(&self) -> AuditLog {
        let e = self.entries();
        assert_eq!(e.len(), 1, "expected exactly one audit entry, got {}: {e:#?}", e.len());
        e.into_iter().next().unwrap()
    }
}

#[async_trait]
impl AuditLogRepository for SpyAuditLog {
    async fn append(&self, log: AuditLog) -> Result<(), AppError> {
        self.entries.lock().unwrap().push(log);
        Ok(())
    }
    async fn list<'a>(&self, _actor_id: Option<Uuid>, _target_type: Option<&'a str>, _action_contains: Option<&'a str>, _created_from: Option<DateTime<Utc>>, _created_to: Option<DateTime<Utc>>, _page: u64, _per_page: u64) -> Result<(Vec<AuditLog>, u64), AppError> {
        Ok((vec![], 0))
    }
}
