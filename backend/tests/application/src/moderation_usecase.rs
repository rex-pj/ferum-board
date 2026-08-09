use std::sync::Arc;

use uuid::Uuid;
use ferum_application::usecases::moderation_usecase::{CreateReportCmd, ModerationUseCase};
use ferum_domain::AppError;
use ferum_test_support::fixtures::{ids, make_post, make_report, make_user, AuthUserBuilder};
use ferum_test_support::mocks::{
    audit_log_repository::{NoopAuditLogRepository, SpyAuditLog},
    cache_service::MockCacheService,
    event_publisher::MockEventPublisher,
    notification_repository::MockNotificationRepository,
    permission_resolver::FixedPermissionResolver,
    post_repository::MockPostRepository,
    report_repository::MockReportRepository,
    thread_repository::MockThreadRepository,
    user_repository::MockUserRepository,
    user_role_repository::MockUserRoleRepository,
};

fn build_uc(
    reports: MockReportRepository,
    posts: MockPostRepository,
    threads: MockThreadRepository,
    users: MockUserRepository,
    notifications: MockNotificationRepository,
    cache: MockCacheService,
    events: MockEventPublisher,
) -> ModerationUseCase {
    ModerationUseCase::new(
        Arc::new(reports),
        Arc::new(posts),
        Arc::new(threads),
        Arc::new(users),
        Arc::new(notifications),
        Arc::new(NoopAuditLogRepository),
        Arc::new(events),
        Arc::new(cache),
    )
}

/// `build_uc` plus the staff lookup, with the *target* resolving to `target_perms`.
///
/// Separate from `build_uc` because most tests do not exercise the staff rule
/// and wiring a resolver into all of them would obscure what they are about.
fn build_uc_with_target_perms(
    users: MockUserRepository,
    notifications: MockNotificationRepository,
    cache: MockCacheService,
    events: MockEventPublisher,
    resolver: FixedPermissionResolver,
) -> ModerationUseCase {
    let mut user_roles = MockUserRoleRepository::new();
    // The assignments themselves are irrelevant — FixedPermissionResolver
    // ignores them and answers with the permission set under test.
    user_roles.expect_list_for_user().returning(|_| Ok(vec![]));

    build_uc(
        MockReportRepository::new(),
        MockPostRepository::new(),
        MockThreadRepository::new(),
        users,
        notifications,
        cache,
        events,
    )
    .with_staff_lookup(Arc::new(user_roles), Arc::new(resolver))
}

// ─── create_report ─────────────────────────────────────────────────────────

