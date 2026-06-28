use std::sync::Arc;

use ferum_application::usecases::category_usecase::CategoryUseCase;
use ferum_domain::AppError;
use ferum_domain::models::category::{PostPolicy, ViewPolicy};
use ferum_test_support::fixtures::{ids, make_category, AuthUserBuilder};
use ferum_test_support::mocks::{
    category_repository::MockCategoryRepository,
    tag_repository::MockTagRepository,
    thread_repository::MockThreadRepository,
    user_repository::MockUserRepository,
};

fn build_uc(
    categories: MockCategoryRepository,
    threads: MockThreadRepository,
    tags: MockTagRepository,
    users: MockUserRepository,
) -> CategoryUseCase {
    CategoryUseCase::new(Arc::new(categories), Arc::new(threads), Arc::new(tags), Arc::new(users))
}

// ─── list_visible ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_visible_guest_sees_only_public_categories() {
    let mut public_cat = make_category(ids::category_a());
    public_cat.view_policy = ViewPolicy::Public;

    let mut members_cat = make_category(ids::user_a());
    members_cat.view_policy = ViewPolicy::MembersOnly;

    let mut staff_cat = make_category(ids::user_b());
    staff_cat.view_policy = ViewPolicy::StaffOnly;

    let mut cats = MockCategoryRepository::new();
    cats.expect_list_all()
        .return_once(move || Ok(vec![public_cat, members_cat, staff_cat]));

    let uc = build_uc(cats, MockThreadRepository::new(), MockTagRepository::new(), MockUserRepository::new());
    let visible = uc.list_visible(None).await.expect("list_visible succeeds");

    assert_eq!(visible.len(), 1, "guest should only see public categories");
    assert_eq!(visible[0].view_policy, ViewPolicy::Public);
}

#[tokio::test]
async fn list_visible_member_sees_public_and_members_only() {
    let actor = AuthUserBuilder::member().build();
    let mut public_cat = make_category(ids::category_a());
    public_cat.view_policy = ViewPolicy::Public;
    let mut members_cat = make_category(ids::user_a());
    members_cat.view_policy = ViewPolicy::MembersOnly;
    let mut staff_cat = make_category(ids::user_b());
    staff_cat.view_policy = ViewPolicy::StaffOnly;

    let mut cats = MockCategoryRepository::new();
    cats.expect_list_all()
        .return_once(move || Ok(vec![public_cat, members_cat, staff_cat]));

    let uc = build_uc(cats, MockThreadRepository::new(), MockTagRepository::new(), MockUserRepository::new());
    let visible = uc.list_visible(Some(&actor)).await.expect("list_visible succeeds");

    assert_eq!(visible.len(), 2, "member should see public + members_only");
}

#[tokio::test]
async fn list_visible_admin_sees_all_categories() {
    let actor = AuthUserBuilder::admin().build();
    let mut public_cat = make_category(ids::category_a());
    public_cat.view_policy = ViewPolicy::Public;
    let mut staff_cat = make_category(ids::user_b());
    staff_cat.view_policy = ViewPolicy::StaffOnly;

    let mut cats = MockCategoryRepository::new();
    cats.expect_list_all()
        .return_once(move || Ok(vec![public_cat, staff_cat]));

    let uc = build_uc(cats, MockThreadRepository::new(), MockTagRepository::new(), MockUserRepository::new());
    let visible = uc.list_visible(Some(&actor)).await.expect("list_visible succeeds");

    assert_eq!(visible.len(), 2, "admin should see all categories including staff_only");
}

// ─── get_by_slug ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_by_slug_not_found_returns_404() {
    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_slug().return_once(|_| Ok(None));

    let uc = build_uc(cats, MockThreadRepository::new(), MockTagRepository::new(), MockUserRepository::new());
    let result = uc.get_by_slug(None, "nonexistent").await;

    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn get_by_slug_staff_only_returns_404_to_guest() {
    // NF-SC-13: staff_only category → 404 (not 403) to guests
    let mut staff_cat = make_category(ids::category_a());
    staff_cat.view_policy = ViewPolicy::StaffOnly;

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_slug().return_once(move |_| Ok(Some(staff_cat)));

    let uc = build_uc(cats, MockThreadRepository::new(), MockTagRepository::new(), MockUserRepository::new());
    let result = uc.get_by_slug(None, "staff-area").await;

    assert!(matches!(result, Err(AppError::NotFound)), "staff_only must 404 guests, not 403");
}

