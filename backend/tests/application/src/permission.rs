//! The ban rule stated at the top of `permission.rs`: a banned user must never
//! reach a permission check that could grant them something.
//!
//! Eleven checks once omitted it — every administrative and moderation one — so a
//! banned admin kept full powers and could unban themselves, while the same
//! account was correctly refused an ordinary reply. Nothing else catches this:
//! middleware resolves `is_banned` but does not reject, and neither
//! `require_admin` nor `require_moderator` reads it.

use ferum_application::permission::PermissionChecker as P;
use ferum_application::shared::AppError;
use ferum_domain::AuthUser;
use ferum_test_support::fixtures::AuthUserBuilder;

/// Every permission the privileged checks look for, so the only thing standing
/// between the actor and success is the ban.
const ALL_PRIVILEGED_PERMS: &[&str] = &[
    "admin.users",
    "admin.categories",
    "admin.roles",
    "admin.config",
    "admin.webhooks",
    "admin.plugins",
    "admin.ban_permanent",
    "moderation.warn",
    "moderation.ban_temp",
    "moderation.view_reports",
    "moderation.resolve",
    "product.manage",
    "product.submit",
    "file.upload",
    "reaction.add",
];

fn banned_but_fully_privileged() -> AuthUser {
    AuthUserBuilder::member()
        .with_perms(ALL_PRIVILEGED_PERMS)
        .banned()
        .build()
}

fn privileged() -> AuthUser {
    AuthUserBuilder::member().with_perms(ALL_PRIVILEGED_PERMS).build()
}

/// A check plus the name a failure should report.
type NamedCheck = (&'static str, fn(&AuthUser) -> Result<(), AppError>);

fn privileged_checks() -> Vec<NamedCheck> {
    vec![
        ("can_warn", P::can_warn),
        ("can_ban_temp", P::can_ban_temp),
        ("can_ban_permanent", P::can_ban_permanent),
        ("can_manage_users", P::can_manage_users),
        ("can_manage_categories", P::can_manage_categories),
        ("can_manage_roles", P::can_manage_roles),
        ("can_manage_config", P::can_manage_config),
        ("can_manage_webhooks", P::can_manage_webhooks),
        ("can_manage_plugins", P::can_manage_plugins),
        ("can_manage_products", P::can_manage_products),
        ("can_submit_products", P::can_submit_products),
        ("can_react", P::can_react),
        ("can_upload", P::can_upload),
        ("can_upload_profile_image", P::can_upload_profile_image),
        ("can_view_reports", |u| P::can_view_reports(u, None)),
        ("can_resolve_report", |u| P::can_resolve_report(u, None)),
    ]
}

#[test]
fn no_privileged_check_admits_a_banned_actor() {
    // Add a new privileged check to the list above when you add one to
    // `PermissionChecker`. This test is the only place the set is written down.
    let banned = banned_but_fully_privileged();
    for (name, check) in privileged_checks() {
        match check(&banned) {
            Err(AppError::Forbidden(code)) if code == "account_suspended" => {}
            other => panic!("{name} admitted a banned actor (or failed wrongly): {other:?}"),
        }
    }
}

#[test]
fn the_same_checks_all_pass_without_the_ban() {
    // Otherwise the test above could pass because the fixture is missing a
    // permission rather than because the ban was enforced.
    let ok = privileged();
    for (name, check) in privileged_checks() {
        assert!(check(&ok).is_ok(), "{name} refused a fully-privileged actor");
    }
}

#[test]
fn a_banned_admin_cannot_reach_the_permission_that_would_unban_them() {
    // The concrete incident: ban an admin, and the ban is the one action that
    // does not stick, because unbanning runs through `can_ban_permanent`.
    let banned_admin = AuthUserBuilder::member()
        .with_perms(&["admin.users", "admin.ban_permanent"])
        .banned()
        .build();

    assert!(matches!(
        P::can_ban_permanent(&banned_admin),
        Err(AppError::Forbidden(c)) if c == "account_suspended"
    ));
    assert!(matches!(
        P::can_manage_users(&banned_admin),
        Err(AppError::Forbidden(c)) if c == "account_suspended"
    ));
}
