use std::sync::Arc;

use uuid::Uuid;
use ferum_application::usecases::admin_usecase::{
    AdminUseCase, CreateCategoryCmd, UpdateCategoryCmd,
};
use ferum_domain::AppError;
use ferum_domain::models::category::{PostPolicy, ViewPolicy};
use ferum_test_support::fixtures::{make_assignment, make_category, make_role, make_user, AuthUserBuilder};
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

// ─── update_category: the parent rules `create` enforces ────────────────────
//
// These were absent, so every rule `create_category` applies could be broken by
// editing the category afterwards. Nothing walks the tree recursively, so a cycle
// does not hang — it makes a category vanish from the admin page instead, since it
// is then neither a root nor any root's child.

fn update_parent_to(parent: Option<Uuid>) -> UpdateCategoryCmd {
    UpdateCategoryCmd {
        name: None,
        slug: None,
        description: None,
        parent_id: Some(parent),
        position: None,
        view_policy: None,
        post_policy: None,
        color: None,
    }
}

#[tokio::test]
async fn update_category_cannot_make_a_category_its_own_parent() {
    let actor = AuthUserBuilder::admin().build();
    let cat_id = Uuid::new_v4();

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_id()
        .returning(move |_| Ok(Some(make_category(cat_id))));
    cats.expect_update().never();

    let uc = build_uc(
        cats, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    let result = uc.update_category(&actor, cat_id, update_parent_to(Some(cat_id))).await;
    assert!(
        matches!(&result, Err(AppError::Invalid { code, .. }) if code == "category_cannot_be_its_own_parent"),
        "got {result:?}"
    );
}

#[tokio::test]
async fn update_category_cannot_nest_under_a_child() {
    // Also the two-node cycle: for B to point back at A, B must be A's child, and
    // a category that already has a parent cannot become one.
    let actor = AuthUserBuilder::admin().build();
    let cat_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();
    let mut child = make_category(child_id);
    child.parent_id = Some(cat_id);

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_id().returning(move |q| {
        Ok(Some(if q == child_id { child.clone() } else { make_category(cat_id) }))
    });
    cats.expect_update().never();

    let uc = build_uc(
        cats, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    let result = uc.update_category(&actor, cat_id, update_parent_to(Some(child_id))).await;
    assert!(
        matches!(&result, Err(AppError::Invalid { code, .. }) if code == "category_nesting_too_deep"),
        "got {result:?}"
    );
}

#[tokio::test]
async fn update_category_cannot_move_a_parent_under_another_root() {
    // The check `create` does not need: this category already has children, so
    // giving it a parent would build a three-level chain.
    let actor = AuthUserBuilder::admin().build();
    let cat_id = Uuid::new_v4();
    let other_root = Uuid::new_v4();

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_id()
        .returning(move |q| Ok(Some(make_category(q))));
    cats.expect_has_children().returning(|_| Ok(true));
    cats.expect_update().never();

    let uc = build_uc(
        cats, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    let result = uc.update_category(&actor, cat_id, update_parent_to(Some(other_root))).await;
    assert!(
        matches!(&result, Err(AppError::Invalid { code, .. }) if code == "category_nesting_too_deep"),
        "got {result:?}"
    );
}

#[tokio::test]
async fn update_category_can_still_clear_its_parent() {
    // `Some(None)` detaches and must never be blocked — otherwise a category that
    // fell foul of the rules above could not be repaired through the UI.
    let actor = AuthUserBuilder::admin().build();
    let cat_id = Uuid::new_v4();

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_id()
        .returning(move |q| Ok(Some(make_category(q))));
    cats.expect_update()
        .returning(move |_, _| Ok(make_category(cat_id)));

    let uc = build_uc(
        cats, MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    assert!(uc.update_category(&actor, cat_id, update_parent_to(None)).await.is_ok());
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

// ─── ban (permanent: banned_until = None) ──────────────────────────────────

#[tokio::test]
async fn permanent_ban_without_perm_returns_403() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(
        MockCategoryRepository::new(), MockRoleRepository::new(), MockUserRoleRepository::new(),
        MockUserRepository::new(), MockCacheService::new(),
    );
    let result = uc.ban(&actor, Uuid::new_v4(), "spam".to_string(), None).await;
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
    let result = uc.ban(&actor, Uuid::new_v4(), "spam".to_string(), None).await;
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
    let result = uc.ban(&actor, target_id, "spam".to_string(), None).await;
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

// ─── assign_moderator: permission cache ───────────────────────────────────────

/// Effective permissions are resolved from `user:roles:{id}`, cached for five
/// minutes. `revoke_moderator` drops that key so the revocation takes effect on
/// the next request; `assign_moderator` did not, so a freshly appointed
/// moderator held no moderation permission in their new category until the
/// entry happened to expire — up to five minutes of an admin action that
/// visibly succeeded and did nothing.
#[tokio::test]
async fn assign_moderator_invalidates_the_target_permission_cache() {
    let actor = AuthUserBuilder::member().with_perm("admin.users").build();
    let target = Uuid::new_v4();
    let cat_id = Uuid::new_v4();
    let role_id = Uuid::new_v4();

    let mut categories = MockCategoryRepository::new();
    categories
        .expect_find_by_id()
        .returning(move |_| Ok(Some(make_category(cat_id))));

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(make_user(target))));

    let mut roles = MockRoleRepository::new();
    roles
        .expect_find_by_slug()
        .returning(move |_| Ok(Some(make_role(role_id, "moderator"))));

    let mut user_roles = MockUserRoleRepository::new();
    user_roles
        .expect_assign()
        .returning(move |_, _, _, _, _| Ok(make_assignment(target, role_id)));

    let mut cache = MockCacheService::new();
    cache
        .expect_del()
        .withf(move |key| key == format!("user:roles:{target}"))
        .times(1)
        .returning(|_| Ok(()));

    let uc = build_uc(categories, roles, user_roles, users, cache);
    uc.assign_moderator(&actor, cat_id, target).await.unwrap();
}

// ─── verify_user_email: trust level ───────────────────────────────────────────

/// Verifying an email is what lifts a user from `New` to `Basic`, and `Basic` is
/// the floor for posting. Both self-service paths (`verify_email` via the
/// emailed token, and auto-verify at registration) set it; the admin button did
/// not, so an admin could "verify" an account and the user would still be told
/// `trust_level_insufficient` on their first post, with nothing explaining why.
#[tokio::test]
async fn admin_email_verification_also_promotes_to_basic() {
    let actor = AuthUserBuilder::member().with_perm("admin.users").build();
    let target = Uuid::new_v4();

    // `make_user` is verified by default; an admin only ever presses this button
    // on an account that is not.
    let mut unverified = make_user(target);
    unverified.is_email_verified = false;
    unverified.trust_level = ferum_domain::models::user::TrustLevel::New;

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(unverified.clone())));
    users.expect_set_email_verified().times(1).returning(|_| Ok(()));
    users
        .expect_set_trust_level()
        .withf(|_, level| *level == ferum_domain::models::user::TrustLevel::Basic)
        .times(1)
        .returning(|_, _| Ok(()));

    let uc = build_uc(
        MockCategoryRepository::new(), MockRoleRepository::new(),
        MockUserRoleRepository::new(), users, MockCacheService::new(),
    );
    uc.verify_user_email(&actor, target).await.unwrap();
}

/// Only a promotion, never a demotion: re-verifying a Regular's address must not
/// knock them back down to Basic.
#[tokio::test]
async fn admin_email_verification_does_not_demote_an_established_user() {
    let actor = AuthUserBuilder::member().with_perm("admin.users").build();
    let target = Uuid::new_v4();
    let mut established = make_user(target);
    established.is_email_verified = true;
    established.trust_level = ferum_domain::models::user::TrustLevel::Regular;

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(established.clone())));
    users.expect_set_email_verified().returning(|_| Ok(()));
    users.expect_set_trust_level().never();

    let uc = build_uc(
        MockCategoryRepository::new(), MockRoleRepository::new(),
        MockUserRoleRepository::new(), users, MockCacheService::new(),
    );
    uc.verify_user_email(&actor, target).await.unwrap();
}
