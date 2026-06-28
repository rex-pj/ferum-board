//! Integration tests for [`PgWebhookRepository`].

use ferum_domain::repositories::webhook_repository::{NewWebhook, UpdateWebhook, WebhookRepository};
use ferum_infrastructure::repositories::PgWebhookRepository;

use crate::common::TestDb;

fn new_webhook(events: Vec<&str>) -> NewWebhook {
    NewWebhook {
        url: "https://example.com/hook".to_string(),
        events: events.into_iter().map(|s| s.to_string()).collect(),
        secret: Some("secret123".to_string()),
        created_by_id: None,
        plugin_id: None,
    }
}

#[tokio::test]
async fn list_on_empty_returns_empty() {
    let db = TestDb::new("wh_list_empty").await;
    let repo = PgWebhookRepository::new(db.conn.clone());
    assert!(repo.list().await.expect("list").is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn create_and_find_by_id() {
    let db = TestDb::new("wh_create_find").await;
    let repo = PgWebhookRepository::new(db.conn.clone());
    let wh = repo.create(new_webhook(vec!["post.created"])).await.expect("create");

    let found = repo.find_by_id(wh.id).await.expect("find_by_id").unwrap();
    assert_eq!(found.id, wh.id);
    assert!(found.events.contains(&"post.created".to_string()));
    db.teardown().await;
}

#[tokio::test]
async fn find_subscribed_returns_matching_webhooks() {
    let db = TestDb::new("wh_find_subscribed").await;
    let repo = PgWebhookRepository::new(db.conn.clone());
    repo.create(new_webhook(vec!["post.created", "thread.created"])).await.expect("create a");
    repo.create(new_webhook(vec!["user.banned"])).await.expect("create b");

    let subs = repo.find_subscribed("post.created").await.expect("find_subscribed");
    assert_eq!(subs.len(), 1);
    assert!(subs[0].events.contains(&"post.created".to_string()));
    db.teardown().await;
}

#[tokio::test]
async fn update_webhook_url_and_events() {
    let db = TestDb::new("wh_update").await;
    let repo = PgWebhookRepository::new(db.conn.clone());
    let wh = repo.create(new_webhook(vec!["post.created"])).await.expect("create");

    let updated = repo.update(wh.id, UpdateWebhook {
        url: Some("https://updated.example.com/hook".to_string()),
        events: Some(vec!["thread.created".to_string()]),
        secret: None,
        is_active: None,
    }).await.expect("update");

    assert_eq!(updated.url, "https://updated.example.com/hook");
    assert!(!updated.events.contains(&"post.created".to_string()));
    assert!(updated.events.contains(&"thread.created".to_string()));
    db.teardown().await;
}

#[tokio::test]
async fn delete_removes_webhook() {
    let db = TestDb::new("wh_delete").await;
    let repo = PgWebhookRepository::new(db.conn.clone());
    let wh = repo.create(new_webhook(vec!["post.created"])).await.expect("create");
    repo.delete(wh.id).await.expect("delete");
    assert!(repo.find_by_id(wh.id).await.expect("find_by_id").is_none());
    db.teardown().await;
}

#[tokio::test]
async fn record_success_does_not_error() {
    let db = TestDb::new("wh_record_success").await;
    let repo = PgWebhookRepository::new(db.conn.clone());
    let wh = repo.create(new_webhook(vec!["post.created"])).await.expect("create");
    repo.record_success(wh.id).await.expect("record_success");
    db.teardown().await;
}

#[tokio::test]
async fn record_failure_does_not_error() {
    let db = TestDb::new("wh_record_failure").await;
    let repo = PgWebhookRepository::new(db.conn.clone());
    let wh = repo.create(new_webhook(vec!["post.created"])).await.expect("create");
    repo.record_failure(wh.id).await.expect("record_failure");
    db.teardown().await;
}

#[tokio::test]
async fn inactive_webhook_not_returned_by_find_subscribed() {
    let db = TestDb::new("wh_inactive").await;
    let repo = PgWebhookRepository::new(db.conn.clone());
    let wh = repo.create(new_webhook(vec!["post.created"])).await.expect("create");

    // Deactivate
    repo.update(wh.id, UpdateWebhook {
        is_active: Some(false),
        url: None, events: None, secret: None,
    }).await.expect("deactivate");

    let subs = repo.find_subscribed("post.created").await.expect("find_subscribed");
    assert!(subs.is_empty(), "inactive webhook should not appear in subscribed list");
    db.teardown().await;
}
