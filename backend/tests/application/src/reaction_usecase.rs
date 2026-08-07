use std::sync::Arc;

use ferum_application::usecases::reaction_usecase::ReactionUseCase;
use ferum_domain::AppError;
use ferum_domain::models::reaction::ReactionKind;
use ferum_domain::models::user::TrustLevel;
use ferum_test_support::fixtures::{ids, make_post, make_thread, AuthUserBuilder};
use ferum_test_support::mocks::{
    event_publisher::MockEventPublisher,
    post_repository::MockPostRepository,
    reaction_repository::MockReactionRepository,
    thread_repository::MockThreadRepository,
    user_repository::MockUserRepository,
};

// ─── Builder ──────────────────────────────────────────────────────────────────

struct Uc {
    reactions: MockReactionRepository,
    posts: MockPostRepository,
    threads: MockThreadRepository,
    users: MockUserRepository,
    events: MockEventPublisher,
}

impl Uc {
    fn new() -> Self {
        Self {
            reactions: MockReactionRepository::new(),
            posts: MockPostRepository::new(),
            threads: MockThreadRepository::new(),
            users: MockUserRepository::new(),
            events: MockEventPublisher::new(),
        }
    }

    fn build(self) -> ReactionUseCase {
        ReactionUseCase::new(
            Arc::new(self.reactions),
            Arc::new(self.posts),
            Arc::new(self.threads),
            Arc::new(self.users),
            Arc::new(self.events),
        )
    }
}

// ─── add ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn add_banned_user_returns_forbidden() {
    let actor = AuthUserBuilder::member().with_perm("reaction.add").banned().build();
    let result = Uc::new().build().add(&actor, ids::post_a(), ReactionKind::Like).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

/// Reacting needs the `reaction.add` grant, not just Basic trust.
///
/// `add` used to check only the ban flag and a hardcoded trust floor, so the
/// RBAC half of `reaction.add` went unread and revoking it did nothing.
#[tokio::test]
async fn add_requires_reaction_add_permission() {
    // AuthUserBuilder::member() carries no permissions of its own.
    let actor = AuthUserBuilder::member().build();
    let result = Uc::new().build().add(&actor, ids::post_a(), ReactionKind::Like).await;
    assert!(matches!(result, Err(AppError::Forbidden(c)) if c == "permission_denied"));
}