#[tokio::test]
async fn create_report_banned_returns_403() {
    let actor = AuthUserBuilder::member()
        .with_perm("report.create")
        .banned()
        .build();

    let uc = build_uc(
        MockReportRepository::new(), MockPostRepository::new(), MockThreadRepository::new(),
        MockUserRepository::new(), MockNotificationRepository::new(), MockCacheService::new(),
        MockEventPublisher::new(),
    );
    let result = uc.create_report(&actor, CreateReportCmd {
        post_id: Some(ids::post_a()), thread_id: None, reason: "spam".to_string(),
    }).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn create_report_without_perm_returns_403() {
    let actor = AuthUserBuilder::member().build(); // no report.create perm

    let uc = build_uc(
        MockReportRepository::new(), MockPostRepository::new(), MockThreadRepository::new(),
        MockUserRepository::new(), MockNotificationRepository::new(), MockCacheService::new(),
        MockEventPublisher::new(),
    );
    let result = uc.create_report(&actor, CreateReportCmd {
        post_id: Some(ids::post_a()), thread_id: None, reason: "spam".to_string(),
    }).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn create_report_no_target_returns_422() {
    let actor = AuthUserBuilder::member()
        .with_perm("report.create")
        .with_trust(ferum_domain::models::user::TrustLevel::Basic)
        .build();

    let uc = build_uc(
        MockReportRepository::new(), MockPostRepository::new(), MockThreadRepository::new(),
        MockUserRepository::new(), MockNotificationRepository::new(), MockCacheService::new(),
        MockEventPublisher::new(),
    );
    let result = uc.create_report(&actor, CreateReportCmd {
        post_id: None, thread_id: None, reason: "spam".to_string(),
    }).await;
    assert!(matches!(&result, Err(AppError::Invalid { code, .. }) if code == "report_target_required"),
        "expected report_target_required, got {result:?}");
}

#[tokio::test]
async fn create_report_post_not_found_returns_404() {
    let actor = AuthUserBuilder::member()
        .with_perm("report.create")
        .with_trust(ferum_domain::models::user::TrustLevel::Basic)
        .build();

    let mut posts = MockPostRepository::new();
    posts.expect_find_by_id().returning(|_| Ok(None));

    let uc = build_uc(
        MockReportRepository::new(), posts, MockThreadRepository::new(),
        MockUserRepository::new(), MockNotificationRepository::new(), MockCacheService::new(),
        MockEventPublisher::new(),
    );
    let result = uc.create_report(&actor, CreateReportCmd {
        post_id: Some(ids::post_a()), thread_id: None, reason: "spam".to_string(),
    }).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn create_report_post_succeeds() {
    let actor_id = Uuid::new_v4();
    let actor = AuthUserBuilder::member()
        .with_id(actor_id)
        .with_perm("report.create")
        .with_trust(ferum_domain::models::user::TrustLevel::Basic)
        .build();

    let post_id = ids::post_a();
    let existing_post = make_post(post_id, ids::thread_a(), ids::user_a());
    let report = make_report(actor_id, Some(post_id), None);

    let mut posts = MockPostRepository::new();
    posts.expect_find_by_id().returning(move |_| Ok(Some(existing_post.clone())));

    let mut reports = MockReportRepository::new();
    reports.expect_create().returning(move |_, _, _, _| Ok(report.clone()));

    let uc = build_uc(
        reports, posts, MockThreadRepository::new(),
        MockUserRepository::new(), MockNotificationRepository::new(), MockCacheService::new(),
        MockEventPublisher::new(),
    );
    let result = uc.create_report(&actor, CreateReportCmd {
        post_id: Some(post_id), thread_id: None, reason: "spam".to_string(),
    }).await;
    assert!(result.is_ok());
}

// ─── warn_user ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn warn_user_without_perm_returns_403() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(
        MockReportRepository::new(), MockPostRepository::new(), MockThreadRepository::new(),
        MockUserRepository::new(), MockNotificationRepository::new(), MockCacheService::new(),
        MockEventPublisher::new(),
    );
    let result = uc.warn_user(&actor, Uuid::new_v4(), "reason".to_string()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn warn_user_not_found_returns_404() {
    let actor = AuthUserBuilder::member().with_perms(&["moderation.warn"]).build();

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(|_| Ok(None));

    let uc = build_uc(
        MockReportRepository::new(), MockPostRepository::new(), MockThreadRepository::new(),
        users, MockNotificationRepository::new(), MockCacheService::new(),
        MockEventPublisher::new(),
    );
    let result = uc.warn_user(&actor, Uuid::new_v4(), "reason".to_string()).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn warn_user_success() {
    let target_id = Uuid::new_v4();
    let target_user = make_user(target_id);
    let actor = AuthUserBuilder::member().with_perm("moderation.warn").build();

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(target_user.clone())));
    users.expect_update().returning(move |_, _| Ok(make_user(target_id)));
    // increment_trust_score is fire-and-forget
    users.expect_increment_trust_score().returning(|_, _| Ok(()));

    let mut notifications = MockNotificationRepository::new();
    notifications.expect_create().returning(move |_, _, _| {
        use ferum_domain::models::notification::{Notification, NotificationKind};
        Ok(Notification {
            id: Uuid::new_v4(), user_id: target_id,
            kind: NotificationKind::Warn,
            payload: serde_json::Value::Null,
            is_read: false, read_at: None,
            created_at: chrono::Utc::now(),
        })
    });

    let mut events = MockEventPublisher::new();
    events.expect_publish().returning(|_| ());

    let uc = build_uc(
        MockReportRepository::new(), MockPostRepository::new(), MockThreadRepository::new(),
        users, notifications, MockCacheService::new(), events,
    );
    let result = uc.warn_user(&actor, target_id, "spamming".to_string()).await;
    assert!(result.is_ok());
}

// ─── temp_ban ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn temp_ban_without_perm_returns_403() {
    let actor = AuthUserBuilder::member().build();
    let uc = build_uc(
        MockReportRepository::new(), MockPostRepository::new(), MockThreadRepository::new(),
        MockUserRepository::new(), MockNotificationRepository::new(), MockCacheService::new(),
        MockEventPublisher::new(),
    );
    let until = chrono::Utc::now() + chrono::Duration::hours(24);
    let result = uc.temp_ban(&actor, Uuid::new_v4(), "reason".to_string(), until).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn temp_ban_user_not_found_returns_404() {
    let actor = AuthUserBuilder::member().with_perm("moderation.ban_temp").build();
    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(|_| Ok(None));

    let uc = build_uc(
        MockReportRepository::new(), MockPostRepository::new(), MockThreadRepository::new(),
        users, MockNotificationRepository::new(), MockCacheService::new(),
        MockEventPublisher::new(),
    );
    let until = chrono::Utc::now() + chrono::Duration::hours(24);
    let result = uc.temp_ban(&actor, Uuid::new_v4(), "reason".to_string(), until).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn temp_ban_success() {
    let target_id = Uuid::new_v4();
    let target_user = make_user(target_id);
    let actor = AuthUserBuilder::member().with_perm("moderation.ban_temp").build();
    let until = chrono::Utc::now() + chrono::Duration::hours(24);

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(target_user.clone())));
    users.expect_update().returning(move |_, _| Ok(make_user(target_id)));

    let mut cache = MockCacheService::new();
    cache.expect_set().returning(|_, _, _| Ok(()));
    cache.expect_del_prefix().returning(|_| Ok(()));
    cache.expect_del().returning(|_| Ok(()));

    let mut events = MockEventPublisher::new();
    events.expect_publish().returning(|_| ());

    let uc = build_uc(
        MockReportRepository::new(), MockPostRepository::new(), MockThreadRepository::new(),
        users, MockNotificationRepository::new(), cache, events,
    );
    let result = uc.temp_ban(&actor, target_id, "spam".to_string(), until).await;
    assert!(result.is_ok());
}

/// `moderation.ban_temp` and `admin.ban_permanent` are separate permissions
/// precisely so a moderator cannot end an account outright. With no ceiling on
/// `until`, a moderator holding only the first could pass a date centuries out
/// and get exactly the effect of the second — the split enforced nothing.
#[tokio::test]
async fn temp_ban_refuses_a_duration_beyond_the_ceiling() {
    let target_id = Uuid::new_v4();
    let target_user = make_user(target_id);
    let actor = AuthUserBuilder::member().with_perm("moderation.ban_temp").build();

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(target_user.clone())));
    // Reached only if the guard fails to fire, which is what makes this assert.
    users.expect_update().never();

    let uc = build_uc(
        MockReportRepository::new(), MockPostRepository::new(), MockThreadRepository::new(),
        users, MockNotificationRepository::new(), MockCacheService::new(),
        MockEventPublisher::new(),
    );

    let forever = chrono::Utc::now() + chrono::Duration::days(365 * 100);
    let result = uc.temp_ban(&actor, target_id, "spam".to_string(), forever).await;
    assert!(
        matches!(&result, Err(AppError::Invalid { code, .. }) if code == "ban_duration_too_long"),
        "expected ban_duration_too_long, got {result:?}"
    );
}

