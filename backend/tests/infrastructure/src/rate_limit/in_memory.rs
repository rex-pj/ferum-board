use std::time::Duration;

use ferum_application::ports::{RateLimitResult, RateLimiter};
use ferum_infrastructure::rate_limit::{InMemoryRateLimiter, NullRateLimiter};

#[tokio::test]
async fn first_request_is_allowed_with_correct_remaining() {
    let rl = InMemoryRateLimiter::new();
    let r = rl.check("ip:1.2.3.4", 5, Duration::from_secs(60)).await.unwrap();
    assert!(matches!(r, RateLimitResult::Allowed { remaining: 4 }));
}

#[tokio::test]
async fn request_at_exact_limit_is_allowed_with_zero_remaining() {
    let rl = InMemoryRateLimiter::new();
    for _ in 0..4 {
        rl.check("key", 5, Duration::from_secs(60)).await.unwrap();
    }
    let r = rl.check("key", 5, Duration::from_secs(60)).await.unwrap();
    assert!(matches!(r, RateLimitResult::Allowed { remaining: 0 }));
}

#[tokio::test]
async fn request_over_limit_is_denied() {
    let rl = InMemoryRateLimiter::new();
    for _ in 0..5 {
        rl.check("key", 5, Duration::from_secs(60)).await.unwrap();
    }
    let r = rl.check("key", 5, Duration::from_secs(60)).await.unwrap();
    assert!(matches!(r, RateLimitResult::Denied { .. }));
}

#[tokio::test]
async fn after_window_expires_counter_resets_and_allows() {
    let rl = InMemoryRateLimiter::new();
    for _ in 0..5 {
        rl.check("key", 5, Duration::from_millis(10)).await.unwrap();
    }
    assert!(matches!(
        rl.check("key", 5, Duration::from_millis(10)).await.unwrap(),
        RateLimitResult::Denied { .. }
    ));
    tokio::time::sleep(Duration::from_millis(30)).await;
    let r = rl.check("key", 5, Duration::from_millis(10)).await.unwrap();
    assert!(matches!(r, RateLimitResult::Allowed { .. }));
}

#[tokio::test]
async fn different_keys_are_independent() {
    let rl = InMemoryRateLimiter::new();
    for _ in 0..5 {
        rl.check("key_a", 5, Duration::from_secs(60)).await.unwrap();
    }
    let r = rl.check("key_b", 5, Duration::from_secs(60)).await.unwrap();
    assert!(matches!(r, RateLimitResult::Allowed { .. }));
}

#[tokio::test]
async fn null_rate_limiter_always_allows_with_full_remaining() {
    let rl = NullRateLimiter;
    for _ in 0..100 {
        let r = rl.check("key", 5, Duration::from_secs(60)).await.unwrap();
        assert!(matches!(r, RateLimitResult::Allowed { remaining: 5 }));
    }
}