#[tokio::test]
async fn get_by_slug_staff_only_returns_404_to_regular_member() {
    let actor = AuthUserBuilder::member().build();
    let mut staff_cat = make_category(ids::category_a());
    staff_cat.view_policy = ViewPolicy::StaffOnly;

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_slug().return_once(move |_| Ok(Some(staff_cat)));

    let uc = build_uc(cats, MockThreadRepository::new(), MockTagRepository::new(), MockUserRepository::new());
    let result = uc.get_by_slug(Some(&actor), "staff-area").await;

    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn get_by_slug_public_category_visible_to_guest() {
    let cat = make_category(ids::category_a());
    let cat_id = cat.id;

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_slug().return_once(move |_| Ok(Some(cat)));

    let uc = build_uc(cats, MockThreadRepository::new(), MockTagRepository::new(), MockUserRepository::new());
    let result = uc.get_by_slug(None, "general").await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().id, cat_id);
}

#[tokio::test]
async fn get_by_slug_members_only_returns_not_found_to_guest() {
    let mut cat = make_category(ids::category_a());
    cat.view_policy = ViewPolicy::MembersOnly;

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_slug().return_once(move |_| Ok(Some(cat)));

    let uc = build_uc(cats, MockThreadRepository::new(), MockTagRepository::new(), MockUserRepository::new());
    // Guest cannot see members_only — returns Unauthorized (only staff_only returns NotFound)
    let result = uc.get_by_slug(None, "members").await;

    assert!(matches!(result, Err(AppError::Unauthorized)));
}

// ─── watch / mute ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn watch_category_on_non_existent_returns_not_found() {
    let actor = AuthUserBuilder::member().build();

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_id().return_once(|_| Ok(None));

    let uc = build_uc(cats, MockThreadRepository::new(), MockTagRepository::new(), MockUserRepository::new());
    let result = uc.watch_category(&actor, ids::category_a()).await;

    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn watch_staff_only_category_returns_not_found_to_member() {
    let actor = AuthUserBuilder::member().build();
    let mut cat = make_category(ids::category_a());
    cat.view_policy = ViewPolicy::StaffOnly;

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_id().return_once(move |_| Ok(Some(cat)));

    let uc = build_uc(cats, MockThreadRepository::new(), MockTagRepository::new(), MockUserRepository::new());
    let result = uc.watch_category(&actor, ids::category_a()).await;

    assert!(matches!(result, Err(AppError::NotFound)));
}

#[tokio::test]
async fn watch_public_category_succeeds() {
    let actor = AuthUserBuilder::member().build();
    let cat = make_category(ids::category_a());

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_id().return_once(move |_| Ok(Some(cat)));

    let mut users = MockUserRepository::new();
    users.expect_watch_category().return_once(|_, _| Ok(()));

    let uc = build_uc(cats, MockThreadRepository::new(), MockTagRepository::new(), users);
    assert!(uc.watch_category(&actor, ids::category_a()).await.is_ok());
}

#[tokio::test]
async fn post_policy_closed_category_does_not_affect_view() {
    // post_policy is orthogonal to view_policy — a closed category is still visible
    let actor = AuthUserBuilder::member().build();
    let mut cat = make_category(ids::category_a());
    cat.view_policy = ViewPolicy::Public;
    cat.post_policy = PostPolicy::Closed;

    let mut cats = MockCategoryRepository::new();
    cats.expect_find_by_slug().return_once(move |_| Ok(Some(cat)));

    let uc = build_uc(cats, MockThreadRepository::new(), MockTagRepository::new(), MockUserRepository::new());
    assert!(uc.get_by_slug(Some(&actor), "general").await.is_ok());
}
