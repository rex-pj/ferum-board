use chrono::{Duration, Utc};
use uuid::Uuid;

use ferum_application::ports::{AccessTokenClaims, TokenService};
use ferum_infrastructure::jwt_token_service::JwtTokenService;

fn svc() -> JwtTokenService {
    JwtTokenService::new("test-secret-key-for-unit-tests-32bytes!", 3600, 604_800)
}

fn make_claims(user_id: Uuid) -> AccessTokenClaims {
    AccessTokenClaims {
        sub: user_id,
        username: "testuser".to_string(),
        display_name: Some("Test User".to_string()),
        avatar_url: None,
        trust_level: "basic".to_string(),
        is_banned: false,
        banned_until: None,
        exp: (Utc::now() + Duration::hours(1)).timestamp(),
        iat: Utc::now().timestamp(),
    }
}

#[test]
fn access_token_roundtrip_preserves_claims() {
    let svc = svc();
    let user_id = Uuid::new_v4();
    let token = svc.mint_access_token(&make_claims(user_id)).unwrap();
    let decoded = svc.verify_access_token(&token).unwrap();
    assert_eq!(decoded.sub, user_id);
    assert_eq!(decoded.username, "testuser");
    assert_eq!(decoded.trust_level, "basic");
    assert!(!decoded.is_banned);
}

#[test]
fn access_token_wrong_secret_rejected() {
    let svc = svc();
    let token = svc.mint_access_token(&make_claims(Uuid::new_v4())).unwrap();
    let other = JwtTokenService::new("completely-different-secret-key!!", 3600, 604_800);
    assert!(other.verify_access_token(&token).is_err());
}

#[test]
fn access_token_tampered_signature_rejected() {
    let svc = svc();
    let token = svc.mint_access_token(&make_claims(Uuid::new_v4())).unwrap();
    let parts: Vec<&str> = token.splitn(3, '.').collect();
    let tampered = format!("{}.{}.invalidsignatureXXX", parts[0], parts[1]);
    assert!(svc.verify_access_token(&tampered).is_err());
}

#[test]
fn expired_access_token_rejected() {
    let svc = svc();
    let user_id = Uuid::new_v4();
    let mut claims = make_claims(user_id);
    claims.exp = (Utc::now() - Duration::hours(2)).timestamp();
    let token = svc.mint_access_token(&claims).unwrap();
    assert!(svc.verify_access_token(&token).is_err());
}

#[test]
fn refresh_token_roundtrip_preserves_user_id() {
    let svc = svc();
    let user_id = Uuid::new_v4();
    let token = svc.mint_refresh_token(user_id).unwrap();
    let decoded = svc.verify_refresh_token(&token).unwrap();
    assert_eq!(decoded, user_id);
}

#[test]
fn refresh_token_wrong_secret_rejected() {
    let svc = svc();
    let token = svc.mint_refresh_token(Uuid::new_v4()).unwrap();
    let other = JwtTokenService::new("completely-different-secret-key!!", 3600, 604_800);
    assert!(other.verify_refresh_token(&token).is_err());
}

#[test]
fn access_token_cannot_be_used_as_refresh_token() {
    let svc = svc();
    let access = svc.mint_access_token(&make_claims(Uuid::new_v4())).unwrap();
    assert!(svc.verify_refresh_token(&access).is_err());
}

#[test]
fn email_token_roundtrip_with_correct_purpose() {
    let svc = svc();
    let user_id = Uuid::new_v4();
    let token = svc.mint_email_token(user_id, "email_verify").unwrap();
    let decoded = svc.verify_email_token(&token, "email_verify").unwrap();
    assert_eq!(decoded, user_id);
}

#[test]
fn email_token_wrong_purpose_rejected() {
    let svc = svc();
    let user_id = Uuid::new_v4();
    let token = svc.mint_email_token(user_id, "email_verify").unwrap();
    assert!(svc.verify_email_token(&token, "password_reset").is_err());
}

#[test]
fn refresh_token_cannot_be_used_as_email_token() {
    let svc = svc();
    let refresh = svc.mint_refresh_token(Uuid::new_v4()).unwrap();
    assert!(svc.verify_email_token(&refresh, "email_verify").is_err());
}
