use std::sync::Arc;
use std::collections::HashMap;

use uuid::Uuid;
use ferum_application::usecases::role_usecase::{AssignRoleCmd, CreateRoleCmd, RoleUseCase};
use ferum_domain::AppError;
use ferum_test_support::fixtures::{make_assignment, make_role, AuthUserBuilder};
use ferum_test_support::mocks::{
    permission_repository::MockPermissionRepository,
    role_repository::MockRoleRepository,
    user_role_repository::MockUserRoleRepository,
};

fn build_uc(
    roles: MockRoleRepository,
    perms: MockPermissionRepository,
    user_roles: MockUserRoleRepository,
) -> RoleUseCase {
    RoleUseCase::new(Arc::new(roles), Arc::new(perms), Arc::new(user_roles))
}

// ─── create_role ───────────────────────────────────────────────────────────

#[tokio::test]
async fn create_role_without_perm_returns_403() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(MockRoleRepository::new(), MockPermissionRepository::new(), MockUserRoleRepository::new());
    let result = uc.create_role(&actor, CreateRoleCmd {
        slug: "new-role".to_string(), name: "New Role".to_string(),
        description: None, color: None, position: 0,
    }).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn create_role_with_perm_succeeds() {
    let role_id = Uuid::new_v4();
    let new_role = make_role(role_id, "custom-role");

    let mut roles = MockRoleRepository::new();
    roles.expect_create().returning(move |_| Ok(new_role.clone()));

    let actor = AuthUserBuilder::admin().build();
    let uc = build_uc(roles, MockPermissionRepository::new(), MockUserRoleRepository::new());
    let result = uc.create_role(&actor, CreateRoleCmd {
        slug: "custom-role".to_string(), name: "Custom Role".to_string(),
        description: None, color: None, position: 0,
    }).await;
    assert!(result.is_ok());
}

// ─── delete_role ───────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_role_without_perm_returns_403() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(MockRoleRepository::new(), MockPermissionRepository::new(), MockUserRoleRepository::new());
    assert!(matches!(
        uc.delete_role(&actor, Uuid::new_v4()).await,
        Err(AppError::Forbidden(_))
    ));
}

#[tokio::test]
async fn delete_role_delegates_system_guard_to_repository() {
    // Repository returns Forbidden for system roles — usecase passes it through
    let mut roles = MockRoleRepository::new();
    roles.expect_delete()
        .returning(|_| Err(AppError::forbidden("cannot_delete_system_role")));

    let actor = AuthUserBuilder::admin().build();
    let uc = build_uc(roles, MockPermissionRepository::new(), MockUserRoleRepository::new());
    let result = uc.delete_role(&actor, Uuid::new_v4()).await;
    assert!(matches!(result, Err(AppError::Forbidden(c)) if c == "cannot_delete_system_role"));
}

// ─── set_role_permissions ──────────────────────────────────────────────────

