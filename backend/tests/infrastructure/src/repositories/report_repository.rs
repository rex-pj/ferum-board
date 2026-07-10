//! Integration tests for [`PgReportRepository`].
//!
//! `count_by_status` uses a GROUP BY + CASE aggregate; tested here.

use ferum_domain::models::report::ReportStatus;
use ferum_domain::repositories::report_repository::ReportRepository;
use ferum_infrastructure::repositories::PgReportRepository;

use crate::common::{insert_category, insert_post, insert_thread, insert_user, TestDb};

#[tokio::test]
async fn count_by_status_on_empty_returns_zeros() {
    // GROUP BY + CASE must not error on empty table
    let db = TestDb::new("rpt_count_empty").await;
    let repo = PgReportRepository::new(db.conn.clone());
    let counts = repo.count_by_status(None).await.expect("count_by_status");
    assert_eq!(counts.pending, 0);
    assert_eq!(counts.resolved, 0);
    assert_eq!(counts.dismissed, 0);
    db.teardown().await;
}

#[tokio::test]
async fn create_post_report_and_find_by_id() {
    let db = TestDb::new("rpt_create_post").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;
    let post = insert_post(&db.conn, thread.id, user.id).await;
    let reporter = insert_user(&db.conn, 2).await;

    let repo = PgReportRepository::new(db.conn.clone());
    let report = repo.create(reporter.id, Some(post.id), None, "spam".to_string())
        .await.expect("create");
    assert_eq!(report.status, ReportStatus::Pending);
    assert_eq!(report.post_id, Some(post.id));

    let found = repo.find_by_id(report.id).await.expect("find_by_id").unwrap();
    assert_eq!(found.id, report.id);
    db.teardown().await;
}

#[tokio::test]
async fn create_thread_report() {
    let db = TestDb::new("rpt_create_thread").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;
    let reporter = insert_user(&db.conn, 2).await;

    let repo = PgReportRepository::new(db.conn.clone());
    let report = repo.create(reporter.id, None, Some(thread.id), "off-topic".to_string())
        .await.expect("create thread report");
    assert_eq!(report.thread_id, Some(thread.id));
    assert!(report.post_id.is_none());
    db.teardown().await;
}

#[tokio::test]
async fn count_by_status_counts_pending_reports() {
    let db = TestDb::new("rpt_count_pending").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;
    let post = insert_post(&db.conn, thread.id, user.id).await;
    let reporter = insert_user(&db.conn, 2).await;

    let repo = PgReportRepository::new(db.conn.clone());
    repo.create(reporter.id, Some(post.id), None, "spam".to_string()).await.expect("create");
    repo.create(reporter.id, None, Some(thread.id), "off".to_string()).await.expect("create 2");

    let counts = repo.count_by_status(None).await.expect("count_by_status");
    assert_eq!(counts.pending, 2);
    db.teardown().await;
}

#[tokio::test]
async fn update_status_to_resolved() {
    let db = TestDb::new("rpt_update_resolved").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;
    let post = insert_post(&db.conn, thread.id, user.id).await;
    let reporter = insert_user(&db.conn, 2).await;
    let mod_user = insert_user(&db.conn, 3).await;

    let repo = PgReportRepository::new(db.conn.clone());
    let report = repo.create(reporter.id, Some(post.id), None, "spam".to_string()).await.expect("create");

    repo.update_status(report.id, ReportStatus::Resolved, mod_user.id, Some("handled".to_string()))
        .await.expect("update_status");

    let found = repo.find_by_id(report.id).await.expect("find_by_id").unwrap();
    assert_eq!(found.status, ReportStatus::Resolved);
    assert_eq!(found.resolved_by_id, Some(mod_user.id));
    db.teardown().await;
}

#[tokio::test]
async fn list_all_with_status_filter() {
    let db = TestDb::new("rpt_list_all_filter").await;
    let user = insert_user(&db.conn, 1).await;
    let cat = insert_category(&db.conn, 1).await;
    let thread = insert_thread(&db.conn, 1, cat.id, user.id).await;
    let post_a = insert_post(&db.conn, thread.id, user.id).await;
    let post_b = insert_post(&db.conn, thread.id, user.id).await;
    let reporter = insert_user(&db.conn, 2).await;
    let mod_user = insert_user(&db.conn, 3).await;

    let repo = PgReportRepository::new(db.conn.clone());
    let r1 = repo.create(reporter.id, Some(post_a.id), None, "sp".to_string()).await.expect("r1");
    repo.create(reporter.id, Some(post_b.id), None, "sp".to_string()).await.expect("r2");
    repo.update_status(r1.id, ReportStatus::Dismissed, mod_user.id, None).await.expect("dismiss r1");

    let (pending, total) = repo.list_all(Some(ReportStatus::Pending), None, None, None, 1, 20)
        .await.expect("list pending");
    assert_eq!(total, 1);
    assert_eq!(pending[0].status, ReportStatus::Pending);
    db.teardown().await;
}