/// The ceiling is a ceiling, not a narrowing: a ban right up to the limit is
/// still a legitimate moderator action and must go through.
#[tokio::test]
async fn temp_ban_allows_a_duration_at_the_ceiling() {
    let target_id = Uuid::new_v4();
    let target_user = make_user(target_id);
    let actor = AuthUserBuilder::member().with_perm("moderation.ban_temp").build();

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(target_user.clone())));
    users.expect_update().returning(move |_, _| Ok(make_user(target_id)));

    let mut cache = MockCacheService::new();
    cache.expect_set().returning(|_, _, _| Ok(()));
    cache.expect_del_prefix().returning(|_| Ok(()));
    cache.expect_del().returning(|_| Ok(()));

    let mut events = MockEventPublisher::new();
    events.expect_publish().returning(|_| ());

    let uc = build_uc(
        MockReportRepository::new(), MockPostRepository::new(), MockThreadRepository::new(),
        users, MockNotificationRepository::new(), cache, events,
    );

    // A minute inside the limit, to stay clear of the clock advancing between
    // the test computing the date and the use case checking it.
    let until = chrono::Utc::now()
        + chrono::Duration::days(ferum_application::constants::MAX_TEMP_BAN_DAYS)
        - chrono::Duration::minutes(1);
    uc.temp_ban(&actor, target_id, "spam".to_string(), until).await.unwrap();
}

