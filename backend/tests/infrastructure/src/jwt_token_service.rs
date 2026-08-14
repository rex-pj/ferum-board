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
    let token = svc.mint_email_token(user_id, "email_verify", 3600).unwrap();
    let decoded = svc.verify_email_token(&token, "email_verify").unwrap();
    assert_eq!(decoded, user_id);
}

#[test]
fn email_token_wrong_purpose_rejected() {
    let svc = svc();
    let user_id = Uuid::new_v4();
    let token = svc.mint_email_token(user_id, "email_verify", 3600).unwrap();
    assert!(svc.verify_email_token(&token, "password_reset").is_err());
}

#[test]
fn refresh_token_cannot_be_used_as_email_token() {
    let svc = svc();
    let refresh = svc.mint_refresh_token(Uuid::new_v4()).unwrap();
    assert!(svc.verify_email_token(&refresh, "email_verify").is_err());
}

/// The TTL must come from the caller, not from a constant inside the service.
///
/// It used to be hardcoded to the password-reset lifetime for every purpose, which
/// is right for a reset and wrong for an unsubscribe link: an email sits in an
/// inbox for months, and an unsubscribe that has expired is indistinguishable from
/// one that does not work — which is what gets a sending domain reported rather
/// than merely muted.
#[test]
fn the_ttl_argument_reaches_the_expiry_claim() {
    let svc = svc();
    let user_id = Uuid::new_v4();

    // Proved by comparing two tokens rather than by watching one expire. The
    // obvious test — mint with a TTL of 0 and expect rejection — passes on a
    // service that ignores the argument entirely, because `jsonwebtoken`'s
    // `Validation` allows 60 seconds of leeway on `exp` for clock skew. Waiting
    // out that leeway is not a thing to put in a unit test.
    //
    // Everything but the TTL is identical, so the claims can only differ through
    // it. Crossing a second boundary between the two calls would also make them
    // differ, which is the direction that passes — so this cannot flake into a
    // false failure.
    let hour = svc.mint_email_token(user_id, "unsubscribe", 3600).unwrap();
    let year = svc
        .mint_email_token(user_id, "unsubscribe", 365 * 24 * 3600)
        .unwrap();

    assert_ne!(
        hour, year,
        "two TTLs produced the same token — the ttl argument is being ignored, \
         which would silently give unsubscribe links the one-hour reset lifetime"
    );
}

#[test]
fn a_long_ttl_token_verifies() {
    let svc = svc();
    let user_id = Uuid::new_v4();
    // A year, the unsubscribe lifetime.
    let token = svc
        .mint_email_token(user_id, "unsubscribe", 365 * 24 * 3600)
        .unwrap();
    assert_eq!(
        svc.verify_email_token(&token, "unsubscribe").unwrap(),
        user_id
    );
}

#[test]
fn the_unsubscribe_purpose_is_not_interchangeable_with_the_others() {
    // Otherwise a password-reset link would double as an unsubscribe link, and
    // worse, an unsubscribe token — which is long-lived and sits in every
    // notification email — would be accepted where a reset token is expected.
    let svc = svc();
    let user_id = Uuid::new_v4();
    let unsub = svc.mint_email_token(user_id, "unsubscribe", 3600).unwrap();
    assert!(svc.verify_email_token(&unsub, "password_reset").is_err());
    assert!(svc.verify_email_token(&unsub, "email_verification").is_err());

    let reset = svc.mint_email_token(user_id, "password_reset", 3600).unwrap();
    assert!(svc.verify_email_token(&reset, "unsubscribe").is_err());
}
