use std::time::Duration;

use ferum_application::ports::CacheService;
use ferum_infrastructure::cache::InMemoryCacheService;

#[tokio::test]
async fn get_on_empty_cache_returns_none() {
    let c = InMemoryCacheService::new();
    assert!(c.get("missing").await.is_none());
}

#[tokio::test]
async fn set_and_get_roundtrip() {
    let c = InMemoryCacheService::new();
    c.set("k", "hello", Duration::from_secs(60)).await.unwrap();
    assert_eq!(c.get("k").await.unwrap(), "hello");
}

#[tokio::test]
async fn overwrite_existing_key() {
    let c = InMemoryCacheService::new();
    c.set("k", "first", Duration::from_secs(60)).await.unwrap();
    c.set("k", "second", Duration::from_secs(60)).await.unwrap();
    assert_eq!(c.get("k").await.unwrap(), "second");
}

#[tokio::test]
async fn expired_entry_returns_none() {
    let c = InMemoryCacheService::new();
    c.set("k", "v", Duration::from_millis(1)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(c.get("k").await.is_none());
}

#[tokio::test]
async fn del_removes_existing_entry() {
    let c = InMemoryCacheService::new();
    c.set("k", "v", Duration::from_secs(60)).await.unwrap();
    c.del("k").await.unwrap();
    assert!(c.get("k").await.is_none());
}

#[tokio::test]
async fn del_on_missing_key_is_noop() {
    let c = InMemoryCacheService::new();
    assert!(c.del("ghost").await.is_ok());
}

#[tokio::test]
async fn del_prefix_removes_matching_and_leaves_others() {
    let c = InMemoryCacheService::new();
    c.set("user:1", "a", Duration::from_secs(60)).await.unwrap();
    c.set("user:2", "b", Duration::from_secs(60)).await.unwrap();
    c.set("session:1", "c", Duration::from_secs(60)).await.unwrap();
    c.del_prefix("user:").await.unwrap();
    assert!(c.get("user:1").await.is_none());
    assert!(c.get("user:2").await.is_none());
    assert_eq!(c.get("session:1").await.unwrap(), "c");
}

#[tokio::test]
async fn set_nx_on_absent_key_inserts_and_returns_true() {
    let c = InMemoryCacheService::new();
    let inserted = c.set_nx("k", "v", Duration::from_secs(60)).await.unwrap();
    assert!(inserted);
    assert_eq!(c.get("k").await.unwrap(), "v");
}

#[tokio::test]
async fn set_nx_on_live_key_does_not_overwrite_and_returns_false() {
    let c = InMemoryCacheService::new();
    c.set("k", "original", Duration::from_secs(60)).await.unwrap();
    let inserted = c.set_nx("k", "new", Duration::from_secs(60)).await.unwrap();
    assert!(!inserted);
    assert_eq!(c.get("k").await.unwrap(), "original");
}

#[tokio::test]
async fn set_nx_on_expired_key_inserts_and_returns_true() {
    let c = InMemoryCacheService::new();
    c.set("k", "old", Duration::from_millis(1)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    let inserted = c.set_nx("k", "new", Duration::from_secs(60)).await.unwrap();
    assert!(inserted);
    assert_eq!(c.get("k").await.unwrap(), "new");
}

#[tokio::test]
async fn incr_with_ttl_starts_at_one() {
    let c = InMemoryCacheService::new();
    let v = c.incr_with_ttl("counter", Duration::from_secs(60)).await.unwrap();
    assert_eq!(v, 1);
}

#[tokio::test]
async fn incr_with_ttl_increments_existing() {
    let c = InMemoryCacheService::new();
    c.incr_with_ttl("counter", Duration::from_secs(60)).await.unwrap();
    let v = c.incr_with_ttl("counter", Duration::from_secs(60)).await.unwrap();
    assert_eq!(v, 2);
}

#[tokio::test]
async fn exists_returns_true_for_live_key() {
    let c = InMemoryCacheService::new();
    c.set("k", "v", Duration::from_secs(60)).await.unwrap();
    assert!(c.exists("k").await);
}

#[tokio::test]
async fn exists_returns_false_for_missing_key() {
    let c = InMemoryCacheService::new();
    assert!(!c.exists("missing").await);
}
