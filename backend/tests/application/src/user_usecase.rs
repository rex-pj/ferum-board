use std::sync::Arc;

use uuid::Uuid;
use ferum_application::usecases::user_usecase::{UpdateProfileCmd, UserUseCase};
use ferum_domain::AppError;
use ferum_test_support::fixtures::{make_user, AuthUserBuilder};
use ferum_test_support::mocks::{
    cache_service::MockCacheService,
    job_queue::NoopJobQueue,
    password_hasher::MockPasswordHasher,
    storage_service::NoopStorageService,
    stored_file_repository::NoopStoredFileRepository,
    user_repository::MockUserRepository,
};

fn build_uc(users: MockUserRepository, hasher: MockPasswordHasher) -> UserUseCase {
    UserUseCase::new(
        Arc::new(users),
        Arc::new(hasher),
        Arc::new(NoopStoredFileRepository),
        Arc::new(NoopStorageService),
        Arc::new(NoopJobQueue),
    )
}

// ─── update_profile ────────────────────────────────────────────────────────

#[tokio::test]
async fn update_profile_banned_returns_403() {
    let actor = AuthUserBuilder::member().banned().build();
    let uc = build_uc(MockUserRepository::new(), MockPasswordHasher::new());
    let result = uc.update_profile(&actor, UpdateProfileCmd::default()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn update_profile_display_name_too_long_returns_422() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(MockUserRepository::new(), MockPasswordHasher::new());
    let result = uc.update_profile(&actor, UpdateProfileCmd {
        display_name: Some("a".repeat(61)),
        ..Default::default()
    }).await;
    assert!(matches!(&result, Err(AppError::Invalid { code, .. }) if code == "display_name_length"),
        "expected display_name_length, got {result:?}");
}

#[tokio::test]
async fn update_profile_bio_too_long_returns_422() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(MockUserRepository::new(), MockPasswordHasher::new());
    let result = uc.update_profile(&actor, UpdateProfileCmd {
        bio: Some("x".repeat(501)),
        ..Default::default()
    }).await;
    assert!(matches!(&result, Err(AppError::Invalid { code, .. }) if code == "bio_too_long"),
        "expected bio_too_long, got {result:?}");
}

#[tokio::test]
async fn update_profile_invalid_website_returns_422() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(MockUserRepository::new(), MockPasswordHasher::new());
    let result = uc.update_profile(&actor, UpdateProfileCmd {
        website: Some("not-a-url".to_string()),
        ..Default::default()
    }).await;
    assert!(matches!(&result, Err(AppError::Invalid { code, .. }) if code == "invalid_website_url"),
        "expected invalid_website_url, got {result:?}");
}

#[tokio::test]
async fn update_profile_success() {
    let actor_id = Uuid::new_v4();
    let actor = AuthUserBuilder::member().with_id(actor_id).build();
    let updated_user = make_user(actor_id);

    let mut users = MockUserRepository::new();
    users.expect_update().returning(move |_, _| Ok(updated_user.clone()));

    let uc = build_uc(users, MockPasswordHasher::new());
    let result = uc.update_profile(&actor, UpdateProfileCmd {
        display_name: Some("New Name".to_string()),
        ..Default::default()
    }).await;
    assert!(result.is_ok());
}

// ─── change_password ───────────────────────────────────────────────────────

#[tokio::test]
async fn change_password_weak_new_password_returns_422() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(MockUserRepository::new(), MockPasswordHasher::new());
    let result = uc.change_password(&actor, "current_pass", "weak").await;
    assert!(matches!(&result, Err(AppError::Invalid { code, .. }) if code == "password_requirements"),
        "expected password_requirements, got {result:?}");
}

#[tokio::test]
async fn change_password_user_not_found_returns_404() {
    let actor = AuthUserBuilder::member().build();
    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(|_| Ok(None));

    let uc = build_uc(users, MockPasswordHasher::new());
    let result = uc.change_password(&actor, "current_pass", "NewPassword1!").await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn change_password_no_password_set_returns_403() {
    let actor_id = Uuid::new_v4();
    let actor = AuthUserBuilder::member().with_id(actor_id).build();
    let mut user = make_user(actor_id);
    user.password_hash = None; // OAuth user, no password

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(user.clone())));

    let uc = build_uc(users, MockPasswordHasher::new());
    let result = uc.change_password(&actor, "current_pass", "NewPassword1!").await;
    assert!(matches!(result, Err(AppError::Forbidden(c)) if c == "no_password_set"));
}

#[tokio::test]
async fn change_password_wrong_current_password_returns_403() {
    let actor_id = Uuid::new_v4();
    let actor = AuthUserBuilder::member().with_id(actor_id).build();
    let mut user = make_user(actor_id);
    user.password_hash = Some("$2b$12$hash".to_string());

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(user.clone())));

    let mut hasher = MockPasswordHasher::new();
    hasher.expect_verify().returning(|_, _| Ok(false));

    let uc = build_uc(users, hasher);
    let result = uc.change_password(&actor, "wrongpass", "NewPassword1!").await;
    assert!(matches!(result, Err(AppError::Forbidden(c)) if c == "incorrect_current_password"));
}

#[tokio::test]
async fn change_password_success() {
    let actor_id = Uuid::new_v4();
    let actor = AuthUserBuilder::member().with_id(actor_id).build();
    let mut user = make_user(actor_id);
    user.password_hash = Some("$2b$12$hash".to_string());

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(user.clone())));
    users.expect_set_password_hash().returning(|_, _| Ok(()));

    let mut hasher = MockPasswordHasher::new();
    hasher.expect_verify().returning(|_, _| Ok(true));
    hasher.expect_hash().returning(|_| Ok("$2b$12$newhash".to_string()));

    let uc = build_uc(users, hasher);
    assert!(uc.change_password(&actor, "CorrectPass1!", "NewPassword1!").await.is_ok());
}

/// The session epoch withdraws access tokens only — it is compared against
/// `iat`, and refreshing mints one stamped `now`, which always clears it. A
/// password change that leaves the refresh keys in place therefore ends no
/// other session at all: whoever holds a refresh cookie keeps minting access
/// tokens for its full lifetime. See the sibling test in `auth_usecase.rs`.
#[tokio::test]
async fn change_password_revokes_refresh_tokens() {
    let actor_id = Uuid::new_v4();
    let actor = AuthUserBuilder::member().with_id(actor_id).build();
    let mut user = make_user(actor_id);
    user.password_hash = Some("$2b$12$hash".to_string());

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(user.clone())));
    users.expect_set_password_hash().returning(|_, _| Ok(()));

    let mut hasher = MockPasswordHasher::new();
    hasher.expect_verify().returning(|_, _| Ok(true));
    hasher.expect_hash().returning(|_| Ok("$2b$12$newhash".to_string()));

    let mut cache = MockCacheService::new();
    cache.expect_set().returning(|_, _, _| Ok(()));
    cache
        .expect_del_prefix()
        .withf(move |prefix| prefix == format!("refresh:{actor_id}:"))
        .times(1)
        .returning(|_| Ok(()));

    let uc = build_uc(users, hasher).with_cache(Arc::new(cache));
    uc.change_password(&actor, "CorrectPass1!", "NewPassword1!").await.unwrap();
}
