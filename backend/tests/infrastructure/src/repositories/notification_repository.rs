//! Integration tests for [`PgNotificationRepository`].

use ferum_domain::models::notification::NotificationKind;
use ferum_domain::repositories::notification_repository::NotificationRepository;
use ferum_infrastructure::repositories::PgNotificationRepository;

use crate::common::{insert_user, TestDb};

#[tokio::test]
async fn list_for_user_on_empty_returns_zero() {
    let db = TestDb::new("notif_list_empty").await;
    let user = insert_user(&db.conn, 1).await;
    let repo = PgNotificationRepository::new(db.conn.clone());
    let (notifs, total) = repo.list_for_user(user.id, 1, 20).await.expect("list_for_user");
    assert_eq!(total, 0);
    assert!(notifs.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn create_and_list_notification() {
    let db = TestDb::new("notif_create_list").await;
    let user = insert_user(&db.conn, 1).await;
    let repo = PgNotificationRepository::new(db.conn.clone());

    repo.create(user.id, NotificationKind::System, serde_json::json!({"msg": "hello"}))
        .await.expect("create");

    let (notifs, total) = repo.list_for_user(user.id, 1, 20).await.expect("list_for_user");
    assert_eq!(total, 1);
    assert_eq!(notifs[0].user_id, user.id);
    assert_eq!(notifs[0].kind, NotificationKind::System);
    assert!(!notifs[0].is_read);
    db.teardown().await;
}

#[tokio::test]
async fn unread_count_increases_after_create() {
    let db = TestDb::new("notif_unread_count").await;
    let user = insert_user(&db.conn, 1).await;
    let repo = PgNotificationRepository::new(db.conn.clone());

    assert_eq!(repo.unread_count(user.id).await.expect("initial count"), 0);
    repo.create(user.id, NotificationKind::Reply, serde_json::json!({})).await.expect("create");
    assert_eq!(repo.unread_count(user.id).await.expect("count after create"), 1);
    db.teardown().await;
}

#[tokio::test]
async fn mark_read_sets_is_read_true() {
    let db = TestDb::new("notif_mark_read").await;
    let user = insert_user(&db.conn, 1).await;
    let repo = PgNotificationRepository::new(db.conn.clone());

    let notif = repo.create(user.id, NotificationKind::Mention, serde_json::json!({}))
        .await.expect("create");
    repo.mark_read(notif.id, user.id).await.expect("mark_read");

    let (notifs, _) = repo.list_for_user(user.id, 1, 20).await.expect("list");
    assert!(notifs[0].is_read);
    assert_eq!(repo.unread_count(user.id).await.expect("count"), 0);
    db.teardown().await;
}

#[tokio::test]
async fn mark_all_read_clears_all_unread() {
    let db = TestDb::new("notif_mark_all_read").await;
    let user = insert_user(&db.conn, 1).await;
    let repo = PgNotificationRepository::new(db.conn.clone());

    repo.create(user.id, NotificationKind::Reply, serde_json::json!({})).await.expect("create 1");
    repo.create(user.id, NotificationKind::Reaction, serde_json::json!({})).await.expect("create 2");
    assert_eq!(repo.unread_count(user.id).await.expect("before"), 2);

    repo.mark_all_read(user.id).await.expect("mark_all_read");
    assert_eq!(repo.unread_count(user.id).await.expect("after"), 0);
    db.teardown().await;
}

#[tokio::test]
async fn list_for_user_paginates() {
    let db = TestDb::new("notif_pagination").await;
    let user = insert_user(&db.conn, 1).await;
    let repo = PgNotificationRepository::new(db.conn.clone());

    for _ in 0..5 {
        repo.create(user.id, NotificationKind::System, serde_json::json!({}))
            .await.expect("create");
    }

    let (page1, total) = repo.list_for_user(user.id, 1, 3).await.expect("page 1");
    assert_eq!(total, 5);
    assert_eq!(page1.len(), 3);

    let (page2, _) = repo.list_for_user(user.id, 2, 3).await.expect("page 2");
    assert_eq!(page2.len(), 2);
    db.teardown().await;
}
