use ferum_web::handlers::admin::api::categories::AssignModeratorRequest;
use ferum_web::view_models::category::{CreateCategoryRequest, UpdateCategoryRequest};
use uuid::Uuid;
use validator::Validate;

// ─── CreateCategoryRequest ────────────────────────────────────────────────────

#[test]
fn create_category_minimal_valid() {
    let req: CreateCategoryRequest =
        serde_json::from_str(r#"{"name":"General","slug":"general"}"#).unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn create_category_empty_name_fails() {
    let req: CreateCategoryRequest =
        serde_json::from_str(r#"{"name":"","slug":"general"}"#).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn create_category_invalid_slug_fails() {
    let req: CreateCategoryRequest =
        serde_json::from_str(r#"{"name":"General","slug":"UPPER_CASE"}"#).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn create_category_with_all_fields() {
    let parent_id = Uuid::new_v4();
    let json = format!(
        r##"{{"name":"Tech","slug":"tech","description":"Tech talk","parent_id":"{parent_id}","view_policy":"public","post_policy":"members","color":"#3498db"}}"##
    );
    let req: CreateCategoryRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_ok());
    assert_eq!(req.parent_id, Some(parent_id));
}

#[test]
fn create_category_default_policies_are_set() {
    let req: CreateCategoryRequest =
        serde_json::from_str(r#"{"name":"General","slug":"general"}"#).unwrap();
    // view_policy and post_policy default to "public" and "members" via serde default
    assert_eq!(req.view_policy, "public");
    assert_eq!(req.post_policy, "members");
}

// ─── UpdateCategoryRequest ────────────────────────────────────────────────────

#[test]
fn update_category_all_none_allowed() {
    let req: UpdateCategoryRequest = serde_json::from_str(r#"{}"#).unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn update_category_name_too_long_fails() {
    let long_name = "a".repeat(101);
    let json = format!(r#"{{"name":"{long_name}"}}"#);
    let req: UpdateCategoryRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn update_category_slug_invalid_format_fails() {
    let req: UpdateCategoryRequest =
        serde_json::from_str(r#"{"slug":"UPPERCASE"}"#).unwrap();
    assert!(req.validate().is_err());
}

// ─── AssignModeratorRequest ───────────────────────────────────────────────────

#[test]
fn assign_moderator_with_user_id_deserializes() {
    let user_id = Uuid::new_v4();
    let json = format!(r#"{{"user_id":"{user_id}"}}"#);
    let req: AssignModeratorRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(req.user_id, user_id);
}

#[test]
fn assign_moderator_missing_user_id_fails() {
    let result: Result<AssignModeratorRequest, _> = serde_json::from_str(r#"{}"#);
    assert!(result.is_err());
}

#[test]
fn assign_moderator_invalid_uuid_fails() {
    let result: Result<AssignModeratorRequest, _> =
        serde_json::from_str(r#"{"user_id":"not-a-uuid"}"#);
    assert!(result.is_err());
}
