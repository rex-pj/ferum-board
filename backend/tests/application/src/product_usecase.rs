//! Ownership rules for catalogue products.
//!
//! The rule under test: curators (`product.manage`) may edit anything; a
//! contributor (`product.submit`) may edit only a product they submitted, and
//! only while it is still an unapproved `draft`. Once approved it is shared
//! catalogue content that other people's reviews hang off.
//!
//! The self-approval case is the one that matters most — a contributor who can
//! edit their own draft must not be able to PATCH it to `published` and walk
//! past the moderation queue.

use std::sync::Arc;

use ferum_application::usecases::product_usecase::ProductUseCase;
use ferum_domain::models::product::ProductStatus;
use ferum_domain::repositories::product_repository::UpdateProduct;
use ferum_domain::{AppError, AuthUser};
use ferum_test_support::fixtures::{ids, make_product, AuthUserBuilder};
use ferum_test_support::mocks::{
    brand_repository::MockBrandRepository, job_queue::MockJobQueue,
    material_repository::MockMaterialRepository,
    product_category_repository::{MockProductCategoryRepository, NoopProductCategoryRepository},
    product_repository::MockProductRepository,
    stored_file_repository::NoopStoredFileRepository,
};

// ─── Builder ──────────────────────────────────────────────────────────────────

fn build(products: MockProductRepository) -> ProductUseCase {
    build_with_categories(products, NoopProductCategoryRepository)
}

fn build_with_categories(
    products: MockProductRepository,
    categories: impl ferum_domain::repositories::product_repository::ProductCategoryRepository
        + 'static,
) -> ProductUseCase {
    ProductUseCase::new(
        Arc::new(products),
        Arc::new(categories),
        Arc::new(MockMaterialRepository::new()),
        Arc::new(MockBrandRepository::new()),
        Arc::new(NoopStoredFileRepository),
        Arc::new(MockJobQueue::new()),
    )
}

fn curator() -> AuthUser {
    AuthUserBuilder::member()
        .with_id(ids::user_a())
        .with_perms(&["product.manage"])
        .build()
}

fn contributor(id: uuid::Uuid) -> AuthUser {
    AuthUserBuilder::member().with_id(id).with_perms(&["product.submit"]).build()
}

fn rename() -> UpdateProduct {
    UpdateProduct { name: Some("Renamed".into()), ..Default::default() }
}

// ─── update: who may edit ─────────────────────────────────────────────────────

