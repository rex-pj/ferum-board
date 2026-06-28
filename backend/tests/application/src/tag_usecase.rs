use std::sync::Arc;

use uuid::Uuid;
use ferum_application::usecases::tag_usecase::TagUseCase;
use ferum_domain::AppError;
use ferum_domain::models::tag::{NewTag, Tag};
use ferum_test_support::fixtures::AuthUserBuilder;
use ferum_test_support::mocks::tag_repository::MockTagRepository;

fn make_tag(name: &str) -> Tag {
    Tag {
        id: Uuid::new_v4(),
        name: name.to_string(),
        slug: slug::slugify(name),
        color: None,
        created_by_id: None,
        created_at: chrono::Utc::now(),
    }
}

// ─── resolve_or_create ─────────────────────────────────────────────────────

#[tokio::test]
async fn resolve_existing_tag_without_perm_succeeds() {
    let existing = make_tag("rust-lang");
    let mut tags = MockTagRepository::new();
    tags.expect_find_by_slug().returning(move |_| Ok(Some(existing.clone())));

    let uc = TagUseCase::new(Arc::new(tags));
    // actor has NO tag.create permission — but tag already exists
    let actor = AuthUserBuilder::member().build();
    let result = uc.resolve_or_create(&actor, vec!["Rust Lang".to_string()], 5).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 1);
}

#[tokio::test]
async fn create_new_tag_without_perm_returns_403() {
    let mut tags = MockTagRepository::new();
    tags.expect_find_by_slug().returning(|_| Ok(None));

    let uc = TagUseCase::new(Arc::new(tags));
    let actor = AuthUserBuilder::member().build(); // no tag.create perm
    let result = uc.resolve_or_create(&actor, vec!["New Tag".to_string()], 5).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn create_new_tag_with_perm_succeeds() {
    let created = make_tag("new-tag");
    let mut tags = MockTagRepository::new();
    tags.expect_find_by_slug().returning(|_| Ok(None));
    tags.expect_create().returning(move |_| Ok(created.clone()));

    let uc = TagUseCase::new(Arc::new(tags));
    let actor = AuthUserBuilder::member().with_perm("tag.create").build();
    let result = uc.resolve_or_create(&actor, vec!["New Tag".to_string()], 5).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 1);
}

#[tokio::test]
async fn resolve_or_create_respects_max_limit() {
    let tag1 = make_tag("tag-one");
    let tag2 = make_tag("tag-two");
    let mut tags = MockTagRepository::new();
    // Both slugs exist
    {
        let t1 = tag1.clone();
        let t2 = tag2.clone();
        tags.expect_find_by_slug()
            .returning(move |slug| {
                if slug == "tag-one" { Ok(Some(t1.clone())) }
                else { Ok(Some(t2.clone())) }
            });
    }

    let uc = TagUseCase::new(Arc::new(tags));
    let actor = AuthUserBuilder::member().build();
    let result = uc.resolve_or_create(&actor, vec!["Tag One".to_string(), "Tag Two".to_string(), "Tag Three".to_string()], 2).await;
    // max=2, so only first 2 are processed
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 2);
}

#[tokio::test]
async fn empty_name_is_skipped() {
    let tags = MockTagRepository::new();
    let uc = TagUseCase::new(Arc::new(tags));
    let actor = AuthUserBuilder::member().build();
    // "  " trims to empty string → skipped
    let result = uc.resolve_or_create(&actor, vec!["  ".to_string()], 5).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}
