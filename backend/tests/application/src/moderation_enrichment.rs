//! Tests for `ModerationUseCase`'s batched report enrichment.
//!
//! `enrich_reports` is private, exercised here through the two public
//! list-with-context methods. What these tests pin down is not only the output
//! shape but the *query count*: the point of the batched form is that a page of
//! N reports costs a fixed number of repository calls instead of up to 2N.
//! `mockall`'s `.times(..)` is what actually enforces that — without it a
//! regression back to per-row lookups would still satisfy every assertion about
//! slugs and titles.

use std::sync::Arc;

use uuid::Uuid;

use ferum_application::usecases::moderation_usecase::ModerationUseCase;
use ferum_domain::models::report::Report;
use ferum_domain::models::thread::Thread;
use ferum_domain::AuthUser;
use ferum_test_support::fixtures::{
    ids, make_post, make_report, make_thread, make_user, AuthUserBuilder,
};
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

/// Only the four repositories enrichment touches vary between these tests; the
/// rest are inert.
fn build_uc(
    reports: MockReportRepository,
    posts: MockPostRepository,
    threads: MockThreadRepository,
    users: MockUserRepository,
) -> ModerationUseCase {
    ModerationUseCase::new(
        Arc::new(reports),
        Arc::new(posts),
        Arc::new(threads),
        Arc::new(users),
        Arc::new(MockNotificationRepository::new()),
        Arc::new(NoopAuditLogRepository),
        Arc::new(MockEventPublisher::new()),
        Arc::new(MockCacheService::new()),
    )
}