#[tokio::test]
async fn add_trust_level_new_returns_forbidden() {
    let actor = AuthUserBuilder::member().with_perm("reaction.add").with_trust(TrustLevel::New).build();
    let result = Uc::new().build().add(&actor, ids::post_a(), ReactionKind::Like).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn add_post_not_found_returns_not_found() {
    let actor = AuthUserBuilder::member().with_perm("reaction.add").build();
    let mut b = Uc::new();
    b.posts.expect_find_by_id().return_once(|_| Ok(None));
    let result = b.build().add(&actor, ids::post_a(), ReactionKind::Like).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn add_deleted_post_returns_not_found() {
    let actor = AuthUserBuilder::member().with_perm("reaction.add").with_id(ids::user_a()).build();
    let mut post = make_post(ids::post_a(), ids::thread_a(), ids::user_b());
    post.is_deleted = true;

    let mut b = Uc::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    let result = b.build().add(&actor, ids::post_a(), ReactionKind::Like).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn add_own_post_returns_forbidden() {
    let actor = AuthUserBuilder::member().with_perm("reaction.add").with_id(ids::user_a()).build();
    let post = make_post(ids::post_a(), ids::thread_a(), actor.id); // same author

    let mut b = Uc::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    let result = b.build().add(&actor, ids::post_a(), ReactionKind::Like).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn add_idempotent_when_already_reacted() {
    let actor = AuthUserBuilder::member().with_perm("reaction.add").with_id(ids::user_a()).build();
    let post = make_post(ids::post_a(), ids::thread_a(), ids::user_b());

    use ferum_domain::models::reaction::Reaction;
    let existing = Reaction {
        post_id: ids::post_a(),
        user_id: actor.id,
        kind: ReactionKind::Like,
        created_at: chrono::Utc::now(),
    };

    let mut b = Uc::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    b.reactions.expect_find().return_once(move |_, _, _| Ok(Some(existing)));
    b.reactions.expect_counts_by_post().return_once(|_| Ok(vec![]));

    let result = b.build().add(&actor, ids::post_a(), ReactionKind::Like).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn add_like_succeeds_and_publishes_event() {
    let actor = AuthUserBuilder::member().with_perm("reaction.add").with_id(ids::user_a()).build();
    let post = make_post(ids::post_a(), ids::thread_a(), ids::user_b());
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());

    let mut b = Uc::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    b.reactions.expect_find().return_once(|_, _, _| Ok(None));
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.reactions.expect_add().return_once(|_, _, _| {
        Ok(ferum_domain::models::reaction::Reaction {
            post_id: ids::post_a(),
            user_id: ids::user_a(),
            kind: ReactionKind::Like,
            created_at: chrono::Utc::now(),
        })
    });
    b.events.expect_publish().return_once(|_| ());
    b.reactions.expect_counts_by_post().return_once(|_| Ok(vec![]));
    // Like does NOT trigger trust_score increment — expect_increment_trust_score NOT set

    let result = b.build().add(&actor, ids::post_a(), ReactionKind::Like).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn add_helpful_triggers_trust_score_increment() {
    let actor = AuthUserBuilder::member().with_perm("reaction.add").with_id(ids::user_a()).build();
    let post = make_post(ids::post_a(), ids::thread_a(), ids::user_b());
    let thread = make_thread(ids::thread_a(), ids::category_a(), ids::user_b());

    let mut b = Uc::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    b.reactions.expect_find().return_once(|_, _, _| Ok(None));
    b.threads.expect_find_by_id().return_once(move |_| Ok(Some(thread)));
    b.reactions.expect_add().return_once(|_, _, _| {
        Ok(ferum_domain::models::reaction::Reaction {
            post_id: ids::post_a(),
            user_id: ids::user_a(),
            kind: ReactionKind::Helpful,
            created_at: chrono::Utc::now(),
        })
    });
    b.events.expect_publish().return_once(|_| ());
    b.reactions.expect_counts_by_post().return_once(|_| Ok(vec![]));
    b.users.expect_increment_trust_score().return_once(|_, _| Ok(()));

    let result = b.build().add(&actor, ids::post_a(), ReactionKind::Helpful).await;
    assert!(result.is_ok());
}

// ─── remove ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn remove_banned_user_returns_forbidden() {
    let actor = AuthUserBuilder::member().banned().build();
    let result = Uc::new().build().remove(&actor, ids::post_a(), ReactionKind::Like).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn remove_deleted_post_returns_not_found() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let mut post = make_post(ids::post_a(), ids::thread_a(), ids::user_b());
    post.is_deleted = true;

    let mut b = Uc::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    let result = b.build().remove(&actor, ids::post_a(), ReactionKind::Like).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn remove_like_publishes_event_without_trust_change() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let post = make_post(ids::post_a(), ids::thread_a(), ids::user_b());

    let mut b = Uc::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    b.reactions.expect_find().return_once(|_, _, _| {
        Ok(Some(ferum_domain::models::reaction::Reaction {
            post_id: ids::post_a(),
            user_id: ids::user_a(),
            kind: ReactionKind::Like,
            created_at: chrono::Utc::now(),
        }))
    });
    b.reactions.expect_remove().return_once(|_, _, _| Ok(()));
    b.events.expect_publish().return_once(|_| ());
    b.reactions.expect_counts_by_post().return_once(|_| Ok(vec![]));
    // Like removal does NOT trigger trust_score decrement

    let result = b.build().remove(&actor, ids::post_a(), ReactionKind::Like).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn remove_helpful_decrements_trust_score() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let post = make_post(ids::post_a(), ids::thread_a(), ids::user_b());

    let mut b = Uc::new();
    b.posts.expect_find_by_id().return_once(move |_| Ok(Some(post)));
    b.reactions.expect_find().return_once(|_, _, _| {
        Ok(Some(ferum_domain::models::reaction::Reaction {
            post_id: ids::post_a(),
            user_id: ids::user_a(),
            kind: ReactionKind::Helpful,
            created_at: chrono::Utc::now(),
        }))
    });
    b.reactions.expect_remove().return_once(|_, _, _| Ok(()));
    b.events.expect_publish().return_once(|_| ());
    b.reactions.expect_counts_by_post().return_once(|_| Ok(vec![]));
    b.users.expect_increment_trust_score().return_once(|_, _| Ok(()));

    let result = b.build().remove(&actor, ids::post_a(), ReactionKind::Helpful).await;
    assert!(result.is_ok());
}