// ─── who a moderator may act on ───────────────────────────────────────────────
//
// Nothing checked this before: `warn_user` and `temp_ban` verified the actor's
// permission and then acted on whatever user id they were handed. A moderator
// could ban the site owner, and — with `until` unbounded, as it also was —
// permanently. The comment in the source claimed the case was "handled by the
// fact that admins have all permissions", which does not follow.

fn a_moderator() -> ferum_domain::AuthUser {
    AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perms(&["moderation.warn", "moderation.ban_temp"])
        .build()
}

fn an_admin() -> ferum_domain::AuthUser {
    AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perms(&["admin.users", "moderation.warn", "moderation.ban_temp"])
        .build()
}

fn one_day_out() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now() + chrono::Duration::hours(24)
}

#[tokio::test]
async fn a_moderator_cannot_ban_an_admin() {
    let mut users = MockUserRepository::new();
    users.expect_update().never();

    let uc = build_uc_with_target_perms(
        users,
        MockNotificationRepository::new(),
        MockCacheService::new(),
        MockEventPublisher::new(),
        FixedPermissionResolver::global(&["admin.users"]),
    );

    let result = uc.temp_ban(&a_moderator(), ids::user_b(), "spam".into(), one_day_out()).await;
    assert!(
        matches!(&result, Err(AppError::Forbidden(c)) if c == "cannot_moderate_staff"),
        "got {result:?}"
    );
}

#[tokio::test]
async fn a_moderator_cannot_warn_another_moderator() {
    let mut users = MockUserRepository::new();
    users.expect_update().never();

    let uc = build_uc_with_target_perms(
        users,
        MockNotificationRepository::new(),
        MockCacheService::new(),
        MockEventPublisher::new(),
        FixedPermissionResolver::global(&["moderation.warn"]),
    );

    let result = uc.warn_user(&a_moderator(), ids::user_b(), "rude".into()).await;
    assert!(
        matches!(&result, Err(AppError::Forbidden(c)) if c == "cannot_moderate_staff"),
        "got {result:?}"
    );
}

/// A category-scoped moderator is still staff when seen from outside that
/// category — the protection cannot depend on where the actor happens to stand.
#[tokio::test]
async fn a_moderator_cannot_ban_a_category_scoped_moderator() {
    let mut users = MockUserRepository::new();
    users.expect_update().never();

    let uc = build_uc_with_target_perms(
        users,
        MockNotificationRepository::new(),
        MockCacheService::new(),
        MockEventPublisher::new(),
        FixedPermissionResolver::in_category(ids::category_a(), &["moderation.view_reports"]),
    );

    let result = uc.temp_ban(&a_moderator(), ids::user_b(), "spam".into(), one_day_out()).await;
    assert!(
        matches!(&result, Err(AppError::Forbidden(c)) if c == "cannot_moderate_staff"),
        "got {result:?}"
    );
}

