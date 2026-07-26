//! Integration tests for [`PgThemeRepository`].

use uuid::Uuid;
use ferum_domain::models::theme::NewTheme;
use ferum_domain::repositories::theme_repository::ThemeRepository;
use ferum_infrastructure::repositories::PgThemeRepository;

use crate::common::TestDb;

fn new_theme(slug: &str) -> NewTheme {
    NewTheme {
        id: Uuid::new_v4(),
        slug: slug.to_string(),
        name: format!("{slug} theme"),
        author: Some("test".to_string()),
        version: "1.0.0".to_string(),
        description: None,
        parent_slug: "default".to_string(),
    }
}

#[tokio::test]
async fn list_returns_seeded_default_theme() {
    // Migrations insert the "default" system theme; the list must not be empty.
    let db = TestDb::new("theme_list_empty").await;
    let repo = PgThemeRepository::new(db.conn.clone());
    let themes = repo.list().await.expect("list");
    assert_eq!(themes.len(), 1);
    assert_eq!(themes[0].slug, "default");
    db.teardown().await;
}

#[tokio::test]
async fn upsert_and_find_by_slug() {
    let db = TestDb::new("theme_upsert_find").await;
    let repo = PgThemeRepository::new(db.conn.clone());
    let theme = repo.upsert(new_theme("ferum-dark")).await.expect("upsert");

    let found = repo.find_by_slug("ferum-dark").await.expect("find_by_slug").unwrap();
    assert_eq!(found.id, theme.id);
    assert_eq!(found.name, "ferum-dark theme");
    db.teardown().await;
}

#[tokio::test]
async fn upsert_idempotent_on_same_slug() {
    // The system seeder writes a "default" theme; track initial count so the assertion
    // is independent of how many system themes the migration adds.
    let db = TestDb::new("theme_upsert_idempotent").await;
    let repo = PgThemeRepository::new(db.conn.clone());
    let initial = repo.list().await.expect("initial list").len();

    let t = new_theme("my-theme");
    let id = t.id;
    repo.upsert(t).await.expect("first upsert");

    let mut t2 = new_theme("my-theme");
    t2.id = id; // same id → must update, not duplicate
    t2.version = "2.0.0".to_string();
    repo.upsert(t2).await.expect("second upsert");

    let list = repo.list().await.expect("list");
    assert_eq!(list.len(), initial + 1, "second upsert must not add a duplicate row");
    db.teardown().await;
}

#[tokio::test]
async fn set_active_marks_one_theme_active() {
    let db = TestDb::new("theme_set_active").await;
    let repo = PgThemeRepository::new(db.conn.clone());
    let t1 = repo.upsert(new_theme("t1")).await.expect("upsert t1");
    repo.upsert(new_theme("t2")).await.expect("upsert t2");

    repo.set_active("t1").await.expect("set_active");

    // get_active must return t1 (it's set to active)
    let active = repo.get_active().await.expect("get_active");
    assert_eq!(active.id, t1.id);
    db.teardown().await;
}

#[tokio::test]
async fn set_active_switches_from_previous_active() {
    let db = TestDb::new("theme_set_active_switch").await;
    let repo = PgThemeRepository::new(db.conn.clone());
    repo.upsert(new_theme("t1")).await.expect("upsert t1");
    let t2 = repo.upsert(new_theme("t2")).await.expect("upsert t2");

    repo.set_active("t1").await.expect("activate t1");
    repo.set_active("t2").await.expect("activate t2");

    let active = repo.get_active().await.expect("get_active");
    assert_eq!(active.id, t2.id);
    db.teardown().await;
}

#[tokio::test]
async fn delete_removes_theme() {
    let db = TestDb::new("theme_delete").await;
    let repo = PgThemeRepository::new(db.conn.clone());
    repo.upsert(new_theme("temp")).await.expect("upsert");
    repo.delete("temp").await.expect("delete");
    assert!(repo.find_by_slug("temp").await.expect("find").is_none());
    db.teardown().await;
}

#[tokio::test]
async fn update_preview_url_stores_url() {
    let db = TestDb::new("theme_preview_url").await;
    let repo = PgThemeRepository::new(db.conn.clone());
    repo.upsert(new_theme("preview-test")).await.expect("upsert");
    repo.update_preview_url("preview-test", Some("https://cdn/preview.png".to_string()))
        .await.expect("update_preview_url");

    let found = repo.find_by_slug("preview-test").await.expect("find").unwrap();
    assert_eq!(found.preview_url.as_deref(), Some("https://cdn/preview.png"));
    db.teardown().await;
}
