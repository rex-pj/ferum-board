use ferum_web::view_models::role::{AssignRoleRequest, CreateRoleRequest, SetPermissionsRequest};
use uuid::Uuid;
use validator::Validate;

// ─── CreateRoleRequest ────────────────────────────────────────────────────────

#[test]
fn create_role_minimal_valid() {
    let req: CreateRoleRequest =
        serde_json::from_str(r#"{"name":"Editor","slug":"editor"}"#).unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn create_role_empty_name_fails() {
    let req: CreateRoleRequest =
        serde_json::from_str(r#"{"name":"","slug":"editor"}"#).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn create_role_invalid_slug_fails() {
    let req: CreateRoleRequest =
        serde_json::from_str(r#"{"name":"Editor","slug":"Has Space"}"#).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn create_role_with_color_valid() {
    let req: CreateRoleRequest =
        serde_json::from_str(r##"{"name":"Editor","slug":"editor","color":"#e74c3c"}"##).unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn create_role_invalid_color_fails() {
    let req: CreateRoleRequest =
        serde_json::from_str(r#"{"name":"Editor","slug":"editor","color":"red"}"#).unwrap();
    assert!(req.validate().is_err());
}

// ─── SetPermissionsRequest ────────────────────────────────────────────────────

#[test]
fn set_permissions_empty_list_valid() {
    let req: SetPermissionsRequest =
        serde_json::from_str(r#"{"permission_keys":[]}"#).unwrap();
    assert!(req.permission_keys.is_empty());
}

#[test]
fn set_permissions_with_keys_valid() {
    let req: SetPermissionsRequest =
        serde_json::from_str(r#"{"permission_keys":["thread.create","post.create"]}"#).unwrap();
    assert_eq!(req.permission_keys.len(), 2);
    assert_eq!(req.permission_keys[0], "thread.create");
}

#[test]
fn set_permissions_missing_field_fails() {
    let result: Result<SetPermissionsRequest, _> = serde_json::from_str(r#"{}"#);
    assert!(result.is_err());
}

// ─── AssignRoleRequest ────────────────────────────────────────────────────────

#[test]
fn assign_role_global_no_category() {
    let role_id = Uuid::new_v4();
    let json = format!(r#"{{"role_id":"{role_id}"}}"#);
    let req: AssignRoleRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(req.role_id, role_id);
    assert!(req.category_id.is_none());
}

#[test]
fn assign_role_category_scoped() {
    let role_id = Uuid::new_v4();
    let cat_id = Uuid::new_v4();
    let json = format!(r#"{{"role_id":"{role_id}","category_id":"{cat_id}"}}"#);
    let req: AssignRoleRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(req.category_id, Some(cat_id));
}

#[test]
fn assign_role_missing_role_id_fails() {
    let result: Result<AssignRoleRequest, _> = serde_json::from_str(r#"{}"#);
    assert!(result.is_err());
}

#[test]
fn assign_role_invalid_uuid_fails() {
    let result: Result<AssignRoleRequest, _> =
        serde_json::from_str(r#"{"role_id":"not-a-uuid"}"#);
    assert!(result.is_err());
}