/// Distinct id prefixes, so a mix-up between two of them shows up as a wrong
/// value rather than a coincidentally-correct one.
mod rid {
    use uuid::Uuid;
    pub fn thread_1() -> Uuid { Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000001").unwrap() }
    pub fn thread_2() -> Uuid { Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000002").unwrap() }
    pub fn post_1() -> Uuid { Uuid::parse_str("bbbbbbbb-0000-0000-0000-000000000001").unwrap() }
    pub fn post_2() -> Uuid { Uuid::parse_str("bbbbbbbb-0000-0000-0000-000000000002").unwrap() }
    pub fn reporter() -> Uuid { Uuid::parse_str("cccccccc-0000-0000-0000-000000000001").unwrap() }
    pub fn ghost() -> Uuid { Uuid::parse_str("dddddddd-0000-0000-0000-00000000dead").unwrap() }
}

/// A thread whose slug/title encode a label, so a failure names the thread that
/// actually came back instead of the fixture default.
fn labelled_thread(id: Uuid, label: &str) -> Thread {
    let mut t = make_thread(id, ids::category_a(), ids::user_a());
    t.slug = format!("slug-{label}");
    t.title = format!("Title {label}");
    t
}

fn reports_repo(reports: Vec<Report>) -> MockReportRepository {
    let total = reports.len() as u64;
    let mut repo = MockReportRepository::new();
    repo.expect_list_all()
        .returning(move |_, _, _, _, _, _| Ok((reports.clone(), total)));
    repo
}

fn admin() -> AuthUser {
    AuthUserBuilder::admin().build()
}

/// Reporter lookup resolves; used wherever the username is not what's under test.
fn users_returning_reporter() -> MockUserRepository {
    let mut users = MockUserRepository::new();
    users
        .expect_find_many_by_ids()
        .returning(|_| Ok(vec![make_user(rid::reporter())]));
    users
}

#[tokio::test]
async fn batches_lookups_instead_of_querying_per_row() {
    // Six reports: three on threads, three on posts. The per-row form this
    // replaced would issue 3 thread lookups + 3 post lookups + 3 further thread
    // lookups = 9 calls. The batched form must issue exactly 4 (1 user,
    // 1 post, 2 thread), and stay at 4 no matter how long the page gets.
    let reports = vec![
        make_report(rid::reporter(), None, Some(rid::thread_1())),
        make_report(rid::reporter(), None, Some(rid::thread_2())),
        make_report(rid::reporter(), None, Some(rid::thread_1())),
        make_report(rid::reporter(), Some(rid::post_1()), None),
        make_report(rid::reporter(), Some(rid::post_2()), None),
        make_report(rid::reporter(), Some(rid::post_1()), None),
    ];

    let mut users = MockUserRepository::new();
    users
        .expect_find_many_by_ids()
        .times(1)
        .returning(|_| Ok(vec![make_user(rid::reporter())]));

    let mut posts = MockPostRepository::new();
    posts.expect_find_many_by_ids().times(1).returning(|_| {
        Ok(vec![
            make_post(rid::post_1(), rid::thread_1(), ids::user_a()),
            make_post(rid::post_2(), rid::thread_2(), ids::user_a()),
        ])
    });

    // Two thread calls, not six: one for directly-reported threads, one for the
    // threads reached only through a reported post.
    let mut threads = MockThreadRepository::new();
    threads
        .expect_find_many_by_ids()
        .times(2)
        .returning(|ids| Ok(ids.iter().map(|id| labelled_thread(*id, "x")).collect()));

    let uc = build_uc(reports_repo(reports), posts, threads, users);
    let (enriched, total) = uc
        .list_all_reports_with_context(&admin(), None, None, None, 1, 50)
        .await
        .expect("list");

    assert_eq!(enriched.len(), 6);
    assert_eq!(total, 6);
    // Every row resolved; mockall verifies the call counts when the mocks drop.
    assert!(enriched.iter().all(|r| r.thread_slug.is_some()));
}

#[tokio::test]
async fn two_reports_on_the_same_thread_both_resolve() {
    // Regression guard. An implementation that *consumes* the lookup entry
    // (`HashMap::remove` rather than `get`) resolves the first report and
    // silently returns None for the second — which is exactly the shape of bug
    // batching invites. Both must come back populated.
    let reports = vec![
        make_report(rid::reporter(), None, Some(rid::thread_1())),
        make_report(rid::reporter(), None, Some(rid::thread_1())),
    ];

    let mut posts = MockPostRepository::new();
    posts.expect_find_many_by_ids().returning(|_| Ok(vec![]));
    let mut threads = MockThreadRepository::new();
    threads
        .expect_find_many_by_ids()
        .returning(|_| Ok(vec![labelled_thread(rid::thread_1(), "one")]));

    let uc = build_uc(reports_repo(reports), posts, threads, users_returning_reporter());
    let (enriched, _) = uc
        .list_all_reports_with_context(&admin(), None, None, None, 1, 50)
        .await
        .expect("list");

    assert_eq!(enriched[0].thread_slug.as_deref(), Some("slug-one"));
    assert_eq!(enriched[1].thread_slug.as_deref(), Some("slug-one"));
    assert_eq!(enriched[1].thread_title.as_deref(), Some("Title one"));
}

#[tokio::test]
async fn a_post_report_resolves_through_the_posts_thread() {
    let reports = vec![make_report(rid::reporter(), Some(rid::post_1()), None)];

    let mut posts = MockPostRepository::new();
    posts
        .expect_find_many_by_ids()
        .returning(|_| Ok(vec![make_post(rid::post_1(), rid::thread_2(), ids::user_a())]));
    let mut threads = MockThreadRepository::new();
    threads.expect_find_many_by_ids().returning(|ids| {
        // Only the thread the post belongs to is ever asked for.
        if !ids.is_empty() {
            assert_eq!(ids, [rid::thread_2()]);
        }
        Ok(ids.iter().map(|id| labelled_thread(*id, "via-post")).collect())
    });

    let uc = build_uc(reports_repo(reports), posts, threads, users_returning_reporter());
    let (enriched, _) = uc
        .list_all_reports_with_context(&admin(), None, None, None, 1, 50)
        .await
        .expect("list");

    assert_eq!(enriched[0].thread_slug.as_deref(), Some("slug-via-post"));
}

#[tokio::test]
async fn thread_id_wins_when_a_report_carries_both_targets() {
    // Matches the original if/else-if ordering: `thread_id` is checked first, so
    // a report holding both is a thread report and its post is never fetched.
    let reports = vec![make_report(rid::reporter(), Some(rid::post_1()), Some(rid::thread_1()))];

    let mut posts = MockPostRepository::new();
    posts.expect_find_many_by_ids().returning(|ids| {
        assert!(ids.is_empty(), "post lookup must be skipped for a thread report");
        Ok(vec![])
    });
    let mut threads = MockThreadRepository::new();
    threads
        .expect_find_many_by_ids()
        .returning(|ids| Ok(ids.iter().map(|id| labelled_thread(*id, "direct")).collect()));

    let uc = build_uc(reports_repo(reports), posts, threads, users_returning_reporter());
    let (enriched, _) = uc
        .list_all_reports_with_context(&admin(), None, None, None, 1, 50)
        .await
        .expect("list");

    assert_eq!(enriched[0].thread_slug.as_deref(), Some("slug-direct"));
}

#[tokio::test]
async fn a_thread_reached_both_directly_and_via_a_post_is_not_fetched_twice() {
    // thread_1 is reported directly AND is the parent of a separately reported
    // post. The second batch must exclude it rather than load it again.
    let reports = vec![
        make_report(rid::reporter(), None, Some(rid::thread_1())),
        make_report(rid::reporter(), Some(rid::post_1()), None),
    ];

    let mut posts = MockPostRepository::new();
    posts
        .expect_find_many_by_ids()
        .returning(|_| Ok(vec![make_post(rid::post_1(), rid::thread_1(), ids::user_a())]));

    let mut threads = MockThreadRepository::new();
    let mut seq = mockall::Sequence::new();
    threads
        .expect_find_many_by_ids()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|ids| {
            assert_eq!(ids, [rid::thread_1()], "first batch is the direct threads");
            Ok(vec![labelled_thread(rid::thread_1(), "shared")])
        });
    threads
        .expect_find_many_by_ids()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|ids| {
            assert!(ids.is_empty(), "thread_1 was already loaded; it must not be refetched");
            Ok(vec![])
        });