/// The rule narrows nothing for ordinary moderation, which is the point.
#[tokio::test]
async fn a_moderator_can_still_ban_an_ordinary_member() {
    let target = ids::user_b();
    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(make_user(target))));
    users.expect_update().times(1).returning(move |_, _| Ok(make_user(target)));

    let mut cache = MockCacheService::new();
    cache.expect_set().returning(|_, _, _| Ok(()));
    cache.expect_del_prefix().returning(|_| Ok(()));
    cache.expect_del().returning(|_| Ok(()));

    let mut events = MockEventPublisher::new();
    events.expect_publish().returning(|_| ());

    let uc = build_uc_with_target_perms(
        users,
        MockNotificationRepository::new(),
        cache,
        events,
        FixedPermissionResolver::none(),
    );

    uc.temp_ban(&a_moderator(), target, "spam".into(), one_day_out()).await.unwrap();
}

/// An admin may act on staff — that is what "escalate to admin" means, and it
/// is the escape hatch that makes the rule above workable.
#[tokio::test]
async fn an_admin_can_ban_a_moderator() {
    let target = ids::user_b();
    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(make_user(target))));
    users.expect_update().times(1).returning(move |_, _| Ok(make_user(target)));

    let mut cache = MockCacheService::new();
    cache.expect_set().returning(|_, _, _| Ok(()));
    cache.expect_del_prefix().returning(|_| Ok(()));
    cache.expect_del().returning(|_| Ok(()));

    let mut events = MockEventPublisher::new();
    events.expect_publish().returning(|_| ());

    let uc = build_uc_with_target_perms(
        users,
        MockNotificationRepository::new(),
        cache,
        events,
        FixedPermissionResolver::global(&["moderation.warn"]),
    );

    uc.temp_ban(&an_admin(), target, "abuse of tools".into(), one_day_out()).await.unwrap();
}

/// Including an admin: a self-ban locks the actor out of their own account, and
/// for a sole administrator there is no way back through the UI.
#[tokio::test]
async fn nobody_can_ban_themselves() {
    let admin = an_admin();
    let mut users = MockUserRepository::new();
    users.expect_update().never();

    let uc = build_uc_with_target_perms(
        users,
        MockNotificationRepository::new(),
        MockCacheService::new(),
        MockEventPublisher::new(),
        FixedPermissionResolver::global(&["admin.users"]),
    );

    let result = uc.temp_ban(&admin, admin.id, "oops".into(), one_day_out()).await;
    assert!(
        matches!(&result, Err(AppError::Forbidden(c)) if c == "cannot_moderate_self"),
        "got {result:?}"
    );
}

// ─── refused moderation attempts are recorded ─────────────────────────────────
//
// Successful warns and bans are audited through `UserWarned` / `UserBanned`.
// A *refused* one left no trace at all — and a moderator repeatedly trying to
// ban an administrator is exactly the signal an audit log exists to capture,
// whether it is a compromised account or an insider testing the boundary.
//
// Only the two rank refusals are logged, not every `permission_denied`. These
// two cannot be reached by clicking: the UI never offers a moderator the option
// of banning an admin or themselves, so reaching them means the request was
// constructed deliberately. Logging ordinary permission failures would bury that
// signal under noise from stale tabs.

/// Same as `build_uc_with_target_perms` but hands back the audit spy too.
fn build_uc_with_audit(
    resolver: FixedPermissionResolver,
) -> (ModerationUseCase, Arc<SpyAuditLog>) {
    let audit = Arc::new(SpyAuditLog::default());
    let mut user_roles = MockUserRoleRepository::new();
    user_roles.expect_list_for_user().returning(|_| Ok(vec![]));

    let mut users = MockUserRepository::new();
    users.expect_update().never();

    let uc = ModerationUseCase::new(
        Arc::new(MockReportRepository::new()),
        Arc::new(MockPostRepository::new()),
        Arc::new(MockThreadRepository::new()),
        Arc::new(users),
        Arc::new(MockNotificationRepository::new()),
        audit.clone(),
        Arc::new(MockEventPublisher::new()),
        Arc::new(MockCacheService::new()),
    )
    .with_staff_lookup(Arc::new(user_roles), Arc::new(resolver));

    (uc, audit)
}

