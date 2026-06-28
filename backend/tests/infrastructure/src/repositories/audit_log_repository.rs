//! Integration tests for [`PgAuditLogRepository`].

use uuid::Uuid;
use ferum_domain::models::audit_log::AuditLog;
use ferum_domain::repositories::audit_log_repository::AuditLogRepository;
use ferum_infrastructure::repositories::PgAuditLogRepository;

use crate::common::{insert_user, TestDb};

fn make_log(actor_id: Uuid, target_id: Uuid) -> AuditLog {
    AuditLog::user_action(actor_id, "test.action", "post", target_id, None)
}

#[tokio::test]
async fn list_on_empty_schema_returns_empty() {
    let db = TestDb::new("al_list_empty").await;
    let repo = PgAuditLogRepository::new(db.conn.clone());
    let (logs, total) = repo.list(None, None, None, None, None, 1, 20).await.expect("list");
    assert_eq!(total, 0);
    assert!(logs.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn append_and_list() {
    let db = TestDb::new("al_append_list").await;
    let actor = insert_user(&db.conn, 1).await;
    let target_id = Uuid::new_v4();
    let repo = PgAuditLogRepository::new(db.conn.clone());

    repo.append(make_log(actor.id, target_id)).await.expect("append");

    let (logs, total) = repo.list(None, None, None, None, None, 1, 20).await.expect("list");
    assert_eq!(total, 1);
    assert_eq!(logs[0].actor_id, Some(actor.id));
    assert_eq!(logs[0].action, "test.action");
    db.teardown().await;
}

#[tokio::test]
async fn list_filters_by_actor_id() {
    let db = TestDb::new("al_filter_actor").await;
    let actor_a = insert_user(&db.conn, 1).await;
    let actor_b = insert_user(&db.conn, 2).await;
    let target = Uuid::new_v4();
    let repo = PgAuditLogRepository::new(db.conn.clone());

    repo.append(make_log(actor_a.id, target)).await.expect("append a");
    repo.append(make_log(actor_b.id, target)).await.expect("append b");

    let (logs, total) = repo.list(Some(actor_a.id), None, None, None, None, 1, 20).await.expect("filter by actor");
    assert_eq!(total, 1);
    assert_eq!(logs[0].actor_id, Some(actor_a.id));
    db.teardown().await;
}

#[tokio::test]
async fn append_multiple_entries_and_paginate() {
    let db = TestDb::new("al_paginate").await;
    let actor = insert_user(&db.conn, 1).await;
    let repo = PgAuditLogRepository::new(db.conn.clone());

    for _ in 0..5 {
        repo.append(make_log(actor.id, Uuid::new_v4())).await.expect("append");
    }

    let (page1, total) = repo.list(None, None, None, None, None, 1, 3).await.expect("page 1");
    assert_eq!(total, 5);
    assert_eq!(page1.len(), 3);

    let (page2, _) = repo.list(None, None, None, None, None, 2, 3).await.expect("page 2");
    assert_eq!(page2.len(), 2);
    db.teardown().await;
}
