use ferum_web::handlers::admin::api::users::{
    BanUserRequest, EditUserRequest, SetTrustLevelRequest, UserListQuery,
};

// ─── UserListQuery ────────────────────────────────────────────────────────────

#[test]
fn user_list_query_all_optional() {
    let q: UserListQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.page.is_none());
    assert!(q.per_page.is_none());
    assert!(q.q.is_none());
}

#[test]
fn user_list_query_with_search_and_pagination() {
    let q: UserListQuery =
        serde_json::from_str(r#"{"page":2,"per_page":50,"q":"alice"}"#).unwrap();
    assert_eq!(q.page, Some(2));
    assert_eq!(q.per_page, Some(50));
    assert_eq!(q.q.as_deref(), Some("alice"));
}

// ─── BanUserRequest ───────────────────────────────────────────────────────────

#[test]
fn ban_user_with_reason_deserializes() {
    let req: BanUserRequest =
        serde_json::from_str(r#"{"reason":"Repeated violations of community guidelines"}"#)
            .unwrap();
    assert!(!req.reason.is_empty());
}

#[test]
fn ban_user_missing_reason_fails() {
    let result: Result<BanUserRequest, _> = serde_json::from_str(r#"{}"#);
    assert!(result.is_err());
}

// ─── EditUserRequest ──────────────────────────────────────────────────────────

#[test]
fn edit_user_all_none_deserializes() {
    let req: EditUserRequest = serde_json::from_str(r#"{}"#).unwrap();
    assert!(req.display_name.is_none());
    assert!(req.bio.is_none());
    assert!(req.website.is_none());
}

#[test]
fn edit_user_with_display_name_deserializes() {
    let req: EditUserRequest =
        serde_json::from_str(r#"{"display_name":"Bob"}"#).unwrap();
    assert_eq!(req.display_name.as_deref(), Some("Bob"));
}

// ─── SetTrustLevelRequest ────────────────────────────────────────────────────

#[test]
fn set_trust_level_deserializes() {
    let req: SetTrustLevelRequest =
        serde_json::from_str(r#"{"trust_level":"member"}"#).unwrap();
    assert_eq!(req.trust_level, "member");
}

#[test]
fn set_trust_level_all_levels_deserialize() {
    for level in &["new", "basic", "member", "regular", "leader"] {
        let json = format!(r#"{{"trust_level":"{level}"}}"#);
        let req: SetTrustLevelRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(&req.trust_level, level);
    }
}