#[tokio::test]
async fn a_refused_ban_on_staff_is_recorded() {
    let (uc, audit) = build_uc_with_audit(FixedPermissionResolver::global(&["admin.users"]));
    let actor = a_moderator();

    uc.temp_ban(&actor, ids::user_b(), "spam".into(), one_day_out())
        .await
        .unwrap_err();

    let entry = audit.only();
    assert_eq!(entry.actor_id, Some(actor.id));
    assert_eq!(entry.action, "moderation.refused");
    assert_eq!(entry.target_type, "user");
    assert_eq!(entry.target_id, ids::user_b(), "the target must be the person acted on");

    let meta = entry.metadata.expect("refusal must say what was attempted and why");
    assert_eq!(meta["attempted"], "user.ban_temp");
    assert_eq!(meta["reason"], "cannot_moderate_staff");
}

#[tokio::test]
async fn a_refused_warn_records_the_action_it_was() {
    let (uc, audit) = build_uc_with_audit(FixedPermissionResolver::global(&["moderation.warn"]));

    uc.warn_user(&a_moderator(), ids::user_b(), "rude".into())
        .await
        .unwrap_err();

    let meta = audit.only().metadata.unwrap();
    assert_eq!(meta["attempted"], "user.warn", "warn and ban must be distinguishable");
    assert_eq!(meta["reason"], "cannot_moderate_staff");
}

#[tokio::test]
async fn a_self_ban_attempt_is_recorded_with_its_own_reason() {
    let (uc, audit) = build_uc_with_audit(FixedPermissionResolver::none());
    let actor = an_admin();

    uc.temp_ban(&actor, actor.id, "oops".into(), one_day_out())
        .await
        .unwrap_err();

    let meta = audit.only().metadata.unwrap();
    assert_eq!(meta["reason"], "cannot_moderate_self");
}

/// An allowed action must not produce a refusal entry — the successful one is
/// already written by the `UserBanned` event, and a second row would double-count
/// every ban in the log.
#[tokio::test]
async fn an_allowed_ban_records_no_refusal() {
    let target = ids::user_b();
    let audit = Arc::new(SpyAuditLog::default());
    let mut user_roles = MockUserRoleRepository::new();
    user_roles.expect_list_for_user().returning(|_| Ok(vec![]));

    let mut users = MockUserRepository::new();
    users.expect_find_by_id().returning(move |_| Ok(Some(make_user(target))));
    users.expect_update().returning(move |_, _| Ok(make_user(target)));

    let mut cache = MockCacheService::new();
    cache.expect_set().returning(|_, _, _| Ok(()));
    cache.expect_del_prefix().returning(|_| Ok(()));
    cache.expect_del().returning(|_| Ok(()));

    let mut events = MockEventPublisher::new();
    events.expect_publish().returning(|_| ());

    let uc = ModerationUseCase::new(
        Arc::new(MockReportRepository::new()),
        Arc::new(MockPostRepository::new()),
        Arc::new(MockThreadRepository::new()),
        Arc::new(users),
        Arc::new(MockNotificationRepository::new()),
        audit.clone(),
        Arc::new(events),
        Arc::new(cache),
    )
    .with_staff_lookup(Arc::new(user_roles), Arc::new(FixedPermissionResolver::none()));

    uc.temp_ban(&a_moderator(), target, "spam".into(), one_day_out()).await.unwrap();

    assert!(
        audit.entries().is_empty(),
        "a permitted action must not be logged as a refusal: {:#?}",
        audit.entries()
    );
}
