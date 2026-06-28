use std::sync::Arc;

use uuid::Uuid;
use ferum_application::ports::ForumJob;
use ferum_application::usecases::auth_usecase::{AuthUseCase, LoginCmd, RegisterCmd, ResetPasswordCmd};
use ferum_domain::AppError;
use ferum_test_support::fixtures::{make_assignment, make_role, make_user};
use ferum_test_support::mocks::{
    cache_service::MockCacheService,
    job_queue::MockJobQueue,
    password_hasher::MockPasswordHasher,
    role_repository::MockRoleRepository,
    token_service::MockTokenService,
    user_repository::MockUserRepository,
    user_role_repository::MockUserRoleRepository,
};

fn build_uc(
    users: MockUserRepository,
    roles: MockRoleRepository,
    user_roles: MockUserRoleRepository,
    hasher: MockPasswordHasher,
    tokens: MockTokenService,
    cache: MockCacheService,
    jobs: MockJobQueue,
) -> AuthUseCase {
    AuthUseCase::new(
        Arc::new(users),
        Arc::new(roles),
        Arc::new(user_roles),
        Arc::new(hasher),
        Arc::new(tokens),
        Arc::new(cache),
        Arc::new(jobs),
    )
}

// ─── register ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn register_username_too_short_returns_422() {
    let uc = build_uc(
        MockUserRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockPasswordHasher::new(), MockTokenService::new(), MockCacheService::new(), MockJobQueue::new(),
    );
    let result = uc.register(RegisterCmd {
        username: "ab".to_string(),
        email: "valid@example.com".to_string(),
        password: "Password1!".to_string(),
    }).await;
    assert!(matches!(result, Err(AppError::UnprocessableEntity(_))));
}

#[tokio::test]
async fn register_password_too_weak_returns_422() {
    let uc = build_uc(
        MockUserRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockPasswordHasher::new(), MockTokenService::new(), MockCacheService::new(), MockJobQueue::new(),
    );
    let result = uc.register(RegisterCmd {
        username: "validuser".to_string(),
        email: "valid@example.com".to_string(),
        password: "weak".to_string(),
    }).await;
    assert!(matches!(result, Err(AppError::UnprocessableEntity(_))));
}

