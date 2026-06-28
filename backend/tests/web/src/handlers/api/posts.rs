use ferum_web::handlers::api::posts::PreviewMarkdownRequest;
use ferum_web::view_models::post::{CreatePostRequest, PostListQuery, UpdatePostRequest};
use validator::Validate;

// ─── PreviewMarkdownRequest ───────────────────────────────────────────────────

#[test]
fn preview_markdown_with_content_deserializes() {
    let req: PreviewMarkdownRequest =
        serde_json::from_str(r#"{"content":"**bold** text"}"#).unwrap();
    assert_eq!(req.content, "**bold** text");
}

#[test]
fn preview_markdown_empty_content_deserializes() {
    let req: PreviewMarkdownRequest =
        serde_json::from_str(r#"{"content":""}"#).unwrap();
    assert_eq!(req.content, "");
}

#[test]
fn preview_markdown_missing_content_fails() {
    let result: Result<PreviewMarkdownRequest, _> = serde_json::from_str(r#"{}"#);
    assert!(result.is_err());
}

// ─── CreatePostRequest ────────────────────────────────────────────────────────

#[test]
fn create_post_minimal_valid() {
    let req: CreatePostRequest =
        serde_json::from_str(r#"{"content_md":"Hello world"}"#).unwrap();
    assert!(req.validate().is_ok());
    assert!(req.parent_id.is_none());
}

#[test]
fn create_post_empty_content_fails() {
    let req: CreatePostRequest = serde_json::from_str(r#"{"content_md":""}"#).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn create_post_content_too_long_fails() {
    let long_content = "a".repeat(50_001);
    let json = format!(r#"{{"content_md":"{long_content}"}}"#);
    let req: CreatePostRequest = serde_json::from_str(&json).unwrap();
    assert!(req.validate().is_err());
}

#[test]
fn create_post_missing_content_fails() {
    let result: Result<CreatePostRequest, _> = serde_json::from_str(r#"{}"#);
    assert!(result.is_err());
}

// ─── UpdatePostRequest ────────────────────────────────────────────────────────

#[test]
fn update_post_valid_content() {
    let req: UpdatePostRequest =
        serde_json::from_str(r#"{"content_md":"Updated content here"}"#).unwrap();
    assert!(req.validate().is_ok());
}

#[test]
fn update_post_empty_content_fails() {
    let req: UpdatePostRequest = serde_json::from_str(r#"{"content_md":""}"#).unwrap();
    assert!(req.validate().is_err());
}

// ─── PostListQuery ────────────────────────────────────────────────────────────

#[test]
fn post_list_query_all_optional() {
    let q: PostListQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.page.is_none());
    assert!(q.per_page.is_none());
}

#[test]
fn post_list_query_with_pagination() {
    let q: PostListQuery = serde_json::from_str(r#"{"page":3,"per_page":50}"#).unwrap();
    assert_eq!(q.page, Some(3));
    assert_eq!(q.per_page, Some(50));
}
