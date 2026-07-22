use std::sync::Arc;

use uuid::Uuid;
use ferum_application::usecases::admin_usecase::{AdminUseCase, CreateCategoryCmd};
use ferum_domain::AppError;
use ferum_domain::models::category::{PostPolicy, ViewPolicy};
use ferum_test_support::fixtures::{make_category, make_user, AuthUserBuilder};
use ferum_test_support::mocks::{
    audit_log_repository::NoopAuditLogRepository,
    cache_service::MockCacheService,
    category_repository::MockCategoryRepository,
    role_repository::MockRoleRepository,
    user_repository::MockUserRepository,
    user_role_repository::MockUserRoleRepository,
};

fn build_uc(
    categories: MockCategoryRepository,
    roles: MockRoleRepository,
    user_roles: MockUserRoleRepository,
    users: MockUserRepository,
    cache: MockCacheService,
) -> AdminUseCase {
    AdminUseCase::new(
        Arc::new(categories),
        Arc::new(roles),
        Arc::new(user_roles),
        Arc::new(users),
        Arc::new(NoopAuditLogRepository),
        Arc::new(cache),
    )
}

fn make_cmd(slug: &str) -> CreateCategoryCmd {
    CreateCategoryCmd {
        name: "Test Category".to_string(),
        slug: slug.to_string(),
        description: None,
        parent_id: None,
        position: 0,
        view_policy: ViewPolicy::Public,
        post_policy: PostPolicy::Members,
        color: None,
    }
}

// ─── create_category ───────────────────────────────────────────────────────

