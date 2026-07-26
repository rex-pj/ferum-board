//! Integration tests for [`PgSiteConfigRepository`].

use std::collections::HashMap;
use ferum_domain::repositories::site_config_repository::SiteConfigRepository;
use ferum_infrastructure::repositories::PgSiteConfigRepository;

use crate::common::TestDb;

#[tokio::test]
async fn get_on_empty_returns_none() {
    // "site_name" is seeded at startup; use a key that is never seeded.
    let db = TestDb::new("sc_get_empty").await;
    let repo = PgSiteConfigRepository::new(db.conn.clone());
    let val = repo.get("nonexistent_key").await.expect("get");
    assert!(val.is_none());
    db.teardown().await;
}

#[tokio::test]
async fn set_and_get_value() {
    let db = TestDb::new("sc_set_get").await;
    let repo = PgSiteConfigRepository::new(db.conn.clone());
    repo.set("site_name", "My Forum").await.expect("set");
    let val = repo.get("site_name").await.expect("get").unwrap();
    assert_eq!(val, "My Forum");
    db.teardown().await;
}

#[tokio::test]
async fn set_overwrites_existing_value() {
    let db = TestDb::new("sc_set_overwrite").await;
    let repo = PgSiteConfigRepository::new(db.conn.clone());
    repo.set("site_name", "Old Name").await.expect("set 1");
    repo.set("site_name", "New Name").await.expect("set 2");
    let val = repo.get("site_name").await.expect("get").unwrap();
    assert_eq!(val, "New Name");
    db.teardown().await;
}

#[tokio::test]
async fn get_all_returns_all_set_keys() {
    let db = TestDb::new("sc_get_all").await;
    let repo = PgSiteConfigRepository::new(db.conn.clone());
    repo.set("site_name", "Forum").await.expect("set name");
    repo.set("registration_open", "true").await.expect("set reg");

    let all = repo.get_all().await.expect("get_all");
    assert_eq!(all.get("site_name").map(|s| s.as_str()), Some("Forum"));
    assert_eq!(all.get("registration_open").map(|s| s.as_str()), Some("true"));
    db.teardown().await;
}

#[tokio::test]
async fn set_many_inserts_multiple_keys_atomically() {
    let db = TestDb::new("sc_set_many").await;
    let repo = PgSiteConfigRepository::new(db.conn.clone());

    let mut entries = HashMap::new();
    entries.insert("site_name".to_string(), "Batch Forum".to_string());
    entries.insert("site_tagline".to_string(), "Discuss everything".to_string());
    repo.set_many(&entries).await.expect("set_many");

    let all = repo.get_all().await.expect("get_all");
    assert_eq!(all.get("site_name").map(|s| s.as_str()), Some("Batch Forum"));
    assert_eq!(all.get("site_tagline").map(|s| s.as_str()), Some("Discuss everything"));
    db.teardown().await;
}
