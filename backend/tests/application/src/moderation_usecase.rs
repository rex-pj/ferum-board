use std::sync::Arc;

use uuid::Uuid;
use ferum_application::usecases::moderation_usecase::{CreateReportCmd, ModerationUseCase};
use ferum_domain::AppError;
use ferum_test_support::fixtures::{ids, make_post, make_report, make_user, AuthUserBuilder};
use ferum_test_support::mocks::{
    audit_log_repository::NoopAuditLogRepository,
    cache_service::MockCacheService,
    event_publisher::MockEventPublisher,
    notification_repository::MockNotificationRepository,
    post_repository::MockPostRepository,
    report_repository::MockReportRepository,
    thread_repository::MockThreadRepository,
    user_repository::MockUserRepository,
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
    assert!(matches!(result, Err(AppError::UnprocessableEntity(_))));
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