#[tokio::test]
async fn set_role_permissions_without_perm_returns_403() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(MockRoleRepository::new(), MockPermissionRepository::new(), MockUserRoleRepository::new());
    let result = uc.set_role_permissions(&actor, Uuid::new_v4(), vec![]).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn set_role_permissions_role_not_found_returns_404() {
    let actor = AuthUserBuilder::admin().build();
    let mut roles = MockRoleRepository::new();
    roles.expect_find_by_id().returning(|_| Ok(None));

    let uc = build_uc(roles, MockPermissionRepository::new(), MockUserRoleRepository::new());
    let result = uc.set_role_permissions(&actor, Uuid::new_v4(), vec![]).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn set_role_permissions_success() {
    let role_id = Uuid::new_v4();
    let role = make_role(role_id, "custom");

    let mut roles = MockRoleRepository::new();
    roles.expect_find_by_id().returning(move |_| Ok(Some(role.clone())));

    let mut perms = MockPermissionRepository::new();
    perms.expect_set_role_permissions().returning(|_, _| Ok(()));

    let actor = AuthUserBuilder::admin().build();
    let uc = build_uc(roles, perms, MockUserRoleRepository::new());
    let result = uc.set_role_permissions(&actor, role_id, vec!["thread.create".to_string()]).await;
    assert!(result.is_ok());
}

// ─── assign_role ───────────────────────────────────────────────────────────

#[tokio::test]
async fn assign_role_without_perm_returns_403() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(MockRoleRepository::new(), MockPermissionRepository::new(), MockUserRoleRepository::new());
    let result = uc.assign_role(&actor, AssignRoleCmd {
        user_id: Uuid::new_v4(), role_id: Uuid::new_v4(),
        category_id: None, expires_at: None,
    }).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn assign_role_not_found_returns_404() {
    let actor = AuthUserBuilder::admin().build();
    let mut roles = MockRoleRepository::new();
    roles.expect_find_by_id().returning(|_| Ok(None));

    let uc = build_uc(roles, MockPermissionRepository::new(), MockUserRoleRepository::new());
    let result = uc.assign_role(&actor, AssignRoleCmd {
        user_id: Uuid::new_v4(), role_id: Uuid::new_v4(),
        category_id: None, expires_at: None,
    }).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn assign_role_success() {
    let actor_id = Uuid::new_v4();
    let actor = AuthUserBuilder::admin().with_id(actor_id).build();
    let user_id = Uuid::new_v4();
    let role_id = Uuid::new_v4();
    let role = make_role(role_id, "member");
    let assignment = make_assignment(user_id, role_id);

    let mut roles = MockRoleRepository::new();
    roles.expect_find_by_id().returning(move |_| Ok(Some(role.clone())));

    let mut user_roles = MockUserRoleRepository::new();
    user_roles.expect_assign().returning(move |_, _, _, _, _| Ok(assignment.clone()));

    let uc = build_uc(roles, MockPermissionRepository::new(), user_roles);
    let result = uc.assign_role(&actor, AssignRoleCmd {
        user_id, role_id, category_id: None, expires_at: None,
    }).await;
    assert!(result.is_ok());
}

// ─── revoke_role ───────────────────────────────────────────────────────────

#[tokio::test]
async fn revoke_role_without_perm_returns_403() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(MockRoleRepository::new(), MockPermissionRepository::new(), MockUserRoleRepository::new());
    let result = uc.revoke_role(&actor, Uuid::new_v4(), Uuid::new_v4(), None).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn revoke_role_last_admin_returns_403() {
    let actor = AuthUserBuilder::admin().build();
    let role_id = Uuid::new_v4();
    let mut admin_role = make_role(role_id, "admin");
    admin_role.slug = "admin".to_string();

    let mut roles = MockRoleRepository::new();
    roles.expect_find_by_id().returning(move |_| Ok(Some(admin_role.clone())));

    let mut user_roles = MockUserRoleRepository::new();
    user_roles.expect_count_global_by_role().returning(move || {
        let mut m = HashMap::new();
        m.insert(role_id, 1u64); // only 1 global admin
        Ok(m)
    });

    let uc = build_uc(roles, MockPermissionRepository::new(), user_roles);
    let result = uc.revoke_role(&actor, Uuid::new_v4(), role_id, None).await;
    assert!(matches!(result, Err(AppError::Forbidden(c)) if c == "cannot_remove_last_admin"));
}

#[tokio::test]
async fn revoke_role_non_admin_role_succeeds() {
    let actor = AuthUserBuilder::admin().build();
    let role_id = Uuid::new_v4();
    let member_role = make_role(role_id, "member");

    let mut roles = MockRoleRepository::new();
    roles.expect_find_by_id().returning(move |_| Ok(Some(member_role.clone())));

    let mut user_roles = MockUserRoleRepository::new();
    user_roles.expect_revoke().returning(|_, _, _| Ok(()));

    let uc = build_uc(roles, MockPermissionRepository::new(), user_roles);
    let result = uc.revoke_role(&actor, Uuid::new_v4(), role_id, None).await;
    assert!(result.is_ok());
}