#[tokio::test]
async fn create_category_without_perm_returns_403() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(
        MockCategoryRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    let result = uc.create_category(&actor, make_cmd("test-cat")).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn create_category_reserved_slug_returns_422() {
    let actor = AuthUserBuilder::admin().build();
    let uc = build_uc(
        MockCategoryRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    // "admin" is a reserved slug
    let result = uc.create_category(&actor, make_cmd("admin")).await;
    assert!(matches!(&result, Err(AppError::Invalid { code, .. }) if code == "slug_reserved"),
        "expected slug_reserved, got {result:?}");
}

#[tokio::test]
async fn create_category_slug_taken_returns_409() {
    let actor = AuthUserBuilder::admin().build();
    let existing = make_category(Uuid::new_v4());

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_slug().returning(move |_| Ok(Some(existing.clone())));

    let uc = build_uc(
        cats, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    let result = uc.create_category(&actor, make_cmd("existing")).await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn create_category_parent_already_has_parent_returns_422() {
    let actor = AuthUserBuilder::admin().build();
    let grandparent_id = Uuid::new_v4();
    let parent_id = Uuid::new_v4();
    let mut parent = make_category(parent_id);
    parent.parent_id = Some(grandparent_id); // already a child

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_slug().returning(|_| Ok(None));
    cats.expect_find_by_id().returning(move |_| Ok(Some(parent.clone())));

    let uc = build_uc(
        cats, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    let cmd = CreateCategoryCmd { parent_id: Some(parent_id), ..make_cmd("child-cat") };
    let result = uc.create_category(&actor, cmd).await;
    assert!(matches!(&result, Err(AppError::Invalid { code, .. }) if code == "category_nesting_too_deep"),
        "expected category_nesting_too_deep, got {result:?}");
}

#[tokio::test]
async fn create_category_success() {
    let actor = AuthUserBuilder::admin().build();
    let cat_id = Uuid::new_v4();
    let created = make_category(cat_id);

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_slug().returning(|_| Ok(None));
    cats.expect_create().returning(move |_| Ok(created.clone()));

    let uc = build_uc(
        cats, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    let result = uc.create_category(&actor, make_cmd("new-category")).await;
    assert!(result.is_ok());
}

// ─── delete_category ───────────────────────────────────────────────────────

#[tokio::test]
async fn delete_category_without_perm_returns_403() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(
        MockCategoryRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    let result = uc.delete_category(&actor, Uuid::new_v4()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn delete_category_not_found_returns_404() {
    let actor = AuthUserBuilder::admin().build();
    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_id().returning(|_| Ok(None));

    let uc = build_uc(
        cats, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    let result = uc.delete_category(&actor, Uuid::new_v4()).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn delete_category_has_children_returns_409() {
    let actor = AuthUserBuilder::admin().build();
    let cat_id = Uuid::new_v4();
    let cat = make_category(cat_id);

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_id().returning(move |_| Ok(Some(cat.clone())));
    cats.expect_has_children().returning(|_| Ok(true));

    let uc = build_uc(
        cats, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    let result = uc.delete_category(&actor, cat_id).await;
    assert!(matches!(result, Err(AppError::Conflict(c)) if c == "category_has_subcategories"));
}

#[tokio::test]
async fn delete_category_has_threads_returns_409() {
    let actor = AuthUserBuilder::admin().build();
    let cat_id = Uuid::new_v4();
    let cat = make_category(cat_id);

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_id().returning(move |_| Ok(Some(cat.clone())));
    cats.expect_has_children().returning(|_| Ok(false));
    cats.expect_count_threads_by_categories()
        .returning(move |_| Ok(vec![(cat_id, 3)]));

    let uc = build_uc(
        cats, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    let result = uc.delete_category(&actor, cat_id).await;
    assert!(matches!(result, Err(AppError::Conflict(c)) if c == "category_has_threads"));
}

#[tokio::test]
async fn delete_category_empty_succeeds() {
    let actor = AuthUserBuilder::admin().build();
    let cat_id = Uuid::new_v4();
    let cat = make_category(cat_id);

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_id().returning(move |_| Ok(Some(cat.clone())));
    cats.expect_has_children().returning(|_| Ok(false));
    cats.expect_count_threads_by_categories()
        .returning(move |_| Ok(vec![(cat_id, 0)]));
    cats.expect_delete().returning(|_| Ok(()));

    let uc = build_uc(
        cats, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    let result = uc.delete_category(&actor, cat_id).await;
    assert!(result.is_ok());
}

// ─── permanent_ban ─────────────────────────────────────────────────────────

#[tokio::test]
async fn permanent_ban_without_perm_returns_403() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(
        MockCategoryRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    let result = uc.permanent_ban(&actor, Uuid::new_v4(), "spam".to_string()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn permanent_ban_user_not_found_returns_404() {
    let actor = AuthUserBuilder::admin().build();
    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(|_| Ok(None));

    let uc = build_uc(
        MockCategoryRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        users, MockCacheService::new(),
    );
    let result = uc.permanent_ban(&actor, Uuid::new_v4(), "spam".to_string()).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn permanent_ban_success() {
    let actor = AuthUserBuilder::admin().build();
    let target_id = Uuid::new_v4();
    let target_user = make_user(target_id);

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(target_user.clone())));
    users.expect_update().returning(move |_, _| Ok(make_user(target_id)));

    let mut cache = MockCacheService::new();
    cache.expect_set().returning(|_, _, _| Ok(()));
    cache.expect_del_prefix().returning(|_| Ok(()));
    cache.expect_del().returning(|_| Ok(()));

    let uc = build_uc(
        MockCategoryRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        users, cache,
    );
    let result = uc.permanent_ban(&actor, target_id, "spam".to_string()).await;
    assert!(result.is_ok());
}

// ─── ban (with optional expiry) ────────────────────────────────────────────
// Regression coverage for the bug where the admin ban endpoint silently
// dropped `banned_until` and always banned permanently regardless of what
// the caller requested.

#[tokio::test]
async fn ban_with_expiry_sets_banned_until_on_the_update_patch() {
    let actor = AuthUserBuilder::admin().build();
    let target_id = Uuid::new_v4();
    let target_user = make_user(target_id);
    let until = chrono::Utc::now() + chrono::Duration::days(7);

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(target_user.clone())));
    users.expect_update().returning(move |_, patch| {
        assert_eq!(patch.is_banned, Some(true));
        assert_eq!(patch.banned_until, Some(Some(until)));
        Ok(make_user(target_id))
    });

    let mut cache = MockCacheService::new();
    cache.expect_set().returning(|_, _, _| Ok(()));
    cache.expect_del_prefix().returning(|_| Ok(()));
    cache.expect_del().returning(|_| Ok(()));

    let uc = build_uc(
        MockCategoryRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        users, cache,
    );
    let result = uc.ban(&actor, target_id, "spam".to_string(), Some(until)).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn ban_without_expiry_sets_banned_until_to_none() {
    let actor = AuthUserBuilder::admin().build();
    let target_id = Uuid::new_v4();
    let target_user = make_user(target_id);

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(target_user.clone())));
    users.expect_update().returning(move |_, patch| {
        assert_eq!(patch.is_banned, Some(true));
        assert_eq!(patch.banned_until, Some(None));
        Ok(make_user(target_id))
    });

    let mut cache = MockCacheService::new();
    cache.expect_set().returning(|_, _, _| Ok(()));
    cache.expect_del_prefix().returning(|_| Ok(()));
    cache.expect_del().returning(|_| Ok(()));

    let uc = build_uc(
        MockCategoryRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        users, cache,
    );
    let result = uc.ban(&actor, target_id, "spam".to_string(), None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn ban_without_perm_returns_403() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(
        MockCategoryRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    let result = uc.ban(&actor, Uuid::new_v4(), "spam".to_string(), None).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

// ─── unban ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn unban_user_not_found_returns_404() {
    let actor = AuthUserBuilder::admin().build();
    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(|_| Ok(None));

    let uc = build_uc(
        MockCategoryRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        users, MockCacheService::new(),
    );
    let result = uc.unban(&actor, Uuid::new_v4()).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn unban_success() {
    let actor = AuthUserBuilder::admin().build();
    let target_id = Uuid::new_v4();
    let target_user = make_user(target_id);

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(target_user.clone())));
    users.expect_update().returning(move |_, _| Ok(make_user(target_id)));

    let mut cache = MockCacheService::new();
    cache.expect_del().returning(|_| Ok(()));

    let uc = build_uc(
        MockCategoryRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        users, cache,
    );
    let result = uc.unban(&actor, target_id).await;
    assert!(result.is_ok());
}
