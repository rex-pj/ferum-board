use std::sync::Arc;

use uuid::Uuid;
use mockall::predicate::eq;
use ferum_application::usecases::follow_usecase::FollowUseCase;
use ferum_domain::models::follow::Follow;
use ferum_test_support::fixtures::{ids, AuthUserBuilder};
use ferum_test_support::mocks::{
    event_publisher::MockEventPublisher,
    follow_repository::MockFollowRepository,
    user_repository::MockUserRepository,
};

fn make_follow(follower_id: Uuid, followed_id: Uuid) -> Follow {
    Follow {
        id: Uuid::new_v4(),
        follower_id,
        followed_id,
        created_at: chrono::Utc::now(),
    }
}

fn make_user(id: Uuid) -> ferum_domain::models::user::User {
    use ferum_domain::models::user::TrustLevel;
    ferum_domain::models::user::User {
        id,
        username: "target".to_string(),
        email: "target@example.com".to_string(),
        is_email_verified: true,
        display_name: None,
        password_hash: None,
        trust_level: TrustLevel::Member,
        primary_role_slug: None,
        trust_score: 0,
        post_count: 0,
        days_visited: 0,
        avatar_url: None,
        cover_url: None,
        bio: None,
        website: None,
        is_banned: false,
        banned_until: None,
        ban_reason: None,
        warn_count: 0,
        failed_login_count: 0,
        locked_until: None,
        created_at: chrono::Utc::now(),
        updated_at: None,
        deleted_at: None,
        last_seen_at: None,
    }
}

#[tokio::test]
async fn follow_returns_true_when_not_already_following() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let target_id = ids::user_b();

    let mut follows = MockFollowRepository::new();
    follows.expect_find().return_once(|_, _| Ok(None));
    follows
        .expect_add()
        .with(eq(actor.id), eq(target_id))
        .return_once(|f, t| Ok(make_follow(f, t)));

    let mut users = MockUserRepository::new();
    users
        .expect_find_by_id()
        .with(eq(target_id))
        .return_once(move |_| Ok(Some(make_user(target_id))));

    let mut events = MockEventPublisher::new();
    events.expect_publish().return_once(|_| ());

    let uc = FollowUseCase::new(Arc::new(follows), Arc::new(users), Arc::new(events));
    let result = uc.follow(&actor, target_id).await;

    assert!(result.unwrap());
}

#[tokio::test]
async fn follow_returns_true_immediately_when_already_following() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let target_id = ids::user_b();

    let mut follows = MockFollowRepository::new();
    follows
        .expect_find()
        .return_once(move |_, _| Ok(Some(make_follow(ids::user_a(), target_id))));

    let mut users = MockUserRepository::new();
    users
        .expect_find_by_id()
        .return_once(move |_| Ok(Some(make_user(target_id))));

    let events = MockEventPublisher::new(); // expect_publish NOT called

    let uc = FollowUseCase::new(Arc::new(follows), Arc::new(users), Arc::new(events));
    let result = uc.follow(&actor, target_id).await;

    assert!(result.unwrap());
}

#[tokio::test]
async fn follow_self_returns_unprocessable_entity() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();

    let follows = MockFollowRepository::new();
    let users = MockUserRepository::new();
    let events = MockEventPublisher::new();

    let uc = FollowUseCase::new(Arc::new(follows), Arc::new(users), Arc::new(events));
    let result = uc.follow(&actor, actor.id).await;

    assert!(matches!(result, Err(ferum_domain::AppError::UnprocessableEntity(_))));
}

#[tokio::test]
async fn follow_banned_user_returns_forbidden() {
    let actor = AuthUserBuilder::member().banned().build();
    let target_id = ids::user_b();

    let follows = MockFollowRepository::new();
    let users = MockUserRepository::new();
    let events = MockEventPublisher::new();

    let uc = FollowUseCase::new(Arc::new(follows), Arc::new(users), Arc::new(events));
    let result = uc.follow(&actor, target_id).await;

    assert!(matches!(result, Err(ferum_domain::AppError::Forbidden(_))));
}

#[tokio::test]
async fn follow_nonexistent_user_returns_not_found() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();
    let target_id = ids::user_b();

    let follows = MockFollowRepository::new();
    let mut users = MockUserRepository::new();
    users.expect_find_by_id().return_once(|_| Ok(None));

    let events = MockEventPublisher::new();

    let uc = FollowUseCase::new(Arc::new(follows), Arc::new(users), Arc::new(events));
    let result = uc.follow(&actor, target_id).await;

    assert!(matches!(result, Err(ferum_domain::AppError::NotFound)));
}

#[tokio::test]
async fn unfollow_banned_user_returns_forbidden() {
    let actor = AuthUserBuilder::member().banned().build();

    let follows = MockFollowRepository::new();
    let users = MockUserRepository::new();
    let events = MockEventPublisher::new();

    let uc = FollowUseCase::new(Arc::new(follows), Arc::new(users), Arc::new(events));
    let result = uc.unfollow(&actor, ids::user_b()).await;

    assert!(matches!(result, Err(ferum_domain::AppError::Forbidden(_))));
}