#[tokio::test]
async fn register_email_taken_returns_409() {
    let user_id = Uuid::new_v4();
    let existing = make_user(user_id);
    let mut users = MockUserRepository::new();
    users.expect_find_by_email().returning(move |_| Ok(Some(existing.clone())));

    let uc = build_uc(
        users, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockPasswordHasher::new(), MockTokenService::new(), MockCacheService::new(), MockJobQueue::new(),
    );
    let result = uc.register(RegisterCmd {
        username: "newuser123".to_string(),
        email: "taken@example.com".to_string(),
        password: "Password1!".to_string(),
    }).await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn register_success_with_email_verification_enqueues_job() {
    let user_id = Uuid::new_v4();
    let role_id = Uuid::new_v4();
    let created_user = make_user(user_id);
    let member_role = make_role(role_id, "member");
    let assignment = make_assignment(user_id, role_id);

    let mut users = MockUserRepository::new();
    users.expect_find_by_email().returning(|_| Ok(None));
    users.expect_find_by_username().returning(|_| Ok(None));
    users.expect_create().returning({
        let u = created_user.clone();
        move |_| Ok(u.clone())
    });

    let mut roles = MockRoleRepository::new();
    roles.expect_list_default().returning(move || Ok(vec![member_role.clone()]));

    let mut user_roles = MockUserRoleRepository::new();
    user_roles.expect_assign().returning({
        let a = assignment.clone();
        move |_, _, _, _, _| Ok(a.clone())
    });

    let mut hasher = MockPasswordHasher::new();
    hasher.expect_hash().returning(|_| Ok("$2b$12$hash".to_string()));

    let mut tokens = MockTokenService::new();
    tokens.expect_mint_email_token().returning(|_, _| Ok("verify_token".to_string()));

    let mut jobs = MockJobQueue::new();
    jobs.expect_enqueue()
        .withf(|j| matches!(j, ForumJob::SendEmailVerification { .. }))
        .returning(|_| Ok(()));

    let uc = build_uc(users, roles, user_roles, hasher, tokens, MockCacheService::new(), jobs)
        .with_auto_verify_email(false);

    assert!(uc.register(RegisterCmd {
        username: "newuser123".to_string(),
        email: "new@example.com".to_string(),
        password: "Password1!".to_string(),
    }).await.is_ok());
}

#[tokio::test]
async fn register_auto_verify_does_not_enqueue_email_job() {
    let user_id = Uuid::new_v4();
    let role_id = Uuid::new_v4();
    let created_user = make_user(user_id);
    let member_role = make_role(role_id, "member");
    let assignment = make_assignment(user_id, role_id);

    let mut users = MockUserRepository::new();
    users.expect_find_by_email().returning(|_| Ok(None));
    users.expect_find_by_username().returning(|_| Ok(None));
    users.expect_create().returning({
        let u = created_user.clone();
        move |_| Ok(u.clone())
    });
    users.expect_set_email_verified().returning(|_| Ok(()));
    users.expect_set_trust_level().returning(|_, _| Ok(()));

    let mut roles = MockRoleRepository::new();
    roles.expect_list_default().returning(move || Ok(vec![member_role.clone()]));

    let mut user_roles = MockUserRoleRepository::new();
    user_roles.expect_assign().returning({
        let a = assignment.clone();
        move |_, _, _, _, _| Ok(a.clone())
    });

    let mut hasher = MockPasswordHasher::new();
    hasher.expect_hash().returning(|_| Ok("$2b$12$hash".to_string()));

    // No job expectations set → jobs.enqueue must NOT be called (mock panics on unexpected call)
    let uc = build_uc(users, roles, user_roles, hasher, MockTokenService::new(), MockCacheService::new(), MockJobQueue::new())
        .with_auto_verify_email(true);

    assert!(uc.register(RegisterCmd {
        username: "newuser123".to_string(),
        email: "new@example.com".to_string(),
        password: "Password1!".to_string(),
    }).await.is_ok());
}

// ─── login ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn login_user_not_found_returns_401() {
    let mut users = MockUserRepository::new();
    users.expect_find_by_email().returning(|_| Ok(None));

    let mut hasher = MockPasswordHasher::new();
    // Constant-time: verify still called with dummy hash
    hasher.expect_verify().returning(|_, _| Ok(false));

    let uc = build_uc(
        users, MockRoleRepository::new(), MockUserRoleRepository::new(),
        hasher, MockTokenService::new(), MockCacheService::new(), MockJobQueue::new(),
    );
    assert!(matches!(
        uc.login(LoginCmd { email: "no@example.com".to_string(), password: "pass".to_string() }).await,
        Err(AppError::Unauthorized)
    ));
}

#[tokio::test]
async fn login_account_locked_returns_403() {
    let user_id = Uuid::new_v4();
    let mut user = make_user(user_id);
    user.locked_until = Some(chrono::Utc::now() + chrono::Duration::hours(1));

    let mut users = MockUserRepository::new();
    users.expect_find_by_email().returning(move |_| Ok(Some(user.clone())));

    let mut hasher = MockPasswordHasher::new();
    hasher.expect_verify().returning(|_, _| Ok(false));

    let uc = build_uc(
        users, MockRoleRepository::new(), MockUserRoleRepository::new(),
        hasher, MockTokenService::new(), MockCacheService::new(), MockJobQueue::new(),
    );
    assert!(matches!(
        uc.login(LoginCmd { email: "locked@example.com".to_string(), password: "pass".to_string() }).await,
        Err(AppError::Forbidden(c)) if c == "account_locked"
    ));
}

#[tokio::test]
async fn login_wrong_password_returns_401() {
    let user_id = Uuid::new_v4();
    let mut user = make_user(user_id);
    user.password_hash = Some("$2b$12$correcthash".to_string());

    let mut users = MockUserRepository::new();
    users.expect_find_by_email().returning(move |_| Ok(Some(user.clone())));
    users.expect_increment_failed_login().returning(|_| Ok(1));

    let mut hasher = MockPasswordHasher::new();
    hasher.expect_verify().returning(|_, _| Ok(false));

    let uc = build_uc(
        users, MockRoleRepository::new(), MockUserRoleRepository::new(),
        hasher, MockTokenService::new(), MockCacheService::new(), MockJobQueue::new(),
    );
    assert!(matches!(
        uc.login(LoginCmd { email: "user@example.com".to_string(), password: "wrong".to_string() }).await,
        Err(AppError::Unauthorized)
    ));
}

