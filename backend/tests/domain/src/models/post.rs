//! `Post::readable_content` — the one rule for what a *reader* is allowed to
//! see of a post's body.
//!
//! This existed only as an inline `if is_deleted { String::new() }` inside the
//! SSR thread handler. The JSON API had no equivalent, so `PostResponse` copied
//! `content_md`/`content_html` straight out of the row and served the full text
//! of every soft-deleted post to anyone who asked. Both sides now call this.

// Built inline rather than via `ferum-test-support`: this crate depends on
// `ferum-domain` alone, with no features, so that the pure domain is proven to
// compile without the axum or sea-orm glue. See its Cargo.toml.
use ferum_domain::models::post::{Post, PostStatus};
use uuid::Uuid;

fn make_post() -> Post {
    Post {
        id: Uuid::nil(),
        thread_id: Uuid::nil(),
        author_id: Uuid::nil(),
        parent_id: None,
        content_md: "Hello world".to_string(),
        content_html: "<p>Hello world</p>".to_string(),
        status: PostStatus::Published,
        is_deleted: false,
        deleted_at: None,
        deleted_by_id: None,
        edited_at: None,
        edited_by_id: None,
        edit_count: 0,
        created_at: chrono::Utc::now(),
        author_username: None,
        author_display_name: None,
        author_avatar_url: None,
        author_role: None,
        reactions: vec![],
        my_reactions: vec![],
        thread_slug: None,
        thread_title: None,
    }
}

fn deleted_post() -> Post {
    Post { is_deleted: true, ..make_post() }
}

#[test]
fn a_live_post_reads_back_its_own_content() {
    let post = make_post();
    let (md, html) = post.readable_content();
    assert_eq!(md, "Hello world");
    assert_eq!(html, "<p>Hello world</p>");
}

/// The row is still returned — the thread page renders a tombstone in place, and
/// dropping it would renumber every later post. What must not survive is the
/// text.
#[test]
fn a_deleted_post_reads_back_nothing() {
    let post = deleted_post();
    let (md, html) = post.readable_content();
    assert!(md.is_empty(), "deleted markdown must not reach a reader");
    assert!(html.is_empty(), "deleted HTML must not reach a reader");
}

/// The stored row keeps its content regardless: posts are soft-deleted so a
/// moderator can still review what was removed, and `sync_attachment_refs`
/// reads `content_md` back on delete to release the attachments it referenced.
/// Redaction is a read-time rule, not a write-time one.
#[test]
fn redaction_does_not_touch_the_stored_row() {
    let post = deleted_post();
    let _ = post.readable_content();
    assert_eq!(post.content_md, "Hello world");
    assert_eq!(post.content_html, "<p>Hello world</p>");
}
