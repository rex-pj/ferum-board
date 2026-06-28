use std::sync::Arc;

use ferum_application::usecases::notification_usecase::NotificationUseCase;
use ferum_domain::AppError;
use ferum_test_support::fixtures::{ids, AuthUserBuilder};
use ferum_test_support::mocks::notification_repository::MockNotificationRepository;

fn build_uc(notifications: MockNotificationRepository) -> NotificationUseCase {
    NotificationUseCase::new(Arc::new(notifications))
}

// ─── inbox ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn inbox_banned_user_returns_forbidden() {
    let actor = AuthUserBuilder::member().banned().build();
    let uc = build_uc(MockNotificationRepository::new());

    let result = uc.inbox(&actor, 1, 20).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn inbox_returns_notifications_for_user() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();

    let mut repo = MockNotificationRepository::new();
    repo.expect_list_for_user()
        .return_once(|_, _, _| Ok((vec![], 0)));

    let uc = build_uc(repo);
    let (notifs, total) = uc.inbox(&actor, 1, 20).await.expect("inbox succeeds");
    assert_eq!(total, 0);
    assert!(notifs.is_empty());
}

#[tokio::test]
async fn inbox_caps_per_page_at_50() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();

    let mut repo = MockNotificationRepository::new();
    // Verify that per_page is capped at 50 when caller passes 200
    repo.expect_list_for_user()
        .withf(|_, _, per_page| *per_page == 50)
        .return_once(|_, _, _| Ok((vec![], 0)));

    let uc = build_uc(repo);
    uc.inbox(&actor, 1, 200).await.expect("inbox caps per_page");
}

// ─── unread_count ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn unread_count_delegates_to_repository() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();

    let mut repo = MockNotificationRepository::new();
    repo.expect_unread_count().return_once(|_| Ok(7));

    let uc = build_uc(repo);
    let count = uc.unread_count(&actor).await.expect("unread_count succeeds");
    assert_eq!(count, 7);
}

// ─── mark_read ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn mark_read_delegates_to_repository() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();

    let mut repo = MockNotificationRepository::new();
    repo.expect_mark_read().times(1).return_once(|_, _| Ok(()));

    let uc = build_uc(repo);
    assert!(uc.mark_read(&actor, ids::thread_a()).await.is_ok());
}

// ─── mark_all_read ────────────────────────────────────────────────────────────

#[tokio::test]
async fn mark_all_read_delegates_to_repository() {
    let actor = AuthUserBuilder::member().with_id(ids::user_a()).build();

    let mut repo = MockNotificationRepository::new();
    repo.expect_mark_all_read().times(1).return_once(|_| Ok(()));

    let uc = build_uc(repo);
    assert!(uc.mark_all_read(&actor).await.is_ok());
}
