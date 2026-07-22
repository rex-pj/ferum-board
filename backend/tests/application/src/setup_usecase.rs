use std::sync::Arc;

use ferum_application::usecases::setup_usecase::{RunSetupCmd, SetupUseCase};
use ferum_domain::AppError;
use ferum_test_support::fixtures::{ids, make_assignment, make_role, make_user};
use ferum_test_support::mocks::{
    cache_service::MockCacheService,
    password_hasher::MockPasswordHasher,
    role_repository::MockRoleRepository,
    site_config::InMemorySiteConfig,
    token_service::MockTokenService,
    user_repository::MockUserRepository,
    user_role_repository::MockUserRoleRepository,
    bulk_seed_service::NoopBulkSeedService,
};

fn build_uc(
    users: MockUserRepository,
    roles: MockRoleRepository,
    user_roles: MockUserRoleRepository,
    hasher: MockPasswordHasher,
    tokens: MockTokenService,
    cache: MockCacheService,
) -> SetupUseCase {
    SetupUseCase::new(
        Arc::new(users),
        Arc::new(roles),
        Arc::new(user_roles),
        Arc::new(hasher),
        Arc::new(tokens),
        Arc::new(cache),
        Arc::new(InMemorySiteConfig::empty()),
        Arc::new(NoopBulkSeedService),
    )
}

// ─── needs_setup ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn needs_setup_true_when_no_admins_exist() {
    let mut users = MockUserRepository::new();
    users.expect_count_admins().return_once(|| Ok(0));

    let uc = build_uc(users, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockPasswordHasher::new(), MockTokenService::new(), MockCacheService::new());

    assert!(uc.needs_setup().await.expect("needs_setup succeeds"));
}

#[tokio::test]
async fn needs_setup_false_when_admin_exists() {
    let mut users = MockUserRepository::new();
    users.expect_count_admins().return_once(|| Ok(1));

    let uc = build_uc(users, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockPasswordHasher::new(), MockTokenService::new(), MockCacheService::new());

    assert!(!uc.needs_setup().await.expect("needs_setup succeeds"));
}

// ─── run_setup validation ─────────────────────────────────────────────────────

#[tokio::test]
async fn run_setup_username_too_short_returns_422() {
    let mut users = MockUserRepository::new();
    users.expect_count_admins().return_once(|| Ok(0));

    let uc = build_uc(users, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockPasswordHasher::new(), MockTokenService::new(), MockCacheService::new());

    let result = uc.run_setup(RunSetupCmd {
        admin_username: "ab".to_string(),
        admin_email: "admin@example.com".to_string(),
        admin_password: "Password1!".to_string(),
        config: None,
        seed_example_data: false,
    }).await;
    assert!(matches!(&result, Err(AppError::Invalid { code, .. }) if code == "invalid_username_format"));
}

#[tokio::test]
async fn run_setup_password_too_weak_returns_422() {
    let mut users = MockUserRepository::new();
    users.expect_count_admins().return_once(|| Ok(0));

    let uc = build_uc(users, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockPasswordHasher::new(), MockTokenService::new(), MockCacheService::new());

    let result = uc.run_setup(RunSetupCmd {
        admin_username: "adminuser".to_string(),
        admin_email: "admin@example.com".to_string(),
        admin_password: "weak".to_string(),
        config: None,
        seed_example_data: false,
    }).await;
    assert!(matches!(&result, Err(AppError::Invalid { code, .. }) if code == "password_requirements"));
}

#[tokio::test]
async fn run_setup_already_configured_returns_not_found() {
    // setup_guard protects the endpoint, but run_setup also checks internally
    let mut users = MockUserRepository::new();
    users.expect_count_admins().return_once(|| Ok(1)); // admin already exists

    let uc = build_uc(users, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockPasswordHasher::new(), MockTokenService::new(), MockCacheService::new());

    let result = uc.run_setup(RunSetupCmd {
        admin_username: "adminuser".to_string(),
        admin_email: "admin@example.com".to_string(),
        admin_password: "Password1!".to_string(),
        config: None,
        seed_example_data: false,
    }).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn run_setup_email_taken_returns_conflict() {
    let existing = make_user(ids::user_a());

    let mut users = MockUserRepository::new();
    users.expect_count_admins().return_once(|| Ok(0));
    users.expect_find_by_email().return_once(move |_| Ok(Some(existing)));

    let uc = build_uc(users, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockPasswordHasher::new(), MockTokenService::new(), MockCacheService::new());

    let result = uc.run_setup(RunSetupCmd {
        admin_username: "adminuser".to_string(),
        admin_email: "taken@example.com".to_string(),
        admin_password: "Password1!".to_string(),
        config: None,
        seed_example_data: false,
    }).await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn run_setup_success_returns_login_result() {
    let user_id = ids::user_a();
    let role_id = ids::category_a();
    let created = make_user(user_id);
    let admin_role = make_role(role_id, "admin");
    let assignment = make_assignment(user_id, role_id);

    let mut users = MockUserRepository::new();
    users.expect_count_admins().return_once(|| Ok(0));
    users.expect_find_by_email().return_once(|_| Ok(None));
    users.expect_find_by_username().return_once(|_| Ok(None));
    users.expect_create().return_once({
        let u = created.clone();
        move |_| Ok(u)
    });
    users.expect_set_email_verified().return_once(|_| Ok(()));
    users.expect_set_trust_level().return_once(|_, _| Ok(()));
    users.expect_find_by_id().return_once({
        let u = created.clone();
        move |_| Ok(Some(u))
    });

    let mut roles = MockRoleRepository::new();
    roles.expect_find_by_slug()
        .return_once(move |_| Ok(Some(admin_role)));

    let mut user_roles = MockUserRoleRepository::new();
    user_roles.expect_assign().return_once({
        let a = assignment.clone();
        move |_, _, _, _, _| Ok(a)
    });

    let mut hasher = MockPasswordHasher::new();
    hasher.expect_hash().return_once(|_| Ok("$2b$12$hash".to_string()));

    let mut tokens = MockTokenService::new();
    tokens.expect_mint_access_token().return_once(|_| Ok("access_tok".to_string()));
    tokens.expect_mint_refresh_token().return_once(|_| Ok("refresh_tok".to_string()));

    let mut cache = MockCacheService::new();
    cache.expect_set().return_once(|_, _, _| Ok(()));

    let uc = build_uc(users, roles, user_roles, hasher, tokens, cache);
    let result = uc.run_setup(RunSetupCmd {
        admin_username: "adminuser".to_string(),
        admin_email: "admin@example.com".to_string(),
        admin_password: "Password1!".to_string(),
        config: None,
        seed_example_data: false,
    }).await;

    assert!(result.is_ok());
    let login = result.unwrap();
    assert_eq!(login.access_token, "access_tok");
}
