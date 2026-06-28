use ferum_web::handlers::pages::auth::{LoginPageQuery, TokenQuery};

// ─── LoginPageQuery ───────────────────────────────────────────────────────────

#[test]
fn login_page_query_no_error_is_none() {
    let q: LoginPageQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.error.is_none());
}

#[test]
fn login_page_query_derives_default() {
    let q = LoginPageQuery::default();
    assert!(q.error.is_none());
}

#[test]
fn login_page_query_with_error_param() {
    let q: LoginPageQuery =
        serde_json::from_str(r#"{"error":"invalid_credentials"}"#).unwrap();
    assert_eq!(q.error.as_deref(), Some("invalid_credentials"));
}

// ─── TokenQuery ───────────────────────────────────────────────────────────────

#[test]
fn token_query_with_token_deserializes() {
    let q: TokenQuery = serde_json::from_str(r#"{"token":"abc123"}"#).unwrap();
    assert_eq!(q.token.as_deref(), Some("abc123"));
}

#[test]
fn token_query_missing_token_is_none() {
    let q: TokenQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.token.is_none());
}