#[tokio::test]
async fn update_own_draft_is_allowed_for_contributor() {
    let actor = contributor(ids::user_b());
    let product = make_product(ids::product_a(), Some(ids::user_b()), ProductStatus::Draft);
    let returned = product.clone();

    let mut products = MockProductRepository::new();
    products.expect_find_by_id().return_once(move |_| Ok(Some(product)));
    products.expect_update().return_once(move |_, _| Ok(returned));

    let result = build(products).update(&actor, ids::product_a(), rename()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn update_own_published_product_is_forbidden_for_contributor() {
    // Approved: other people's reviews now hang off it, so it stopped being theirs.
    let actor = contributor(ids::user_b());
    let product = make_product(ids::product_a(), Some(ids::user_b()), ProductStatus::Published);

    let mut products = MockProductRepository::new();
    products.expect_find_by_id().return_once(move |_| Ok(Some(product)));
    // expect_update NOT set — must not be reached

    let result = build(products).update(&actor, ids::product_a(), rename()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn update_someone_elses_draft_is_forbidden_for_contributor() {
    let actor = contributor(ids::user_b());
    let product = make_product(ids::product_a(), Some(ids::user_a()), ProductStatus::Draft);

    let mut products = MockProductRepository::new();
    products.expect_find_by_id().return_once(move |_| Ok(Some(product)));

    let result = build(products).update(&actor, ids::product_a(), rename()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn update_published_product_is_allowed_for_curator() {
    let actor = curator();
    let product = make_product(ids::product_a(), Some(ids::user_b()), ProductStatus::Published);
    let returned = product.clone();

    let mut products = MockProductRepository::new();
    products.expect_find_by_id().return_once(move |_| Ok(Some(product)));
    products.expect_update().return_once(move |_, _| Ok(returned));

    let result = build(products).update(&actor, ids::product_a(), rename()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn update_missing_product_returns_not_found() {
    let actor = curator();
    let mut products = MockProductRepository::new();
    products.expect_find_by_id().return_once(|_| Ok(None));

    let result = build(products).update(&actor, ids::product_a(), rename()).await;
    assert!(matches!(result, Err(AppError::NotFound)));
}

// ─── update: what a contributor may change ────────────────────────────────────

#[tokio::test]
async fn contributor_cannot_publish_their_own_draft() {
    // The privilege-escalation case: editing your own draft must not let you
    // approve it. Without `reject_curator_only_fields` this PATCH succeeds and
    // the moderation queue is bypassed entirely.
    let actor = contributor(ids::user_b());
    let product = make_product(ids::product_a(), Some(ids::user_b()), ProductStatus::Draft);

    let mut products = MockProductRepository::new();
    products.expect_find_by_id().return_once(move |_| Ok(Some(product)));
    // expect_update NOT set — reaching the repository at all is the bug

    let patch = UpdateProduct { status: Some(ProductStatus::Published), ..Default::default() };
    let result = build(products).update(&actor, ids::product_a(), patch).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn contributor_cannot_recategorise_their_own_draft() {
    let actor = contributor(ids::user_b());
    let product = make_product(ids::product_a(), Some(ids::user_b()), ProductStatus::Draft);

    let mut products = MockProductRepository::new();
    products.expect_find_by_id().return_once(move |_| Ok(Some(product)));

    let patch =
        UpdateProduct { category_id: Some(Some(ids::category_a())), ..Default::default() };
    let result = build(products).update(&actor, ids::product_a(), patch).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn curator_may_still_change_status() {
    let actor = curator();
    let product = make_product(ids::product_a(), Some(ids::user_b()), ProductStatus::Draft);
    let returned = product.clone();

    let mut products = MockProductRepository::new();
    products.expect_find_by_id().return_once(move |_| Ok(Some(product)));
    products.expect_update().return_once(move |_, _| Ok(returned));

    let patch = UpdateProduct { status: Some(ProductStatus::Published), ..Default::default() };
    let result = build(products).update(&actor, ids::product_a(), patch).await;
    assert!(result.is_ok());
}

// ─── delete ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_own_draft_is_allowed_for_contributor() {
    let actor = contributor(ids::user_b());
    let product = make_product(ids::product_a(), Some(ids::user_b()), ProductStatus::Draft);

    let mut products = MockProductRepository::new();
    products.expect_find_by_id().return_once(move |_| Ok(Some(product)));
    products.expect_count_dependents().return_once(|_| {
        Ok(ferum_domain::repositories::product_repository::ProductDependents {
            reviews: 0,
            media: 0,
            materials: 0,
        })
    });
    products.expect_list_media().return_once(|_| Ok(vec![]));
    products.expect_delete().return_once(|_| Ok(()));

    let result = build(products).delete(&actor, ids::product_a()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn delete_own_published_product_is_forbidden_for_contributor() {
    let actor = contributor(ids::user_b());
    let product = make_product(ids::product_a(), Some(ids::user_b()), ProductStatus::Published);

    let mut products = MockProductRepository::new();
    products.expect_find_by_id().return_once(move |_| Ok(Some(product)));
    // expect_delete NOT set

    let result = build(products).delete(&actor, ids::product_a()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn delete_is_refused_while_reviews_reference_the_product() {
    // Guards both roles: `threads.product_id` is ON DELETE SET NULL, so deleting
    // would leave reviews pointing at nothing.
    let actor = curator();
    let product = make_product(ids::product_a(), Some(ids::user_b()), ProductStatus::Published);

    let mut products = MockProductRepository::new();
    products.expect_find_by_id().return_once(move |_| Ok(Some(product)));
    products.expect_count_dependents().return_once(|_| {
        Ok(ferum_domain::repositories::product_repository::ProductDependents {
            reviews: 3,
            media: 0,
            materials: 0,
        })
    });
    // expect_delete NOT set

    let result = build(products).delete(&actor, ids::product_a()).await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
}

// ─── banned users ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn banned_owner_cannot_edit_their_own_draft() {
    let actor = AuthUserBuilder::member()
        .with_id(ids::user_b())
        .with_perms(&["product.submit"])
        .banned()
        .build();
    let product = make_product(ids::product_a(), Some(ids::user_b()), ProductStatus::Draft);

    let mut products = MockProductRepository::new();
    products.expect_find_by_id().return_once(move |_| Ok(Some(product)));

    let result = build(products).update(&actor, ids::product_a(), rename()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}