#[tokio::test]
async fn login_email_not_verified_returns_403() {
    let user_id = Uuid::new_v4();
    let mut user = make_user(user_id);
    user.is_email_verified = false;
    user.password_hash = Some("$2b$12$hash".to_string());

    let mut users = MockUserRepository::new();
    users.expect_find_by_email().returning(move |_| Ok(Some(user.clone())));

    let mut hasher = MockPasswordHasher::new();
    hasher.expect_verify().returning(|_, _| Ok(true));

    let uc = build_uc(
        users, MockRoleRepository::new(), MockUserRoleRepository::new(),
        hasher, MockTokenService::new(), MockCacheService::new(), MockJobQueue::new(),
    );
    assert!(matches!(
        uc.login(LoginCmd { email: "unverified@example.com".to_string(), password: "Password1!".to_string() }).await,
        Err(AppError::Forbidden(c)) if c == "email_not_verified"
    ));
}

#[tokio::test]
async fn login_banned_returns_403() {
    let user_id = Uuid::new_v4();
    let mut user = make_user(user_id);
    user.is_email_verified = true;
    user.is_banned = true;
    user.banned_until = None;
    user.password_hash = Some("$2b$12$hash".to_string());

    let mut users = MockUserRepository::new();
    users.expect_find_by_email().returning(move |_| Ok(Some(user.clone())));

    let mut hasher = MockPasswordHasher::new();
    hasher.expect_verify().returning(|_, _| Ok(true));

    let uc = build_uc(
        users, MockRoleRepository::new(), MockUserRoleRepository::new(),
        hasher, MockTokenService::new(), MockCacheService::new(), MockJobQueue::new(),
    );
    assert!(matches!(
        uc.login(LoginCmd { email: "banned@example.com".to_string(), password: "Password1!".to_string() }).await,
        Err(AppError::Forbidden(c)) if c == "account_suspended"
    ));
}

#[tokio::test]
async fn login_success_returns_access_and_refresh_tokens() {
    let user_id = Uuid::new_v4();
    let mut user = make_user(user_id);
    user.is_email_verified = true;
    user.is_banned = false;
    user.password_hash = Some("$2b$12$hash".to_string());

    let mut users = MockUserRepository::new();
    users.expect_find_by_email().returning(move |_| Ok(Some(user.clone())));
    users.expect_reset_failed_login().returning(|_| Ok(()));
    users.expect_set_trust_level().returning(|_, _| Ok(()));
    // update_last_seen is spawned — allow zero or more calls
    users.expect_update_last_seen().returning(|_| Ok(()));

    let mut hasher = MockPasswordHasher::new();
    hasher.expect_verify().returning(|_, _| Ok(true));

    let mut tokens = MockTokenService::new();
    tokens.expect_mint_access_token().returning(|_| Ok("access_token".to_string()));
    tokens.expect_mint_refresh_token().returning(|_| Ok("refresh_token".to_string()));

    let mut cache = MockCacheService::new();
    cache.expect_set().returning(|_, _, _| Ok(()));

    let uc = build_uc(
        users, MockRoleRepository::new(), MockUserRoleRepository::new(),
        hasher, tokens, cache, MockJobQueue::new(),
    );
    let result = uc.login(LoginCmd {
        email: "user@example.com".to_string(),
        password: "Password1!".to_string(),
    }).await;
    assert!(result.is_ok());
    let login = result.unwrap();
    assert_eq!(login.access_token, "access_token");
    assert_eq!(login.refresh_token, "refresh_token");
}

// ─── logout ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn logout_deletes_refresh_token_from_cache() {
    let user_id = Uuid::new_v4();
    let mut cache = MockCacheService::new();
    cache.expect_del().times(1).returning(|_| Ok(()));

    let uc = build_uc(
        MockUserRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockPasswordHasher::new(), MockTokenService::new(), cache, MockJobQueue::new(),
    );
    assert!(uc.logout(user_id, "some_refresh_token").await.is_ok());
}

