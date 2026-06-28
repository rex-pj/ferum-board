use ferum_application::ports::PasswordHasher;
use ferum_infrastructure::bcrypt_password_hasher::BcryptPasswordHasher;

// hash() uses BCRYPT_COST 12 in production (~300 ms) — this full-cycle test is
// intentionally slow to verify the real hash→verify path works end-to-end.
#[tokio::test]
async fn hash_produces_valid_bcrypt_string_that_verifies() {
    let hasher = BcryptPasswordHasher;
    let hash = hasher.hash("secret_password").await.unwrap();
    assert!(
        hash.starts_with("$2b$") || hash.starts_with("$2a$"),
        "expected a bcrypt hash, got: {hash}"
    );
    assert!(bcrypt::verify("secret_password", &hash).unwrap());
}

// Use cost 4 directly for fast verify-only tests; bcrypt embeds cost in hash string.
#[tokio::test]
async fn verify_correct_password_returns_true() {
    let hash = bcrypt::hash("hunter2", 4).unwrap();
    let hasher = BcryptPasswordHasher;
    assert!(hasher.verify("hunter2", &hash).await.unwrap());
}

#[tokio::test]
async fn verify_wrong_password_returns_false() {
    let hash = bcrypt::hash("correct_horse", 4).unwrap();
    let hasher = BcryptPasswordHasher;
    assert!(!hasher.verify("battery_staple", &hash).await.unwrap());
}