    let uc = build_uc(reports_repo(reports), posts, threads, users_returning_reporter());
    let (enriched, _) = uc
        .list_all_reports_with_context(&admin(), None, None, None, 1, 50)
        .await
        .expect("list");

    assert_eq!(enriched[0].thread_slug.as_deref(), Some("slug-shared"));
    assert_eq!(enriched[1].thread_slug.as_deref(), Some("slug-shared"));
}

#[tokio::test]
async fn a_deleted_target_yields_no_slug_and_an_unknown_reporter_falls_back_to_its_id() {
    // Both fallbacks the per-row version had must survive: a target that no
    // longer resolves gives (None, None) rather than erroring, and a reporter
    // missing from the batch renders as its raw uuid.
    let reports = vec![make_report(rid::ghost(), None, Some(rid::thread_1()))];

    let mut users = MockUserRepository::new();
    users.expect_find_many_by_ids().returning(|_| Ok(vec![]));
    let mut posts = MockPostRepository::new();
    posts.expect_find_many_by_ids().returning(|_| Ok(vec![]));
    let mut threads = MockThreadRepository::new();
    threads.expect_find_many_by_ids().returning(|_| Ok(vec![]));

    let uc = build_uc(reports_repo(reports), posts, threads, users);
    let (enriched, _) = uc
        .list_all_reports_with_context(&admin(), None, None, None, 1, 50)
        .await
        .expect("list");

    assert_eq!(enriched[0].thread_slug, None);
    assert_eq!(enriched[0].thread_title, None);
    assert_eq!(enriched[0].reporter_username, rid::ghost().to_string());
}

#[tokio::test]
async fn enrichment_preserves_the_order_the_repository_returned() {
    // The repository owns the ORDER BY; enrichment must not reshuffle the page.
    // The mock deliberately returns the threads in the opposite order.
    let reports = vec![
        make_report(rid::reporter(), None, Some(rid::thread_2())),
        make_report(rid::reporter(), None, Some(rid::thread_1())),
    ];

    let mut posts = MockPostRepository::new();
    posts.expect_find_many_by_ids().returning(|_| Ok(vec![]));
    let mut threads = MockThreadRepository::new();
    threads.expect_find_many_by_ids().returning(|_| {
        Ok(vec![
            labelled_thread(rid::thread_1(), "one"),
            labelled_thread(rid::thread_2(), "two"),
        ])
    });

    let uc = build_uc(reports_repo(reports), posts, threads, users_returning_reporter());
    let (enriched, _) = uc
        .list_all_reports_with_context(&admin(), None, None, None, 1, 50)
        .await
        .expect("list");

    assert_eq!(enriched[0].thread_slug.as_deref(), Some("slug-two"));
    assert_eq!(enriched[1].thread_slug.as_deref(), Some("slug-one"));
}

#[tokio::test]
async fn an_empty_page_asks_for_nothing() {
    let mut users = MockUserRepository::new();
    users.expect_find_many_by_ids().returning(|ids| {
        assert!(ids.is_empty());
        Ok(vec![])
    });
    let mut posts = MockPostRepository::new();
    posts.expect_find_many_by_ids().returning(|ids| {
        assert!(ids.is_empty());
        Ok(vec![])
    });
    let mut threads = MockThreadRepository::new();
    threads.expect_find_many_by_ids().returning(|ids| {
        assert!(ids.is_empty());
        Ok(vec![])
    });

    let uc = build_uc(reports_repo(vec![]), posts, threads, users);
    let (enriched, total) = uc
        .list_all_reports_with_context(&admin(), None, None, None, 1, 50)
        .await
        .expect("list");

    assert!(enriched.is_empty());
    assert_eq!(total, 0);
}

#[tokio::test]
async fn the_moderator_scoped_listing_enriches_through_the_same_path() {
    // `list_reports_with_context` used to carry a verbatim copy of the
    // enrichment block. Both now share one helper — this pins that down so the
    // duplicate cannot quietly reappear and drift.
    let actor = AuthUserBuilder::member()
        .with_perm("moderation.view_reports")
        .build();
    let reports = vec![make_report(rid::reporter(), Some(rid::post_1()), None)];

    let mut users = MockUserRepository::new();
    users
        .expect_find_many_by_ids()
        .times(1)
        .returning(|_| Ok(vec![make_user(rid::reporter())]));
    let mut posts = MockPostRepository::new();
    posts
        .expect_find_many_by_ids()
        .times(1)
        .returning(|_| Ok(vec![make_post(rid::post_1(), rid::thread_2(), ids::user_a())]));
    let mut threads = MockThreadRepository::new();
    threads
        .expect_find_many_by_ids()
        .times(2)
        .returning(|ids| Ok(ids.iter().map(|id| labelled_thread(*id, "mod")).collect()));

    let uc = build_uc(reports_repo(reports), posts, threads, users);
    let (enriched, _) = uc
        .list_reports_with_context(&actor, None, None, None, 1, 50)
        .await
        .expect("list");

    assert_eq!(enriched.len(), 1);
    assert_eq!(enriched[0].thread_slug.as_deref(), Some("slug-mod"));
    assert_eq!(enriched[0].reporter_username, "testuser");
}
