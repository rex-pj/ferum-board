//! Integration tests for [`PgFollowRepository`].

use ferum_domain::repositories::follow_repository::FollowRepository;
use ferum_infrastructure::repositories::PgFollowRepository;

use crate::common::{insert_user, TestDb};

#[tokio::test]
async fn find_on_empty_returns_none() {
    let db = TestDb::new("fw_find_empty").await;
    let u1 = insert_user(&db.conn, 1).await;
    let u2 = insert_user(&db.conn, 2).await;
    let repo = PgFollowRepository::new(db.conn.clone());
    assert!(repo.find(u1.id, u2.id).await.expect("find").is_none());
    db.teardown().await;
}

#[tokio::test]
async fn add_and_find_follow() {
    let db = TestDb::new("fw_add_find").await;
    let follower = insert_user(&db.conn, 1).await;
    let followed = insert_user(&db.conn, 2).await;

    let repo = PgFollowRepository::new(db.conn.clone());
    repo.add(follower.id, followed.id).await.expect("add");

    let found = repo.find(follower.id, followed.id).await.expect("find").unwrap();
    assert_eq!(found.follower_id, follower.id);
    assert_eq!(found.followed_id, followed.id);
    db.teardown().await;
}

#[tokio::test]
async fn remove_follow() {
    let db = TestDb::new("fw_remove").await;
    let follower = insert_user(&db.conn, 1).await;
    let followed = insert_user(&db.conn, 2).await;

    let repo = PgFollowRepository::new(db.conn.clone());
    repo.add(follower.id, followed.id).await.expect("add");
    repo.remove(follower.id, followed.id).await.expect("remove");
    assert!(repo.find(follower.id, followed.id).await.expect("find").is_none());
    db.teardown().await;
}

#[tokio::test]
async fn list_following_returns_followed_users() {
    let db = TestDb::new("fw_list_following").await;
    let follower = insert_user(&db.conn, 1).await;
    let u2 = insert_user(&db.conn, 2).await;
    let u3 = insert_user(&db.conn, 3).await;

    let repo = PgFollowRepository::new(db.conn.clone());
    repo.add(follower.id, u2.id).await.expect("follow u2");
    repo.add(follower.id, u3.id).await.expect("follow u3");

    let (list, total) = repo.list_following(follower.id, 1, 20).await.expect("list_following");
    assert_eq!(total, 2);
    assert_eq!(list.len(), 2);
    db.teardown().await;
}

#[tokio::test]
async fn list_followers_returns_users_following_target() {
    let db = TestDb::new("fw_list_followers").await;
    let target = insert_user(&db.conn, 1).await;
    let u2 = insert_user(&db.conn, 2).await;
    let u3 = insert_user(&db.conn, 3).await;

    let repo = PgFollowRepository::new(db.conn.clone());
    repo.add(u2.id, target.id).await.expect("u2 follows target");
    repo.add(u3.id, target.id).await.expect("u3 follows target");

    let (list, total) = repo.list_followers(target.id, 1, 20).await.expect("list_followers");
    assert_eq!(total, 2);
    assert_eq!(list.len(), 2);
    db.teardown().await;
}

#[tokio::test]
async fn count_following_and_followers() {
    let db = TestDb::new("fw_counts").await;
    let a = insert_user(&db.conn, 1).await;
    let b = insert_user(&db.conn, 2).await;
    let c = insert_user(&db.conn, 3).await;

    let repo = PgFollowRepository::new(db.conn.clone());
    repo.add(a.id, b.id).await.expect("a→b");
    repo.add(a.id, c.id).await.expect("a→c");
    repo.add(b.id, a.id).await.expect("b→a");

    assert_eq!(repo.count_following(a.id).await.expect("count_following a"), 2);
    assert_eq!(repo.count_followers(a.id).await.expect("count_followers a"), 1);
    db.teardown().await;
}
