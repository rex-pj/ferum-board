//! Integration tests for [`PgEmailTemplateRepository`].
//!
//! Two things here the compiler cannot check, and both are how the feature
//! silently stops being multilingual: that `resolve` honours the *chain's*
//! order rather than the planner's, and that the startup seeder never writes to
//! this table at all — a row here means "an admin edited this".

use ferum_domain::models::email_template::{EMAIL_TEMPLATES, EMAIL_TEMPLATE_DEFAULTS, LAYOUT_KEY};
use ferum_domain::repositories::email_template_repository::EmailTemplateRepository;
use ferum_infrastructure::repositories::email_template_repository::PgEmailTemplateRepository;
use ferum_infrastructure::system_seed_service::PgSystemSeedService;

use crate::common::TestDb;

fn chain(tags: &[&str]) -> Vec<String> {
    tags.iter().map(|s| s.to_string()).collect()
}

/// **The table starts empty, and that is the contract the editor is built on.**
///
/// A row means "an admin edited this", so pre-seeding defaults would make every
/// template report as customised and would freeze each install on the copy that
/// shipped when it first booted. Unedited copy resolves from the compiled-in
/// catalogue instead.
#[tokio::test]
async fn a_fresh_database_stores_no_template_rows() {
    let db = TestDb::new("email_tpl_empty").await;
    let repo = PgEmailTemplateRepository::new(db.conn.clone());

    assert!(
        repo.list().await.expect("list runs").is_empty(),
        "seeding defaults here would make the customised badge meaningless"
    );
    assert!(
        !EMAIL_TEMPLATE_DEFAULTS.is_empty(),
        "the compiled-in catalogue is what unedited copy resolves from"
    );
}

/// Every template the catalogue declares must ship copy in the default locale,
/// or the first send in a fresh install falls through to nothing.
#[tokio::test]
async fn every_declared_template_has_english_copy() {
    for t in EMAIL_TEMPLATES {
        assert!(
            EMAIL_TEMPLATE_DEFAULTS
                .iter()
                .any(|d| d.key == t.key && d.locale == "en"),
            "{} declares no en default",
            t.key
        );
    }
}

/// The property that keeps this multilingual. A regional locale with no row of
/// its own must inherit its base language, and only then the default — picked
/// in the chain's order, never the order the rows came back in.
#[tokio::test]
async fn resolve_takes_the_first_locale_in_chain_order() {
    let db = TestDb::new("email_tpl_chain").await;
    let repo = PgEmailTemplateRepository::new(db.conn.clone());

    // Both a base language and the default carry edited copy; the regional
    // locale carries none.
    repo.upsert("email-verify", "vi", "vi subject", "<p>{{ url }}</p>")
        .await
        .unwrap();
    repo.upsert("email-verify", "en", "en subject", "<p>{{ url }}</p>")
        .await
        .unwrap();

    // `Locale::fallback_chain` yields ["vi-VN", "vi", "en"], and `vi` must win
    // over `en` — matching in the chain's order, not the planner's.
    let hit = repo
        .resolve("email-verify", &chain(&["vi-VN", "vi", "en"]))
        .await
        .expect("resolve runs")
        .expect("inherits the base language rather than missing");
    assert_eq!(hit.locale, "vi", "vi-VN must inherit vi, not jump to en");

    // With the base absent from the chain, the same lookup lands on the default.
    let hit = repo
        .resolve("email-verify", &chain(&["de", "en"]))
        .await
        .expect("resolve runs")
        .expect("falls through to the default locale");
    assert_eq!(hit.locale, "en");
}

#[tokio::test]
async fn resolve_is_none_for_an_unknown_key() {
    let db = TestDb::new("email_tpl_unknown").await;
    let repo = PgEmailTemplateRepository::new(db.conn.clone());

    let hit = repo
        .resolve("email-does-not-exist", &chain(&["en"]))
        .await
        .expect("resolve runs");
    assert!(hit.is_none(), "an unknown key must not resolve to anything");
}

/// The seeder runs on every boot, and must neither create template rows nor
/// touch an edited one.
#[tokio::test]
async fn the_startup_seeder_leaves_this_table_alone() {
    let db = TestDb::new("email_tpl_reseed").await;
    let repo = PgEmailTemplateRepository::new(db.conn.clone());

    repo.upsert("email-verify", "en", "Edited by the admin", "<p>Edited {{ url }}</p>")
        .await
        .expect("admin edit saves");

    PgSystemSeedService::new(db.conn.clone())
        .seed_system()
        .await
        .expect("re-seeding a populated database succeeds");

    let rows = repo.list().await.expect("list runs");
    assert_eq!(
        rows.len(),
        1,
        "the seeder must not add template rows, or every one reads as customised"
    );
    assert_eq!(
        rows[0].subject, "Edited by the admin",
        "the seeder must not reclaim a row the admin edited"
    );
}


/// "Restore default" is a DELETE, so nothing has to store a second pristine
/// copy — the compiled-in catalogue is what resolves afterwards.
#[tokio::test]
async fn delete_drops_only_the_one_locale() {
    let db = TestDb::new("email_tpl_delete").await;
    let repo = PgEmailTemplateRepository::new(db.conn.clone());

    repo.upsert("email-verify", "vi", "vi", "<p>{{ url }}</p>").await.unwrap();
    repo.upsert("email-verify", "en", "en", "<p>{{ url }}</p>").await.unwrap();

    repo.delete("email-verify", "vi").await.expect("delete runs");

    assert!(
        repo.find("email-verify", "vi").await.unwrap().is_none(),
        "the targeted row is gone"
    );
    assert!(
        repo.find("email-verify", "en").await.unwrap().is_some(),
        "a sibling locale must be untouched"
    );
}

/// The layout is editable through the same table, but ships a default like
/// everything else rather than being stored up front.
#[tokio::test]
async fn the_layout_is_editable_and_ships_a_content_slot() {
    let db = TestDb::new("email_tpl_layout").await;
    let repo = PgEmailTemplateRepository::new(db.conn.clone());

    let shipped = ferum_domain::models::email_template::template_default(LAYOUT_KEY, "en")
        .expect("the layout ships a default");
    assert!(
        shipped.body_html.contains("{{ content }}"),
        "the layout must expose a content slot, got: {}",
        shipped.body_html
    );

    repo.upsert(LAYOUT_KEY, "en", "-", "<div>{{ content }}</div>")
        .await
        .expect("the layout saves like any other template");
    assert!(repo.find(LAYOUT_KEY, "en").await.unwrap().is_some());
}