/// A category-scoped moderator must not see reports filed against content in a
/// category they are not assigned to — covers both the post-report path (which
/// links to a category via posts -> threads) and the thread-report path.
#[tokio::test]
async fn list_all_scoped_to_categories_excludes_other_categories() {
    let db = TestDb::new("rpt_list_scoped").await;
    let user = insert_user(&db.conn, 1).await;
    let reporter = insert_user(&db.conn, 2).await;

    let cat_a = insert_category(&db.conn, 1).await;
    let cat_b = insert_category(&db.conn, 2).await;
    let thread_a = insert_thread(&db.conn, 1, cat_a.id, user.id).await;
    let thread_b = insert_thread(&db.conn, 2, cat_b.id, user.id).await;
    let post_a = insert_post(&db.conn, thread_a.id, user.id).await;
    let post_b = insert_post(&db.conn, thread_b.id, user.id).await;

    let repo = PgReportRepository::new(db.conn.clone());
    // One post-report and one thread-report in each category.
    let post_rpt_a = repo.create(reporter.id, Some(post_a.id), None, "a".into()).await.expect("pa");
    let thr_rpt_a = repo.create(reporter.id, None, Some(thread_a.id), "a".into()).await.expect("ta");
    repo.create(reporter.id, Some(post_b.id), None, "b".into()).await.expect("pb");
    repo.create(reporter.id, None, Some(thread_b.id), "b".into()).await.expect("tb");

    // Unscoped (admin) sees all four.
    let (_, total_all) = repo.list_all(None, None, None, None, 1, 20).await.expect("all");
    assert_eq!(total_all, 4);

    // Scoped to category A sees only A's two reports.
    let (rows, total) = repo
        .list_all(None, None, None, Some(&[cat_a.id]), 1, 20)
        .await
        .expect("scoped");
    assert_eq!(total, 2, "moderator scoped to cat A must see exactly A's reports");
    let ids: Vec<_> = rows.iter().map(|r| r.id).collect();
    assert!(ids.contains(&post_rpt_a.id), "post report in A must be visible");
    assert!(ids.contains(&thr_rpt_a.id), "thread report in A must be visible");

    // An empty scope (moderator assigned to no categories) must see nothing,
    // not everything — the `IN ()` degenerate case must fail closed.
    let (rows, total) = repo.list_all(None, None, None, Some(&[]), 1, 20).await.expect("empty");
    assert_eq!(total, 0);
    assert!(rows.is_empty());

    db.teardown().await;
}

#[tokio::test]
async fn count_by_status_scoped_to_categories() {
    let db = TestDb::new("rpt_count_scoped").await;
    let user = insert_user(&db.conn, 1).await;
    let reporter = insert_user(&db.conn, 2).await;

    let cat_a = insert_category(&db.conn, 1).await;
    let cat_b = insert_category(&db.conn, 2).await;
    let thread_a = insert_thread(&db.conn, 1, cat_a.id, user.id).await;
    let thread_b = insert_thread(&db.conn, 2, cat_b.id, user.id).await;
    let post_b = insert_post(&db.conn, thread_b.id, user.id).await;

    let repo = PgReportRepository::new(db.conn.clone());
    repo.create(reporter.id, None, Some(thread_a.id), "a".into()).await.expect("ta");
    repo.create(reporter.id, Some(post_b.id), None, "b".into()).await.expect("pb");

    assert_eq!(repo.count_by_status(None).await.expect("all").pending, 2);
    assert_eq!(
        repo.count_by_status(Some(&[cat_a.id])).await.expect("scoped").pending,
        1,
        "pending count must exclude reports from unassigned categories"
    );
    assert_eq!(repo.count_by_status(Some(&[])).await.expect("empty").pending, 0);

    db.teardown().await;
}
