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
    token_service::MockTokenService,
    user_repository::MockUserRepository,
};

fn build_uc(users: MockUserRepository, hasher: MockPasswordHasher) -> UserUseCase {
    build_uc_with_tokens(users, hasher, MockTokenService::new())
}

/// The same, with the token service the caller wants — only the unsubscribe path
/// cares what it does.
fn build_uc_with_tokens(
    users: MockUserRepository,
    hasher: MockPasswordHasher,
    tokens: MockTokenService,
) -> UserUseCase {
    UserUseCase::new(
        Arc::new(users),
        Arc::new(hasher),
        Arc::new(NoopStoredFileRepository),
        Arc::new(NoopStorageService),
        Arc::new(NoopJobQueue),
        Arc::new(tokens),
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

// ─── unsubscribe_from_emails ─────────────────────────────────────────────────
//
// Backs the one-click link in every notification email. The link is followed
// without a session — from a mail client, by someone who is often not logged in —
// so the signed token is the only authorisation the request carries, and these
// tests pin what that token may and may not do.

use ferum_domain::models::{EmailNotificationPrefs, UserPreferences};
use ferum_application::ports::UNSUBSCRIBE_PURPOSE;

#[tokio::test]
async fn unsubscribe_turns_every_email_flag_off() {
    let user_id = Uuid::new_v4();

    let mut tokens = MockTokenService::new();
    tokens
        .expect_verify_email_token()
        // The purpose must be checked, or a password-reset token would double as
        // an unsubscribe link.
        .withf(move |_, purpose| purpose == UNSUBSCRIBE_PURPOSE)
        .returning(move |_, _| Ok(user_id));

    let mut users = MockUserRepository::new();
    users.expect_get_preferences().returning(move |_| {
        Ok(UserPreferences {
            user_id,
            email_notifications: EmailNotificationPrefs::OPTED_IN.to_json(),
            ..Default::default()
        })
    });
    users
        .expect_upsert_preferences()
        .withf(move |p| {
            // Written for the token's subject, with every flag off.
            p.user_id == user_id
                && EmailNotificationPrefs::from_json(&p.email_notifications).all_off()
        })
        .times(1)
        .returning(|_| Ok(()));

    let uc = build_uc_with_tokens(users, MockPasswordHasher::new(), tokens);
    uc.unsubscribe_from_emails("a-token").await.unwrap();
}

#[tokio::test]
async fn unsubscribe_preserves_unrelated_preferences() {
    let user_id = Uuid::new_v4();

    let mut tokens = MockTokenService::new();
    tokens.expect_verify_email_token().returning(move |_, _| Ok(user_id));

    let mut users = MockUserRepository::new();
    users.expect_get_preferences().returning(move |_| {
        Ok(UserPreferences {
            user_id,
            theme: "dark".into(),
            timezone: Some("Asia/Ho_Chi_Minh".into()),
            email_notifications: EmailNotificationPrefs::OPTED_IN.to_json(),
            ..Default::default()
        })
    });
    users
        .expect_upsert_preferences()
        // Turning email off must not reset someone's theme or time zone — the row
        // is written whole, so anything not read back first would be silently lost.
        .withf(|p| p.theme == "dark" && p.timezone.as_deref() == Some("Asia/Ho_Chi_Minh"))
        .times(1)
        .returning(|_| Ok(()));

    let uc = build_uc_with_tokens(users, MockPasswordHasher::new(), tokens);
    uc.unsubscribe_from_emails("a-token").await.unwrap();
}

#[tokio::test]
async fn unsubscribe_is_idempotent() {
    // Mail clients prefetch links and people click twice. A second visit must
    // succeed rather than error, or the page would tell someone their unsubscribe
    // failed when it had already worked.
    let user_id = Uuid::new_v4();

    let mut tokens = MockTokenService::new();
    tokens.expect_verify_email_token().returning(move |_, _| Ok(user_id));

    let mut users = MockUserRepository::new();
    users.expect_get_preferences().returning(move |_| {
        Ok(UserPreferences {
            user_id,
            email_notifications: EmailNotificationPrefs::OPTED_OUT.to_json(),
            ..Default::default()
        })
    });
    users.expect_upsert_preferences().returning(|_| Ok(()));

    let uc = build_uc_with_tokens(users, MockPasswordHasher::new(), tokens);
    uc.unsubscribe_from_emails("a-token").await.unwrap();
}

#[tokio::test]
async fn an_invalid_token_writes_nothing() {
    let mut tokens = MockTokenService::new();
    tokens
        .expect_verify_email_token()
        .returning(|_, _| Err(AppError::forbidden("invalid_or_expired_token")));

    // A bare mock: any preference read or write would panic, which is the
    // assertion. A token that does not verify must not be able to change another
    // account's settings.
    let users = MockUserRepository::new();

    let uc = build_uc_with_tokens(users, MockPasswordHasher::new(), tokens);
    assert!(uc.unsubscribe_from_emails("nope").await.is_err());
}
