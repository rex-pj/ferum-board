//! Integration tests for [`PgReactionRepository`].
//!
//! Critical SQL: `counts_by_post` and `counts_by_posts` use `COUNT(*)::BIGINT` + GROUP BY
//! via `Expr::cust` and `PostgresQueryBuilder`.

use ferum_domain::models::reaction::ReactionKind;
use ferum_domain::repositories::reaction_repository::ReactionRepository;
use ferum_infrastructure::repositories::PgReactionRepository;

use crate::common::{insert_category, insert_post, insert_thread, insert_user, TestDb};

#[tokio::test]
async fn add_and_find_reaction() {
    let db = TestDb::new("rxn_add_find").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;
    let post = insert_post(&db.conn, thread.id, user.id).await;

    let repo = PgReactionRepository::new(db.conn.clone());
    let reaction = repo.add(post.id, user.id, ReactionKind::Like).await.expect("add");
    assert_eq!(reaction.kind, ReactionKind::Like);

    let found = repo.find(post.id, user.id, ReactionKind::Like).await.expect("find").unwrap();
    assert_eq!(found.post_id, post.id);
    db.teardown().await;
}

#[tokio::test]
async fn duplicate_reaction_returns_conflict() {
    let db = TestDb::new("rxn_duplicate_conflict").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;
    let post = insert_post(&db.conn, thread.id, user.id).await;

    let repo = PgReactionRepository::new(db.conn.clone());
    repo.add(post.id, user.id, ReactionKind::Helpful).await.expect("first add");
    let result = repo.add(post.id, user.id, ReactionKind::Helpful).await;
    assert!(result.is_err(), "duplicate reaction must fail");
    db.teardown().await;
}

#[tokio::test]
async fn remove_reaction_makes_find_return_none() {
    let db = TestDb::new("rxn_remove").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;
    let post = insert_post(&db.conn, thread.id, user.id).await;

    let repo = PgReactionRepository::new(db.conn.clone());
    repo.add(post.id, user.id, ReactionKind::Funny).await.expect("add");
    repo.remove(post.id, user.id, ReactionKind::Funny).await.expect("remove");
    let found = repo.find(post.id, user.id, ReactionKind::Funny).await.expect("find");
    assert!(found.is_none());
    db.teardown().await;
}

#[tokio::test]
async fn counts_by_post_empty_returns_empty_vec() {
    // `COUNT(*)::BIGINT` + `GROUP BY` on empty table must not error.
    let db = TestDb::new("rxn_counts_by_post_empty").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;
    let post = insert_post(&db.conn, thread.id, user.id).await;

    let repo = PgReactionRepository::new(db.conn.clone());
    let counts = repo.counts_by_post(post.id).await.expect("counts_by_post");
    assert!(counts.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn counts_by_post_returns_correct_counts() {
    let db = TestDb::new("rxn_counts_by_post_data").await;
    let user1 = insert_user(&db.conn, 1).await;
    let user2 = insert_user(&db.conn, 2).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user1.id).await;
    let post = insert_post(&db.conn, thread.id, user1.id).await;

    let repo = PgReactionRepository::new(db.conn.clone());
    repo.add(post.id, user1.id, ReactionKind::Like).await.expect("add like u1");
    repo.add(post.id, user2.id, ReactionKind::Like).await.expect("add like u2");
    repo.add(post.id, user1.id, ReactionKind::Insightful).await.expect("add insightful");

    let counts = repo.counts_by_post(post.id).await.expect("counts_by_post");
    let like_count = counts.iter().find(|(k, _)| *k == ReactionKind::Like).map(|(_, c)| *c).unwrap_or(0);
    let insightful_count = counts.iter().find(|(k, _)| *k == ReactionKind::Insightful).map(|(_, c)| *c).unwrap_or(0);
    assert_eq!(like_count, 2);
    assert_eq!(insightful_count, 1);
    db.teardown().await;
}

#[tokio::test]
async fn counts_by_posts_empty_ids_returns_empty_map() {
    let db = TestDb::new("rxn_counts_by_posts_empty_ids").await;
    let repo = PgReactionRepository::new(db.conn.clone());
    let result = repo.counts_by_posts(&[]).await.expect("counts_by_posts");
    assert!(result.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn counts_by_posts_groups_by_post_id() {
    let db = TestDb::new("rxn_counts_by_posts_grouped").await;
    let user = insert_user(&db.conn, 1).await;
    let user2 = insert_user(&db.conn, 2).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;
    let post_a = insert_post(&db.conn, thread.id, user.id).await;
    let post_b = insert_post(&db.conn, thread.id, user.id).await;

    let repo = PgReactionRepository::new(db.conn.clone());
    repo.add(post_a.id, user.id, ReactionKind::Like).await.expect("like on a");
    repo.add(post_b.id, user2.id, ReactionKind::Helpful).await.expect("helpful on b");

    let result = repo.counts_by_posts(&[post_a.id, post_b.id]).await.expect("counts_by_posts");
    assert!(result.contains_key(&post_a.id));
    assert!(result.contains_key(&post_b.id));
    db.teardown().await;
}

#[tokio::test]
async fn user_reactions_for_posts_empty_ids_returns_empty_map() {
    let db = TestDb::new("rxn_user_reactions_empty").await;
    let user = insert_user(&db.conn, 1).await;
    let repo = PgReactionRepository::new(db.conn.clone());
    let result = repo.user_reactions_for_posts(user.id, &[]).await.expect("user_reactions");
    assert!(result.is_empty());
    db.teardown().await;
}

#[tokio::test]
async fn user_reactions_for_posts_returns_user_specific_reactions() {
    let db = TestDb::new("rxn_user_reactions_specific").await;
    let user1 = insert_user(&db.conn, 1).await;
    let user2 = insert_user(&db.conn, 2).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user1.id).await;
    let post = insert_post(&db.conn, thread.id, user1.id).await;

    let repo = PgReactionRepository::new(db.conn.clone());
    repo.add(post.id, user1.id, ReactionKind::Like).await.expect("add");
    repo.add(post.id, user2.id, ReactionKind::Funny).await.expect("add other");

    // user1 should only see their own Like, not user2's Funny
    let result = repo.user_reactions_for_posts(user1.id, &[post.id]).await.expect("user_reactions");
    let kinds = result.get(&post.id).unwrap();
    assert_eq!(kinds.len(), 1);
    assert!(kinds.contains(&ReactionKind::Like));
    db.teardown().await;
}