// ─── reset_password ────────────────────────────────────────────────────────

#[tokio::test]
async fn reset_password_weak_new_password_returns_422() {
    let uc = build_uc(
        MockUserRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockPasswordHasher::new(), MockTokenService::new(), MockCacheService::new(), MockJobQueue::new(),
    );
    let result = uc.reset_password(ResetPasswordCmd {
        token: "token".to_string(),
        new_password: "weak".to_string(),
    }).await;
    assert!(matches!(result, Err(AppError::UnprocessableEntity(_))));
}

#[tokio::test]
async fn reset_password_invalid_token_returns_403() {
    let mut tokens = MockTokenService::new();
    tokens.expect_verify_email_token()
        .returning(|_, _| Err(AppError::forbidden("invalid_or_expired_token")));

    let uc = build_uc(
        MockUserRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockPasswordHasher::new(), tokens, MockCacheService::new(), MockJobQueue::new(),
    );
    let result = uc.reset_password(ResetPasswordCmd {
        token: "badtoken".to_string(),
        new_password: "Password1!".to_string(),
    }).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn reset_password_token_already_used_returns_403() {
    let user_id = Uuid::new_v4();
    let mut tokens = MockTokenService::new();
    tokens.expect_verify_email_token().returning(move |_, _| Ok(user_id));

    let mut cache = MockCacheService::new();
    cache.expect_set_nx().returning(|_, _, _| Ok(false));

    let uc = build_uc(
        MockUserRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockPasswordHasher::new(), tokens, cache, MockJobQueue::new(),
    );
    let result = uc.reset_password(ResetPasswordCmd {
        token: "usedtoken".to_string(),
        new_password: "Password1!".to_string(),
    }).await;
    assert!(matches!(result, Err(AppError::Forbidden(c)) if c == "token_already_used"));
}

#[tokio::test]
async fn reset_password_success() {
    let user_id = Uuid::new_v4();
    let mut tokens = MockTokenService::new();
    tokens.expect_verify_email_token().returning(move |_, _| Ok(user_id));

    let mut cache = MockCacheService::new();
    cache.expect_set_nx().returning(|_, _, _| Ok(true));

    let mut hasher = MockPasswordHasher::new();
    hasher.expect_hash().returning(|_| Ok("$2b$12$newhash".to_string()));

    let mut users = MockUserRepository::new();
    users.expect_set_password_hash().returning(|_, _| Ok(()));

    let uc = build_uc(users, MockRoleRepository::new(), MockUserRoleRepository::new(), hasher, tokens, cache, MockJobQueue::new());
    assert!(uc.reset_password(ResetPasswordCmd {
        token: "validtoken".to_string(),
        new_password: "Password1!".to_string(),
    }).await.is_ok());
}

// ─── verify_email ──────────────────────────────────────────────────────────

#[tokio::test]
async fn verify_email_already_verified_is_noop() {
    let user_id = Uuid::new_v4();
    let mut user = make_user(user_id);
    user.is_email_verified = true;

    let mut tokens = MockTokenService::new();
    tokens.expect_verify_email_token().returning(move |_, _| Ok(user_id));

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(user.clone())));
    // set_email_verified and set_trust_level must NOT be called

    let uc = build_uc(users, MockRoleRepository::new(), MockUserRoleRepository::new(), MockPasswordHasher::new(), tokens, MockCacheService::new(), MockJobQueue::new());
    assert!(uc.verify_email("validtoken").await.is_ok());
}

#[tokio::test]
async fn verify_email_unverified_sets_verified_and_trust_level() {
    let user_id = Uuid::new_v4();
    let mut user = make_user(user_id);
    user.is_email_verified = false;

    let mut tokens = MockTokenService::new();
    tokens.expect_verify_email_token().returning(move |_, _| Ok(user_id));

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(user.clone())));
    users.expect_set_email_verified().times(1).returning(|_| Ok(()));
    users.expect_set_trust_level().times(1).returning(|_, _| Ok(()));

    let uc = build_uc(users, MockRoleRepository::new(), MockUserRoleRepository::new(), MockPasswordHasher::new(), tokens, MockCacheService::new(), MockJobQueue::new());
    assert!(uc.verify_email("validtoken").await.is_ok());
}
