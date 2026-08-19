//! Renders the search results page and the catalogue browse page for real,
//! against the shipped templates and catalogs.
//!
//! `tera_templates.rs` proves the template *parses*; that is not the same thing.
//! The tab strip, the product preview section and the recovery links are all
//! conditional logic over context the handler supplies, and every way of getting
//! that wrong — a missing variable, `| length` on an absent list, a macro called
//! without `self::` — fails at render, not at parse. This exercises each tab
//! state end to end so those failures surface here rather than on the page.

use std::path::PathBuf;

use ferum_domain::Locale;
use ferum_web::view_models::page_context::{PaginationCtx, SiteCtx};
use serde_json::json;
use tera::Context;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

async fn engine() -> ferum_web::tera_engine::TeraEngine {
    let frontend = repo_root().join("frontend");
    let translator = std::sync::Arc::new(
        ferum_infrastructure::i18n::FluentTranslator::new(vec![repo_root().join("locales")]).await,
    );
    ferum_web::tera_engine::TeraEngine::new(
        frontend.join("themes"),
        frontend.join("templates"),
        frontend.join("static"),
        translator,
    )
    .expect("TeraEngine::new must succeed")
}

/// Mirrors what `handlers::pages::search` inserts, plus the shell variables
/// `render_with_theme_in` adds on every page. Built from the real context
/// structs where they are public, so a field renamed on `SiteCtx` or
/// `PaginationCtx` breaks this file rather than silently diverging from it.
fn base_ctx() -> Context {
    let mut ctx = Context::new();
    ctx.insert(
        "site",
        &SiteCtx {
            name: "Ferum".into(),
            slogan: "Reviews".into(),
            tagline: "Furniture reviews".into(),
            logo_url: None,
            favicon_url: None,
            primary_color: None,
            primary_color_rgb: None,
            url: "http://localhost:5173".into(),
        },
    );
    ctx.insert("current_user", &json!(null));
    ctx.insert("active_theme", "default");
    ctx.insert("nav_categories", &json!([]));
    ctx.insert("search_categories", &json!([]));
    ctx.insert("active_category_id", &json!(null));
    ctx.insert("query", "sofa");
    ctx.insert("query_encoded", "sofa");
    ctx.insert("results", &json!([]));
    ctx.insert("products", &json!([]));
    ctx.insert("thread_total", &0u64);
    ctx.insert("product_total", &0u64);
    ctx.insert("total_all", &0u64);
    ctx.insert("active_tab", "all");
    ctx.insert("search_error", &false);
    ctx.insert("can_submit_product", &false);
    ctx.insert(
        "pagination",
        &PaginationCtx::new(1, 20, 0, "&q=sofa".to_string()),
    );

    ctx.insert("brands", &json!([{ "id": "00000000-0000-0000-0000-0000000000b1", "name": "Milano" }]));
    ctx.insert("materials", &json!([{ "id": "00000000-0000-0000-0000-0000000000c1", "name": "Oak" }]));
    ctx.insert("active_type", &json!(null));
    ctx.insert("active_brand", &json!(null));
    ctx.insert("active_material", &json!(null));
    ctx.insert("active_psort", "relevance");
    ctx.insert("active_tsort", "relevance");
    ctx.insert("active_pcat", &json!(null));
    ctx.insert(
        "product_categories",
        &json!([{ "id": "00000000-0000-0000-0000-0000000000d1", "name": "Sofa", "is_child": false }]),
    );
    ctx.insert("product_filters_active", &false);
    ctx.insert("filter_chips", &json!([]));
    ctx.insert("facet_params", "");
    ctx.insert("relaxed", &json!({ "without_category": null, "without_facets": null }));
    ctx.insert("has_relaxation", &false);

    // The page shell, as injected by `render_with_theme_in`.
    ctx.insert("plugin_slots", &json!({}));
    ctx.insert("plugin_assets", &json!([]));
    ctx.insert("theme_bs_theme", "auto");
    ctx.insert("locale", "en");
    ctx.insert("current_path", "/search");
    ctx.insert("default_locale", "en");
    ctx.insert("js_strings", &json!({}));
    ctx.insert("available_locales", &json!(["en", "vi"]));
    ctx
}

fn product(name: &str, slug: &str) -> serde_json::Value {
    json!({
        "slug": slug,
        "name": name,
        "excerpt": null,
        "product_type": "furniture",
        "style": "scandinavian",
        "primary_image_key": null,
        "price_min": 12_000_000,
        "price_max": 18_000_000,
        "currency": "VND",
        "review_count": 34,
        "avg_overall": 4.6
    })
}

fn thread_hit(title: &str, slug: &str) -> serde_json::Value {
    json!({
        "thread_slug": slug,
        "thread_title": title,
        "excerpt": "a <b>sofa</b> review",
        "category_slug": "reviews",
        "category_name": "Reviews",
        "author_username": "alice",
        "author_display_name": "Alice",
        "created_at": "2026-07-01T10:00:00Z",
        "reply_count": 3
    })
}

async fn render(ctx: Context) -> String {
    engine()
        .await
        .render(&Locale::default_locale(), "default/templates/search.html", ctx)
        .await
        .expect("search.html must render")
}

#[tokio::test]
async fn renders_the_all_tab_with_both_kinds() {
    let mut ctx = base_ctx();
    ctx.insert("products", &json!([product("Milano Sofa", "milano-sofa")]));
    ctx.insert("results", &json!([thread_hit("Sofa after 2 years", "sofa-2y")]));
    ctx.insert("product_total", &12u64);
    ctx.insert("thread_total", &25u64);
    ctx.insert("total_all", &37u64);

    let html = render(ctx).await;

    assert!(html.contains("Milano Sofa"), "product card must render");
    assert!(html.contains("Sofa after 2 years"), "thread row must render");
    // The counts on the tabs are the whole reason the strip exists.
    assert!(html.contains(">12<"), "product tab badge");
    assert!(html.contains(">25<"), "discussion tab badge");
    // A preview of 1 out of 12 must offer the way to the rest.
    assert!(html.contains("tab=products"), "link into the full product set");
}

/// The empty product preview must not leave a dangling section header, and the
/// rating line must survive an unrated product (`avg_overall` null) — the
/// catalogue card reads both fields.
#[tokio::test]
async fn renders_products_tab_with_an_unrated_product() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "products");
    let mut unrated = product("Oak Table", "oak-table");
    unrated["review_count"] = json!(0);
    unrated["avg_overall"] = json!(null);
    unrated["price_min"] = json!(null);
    unrated["price_max"] = json!(null);
    ctx.insert("products", &json!([unrated]));
    ctx.insert("product_total", &1u64);

    let html = render(ctx).await;

    assert!(html.contains("Oak Table"));
    assert!(
        html.contains("No reviews yet"),
        "an unrated product says so rather than showing a blank score"
    );

    // Sort orders results; it is not a filter. It renders on the results header
    // (only reachable once there are products to sort), styled as .fr-sort-select
    // — borderless, distinct from the bordered facet boxes — and positioned
    // AFTER the filter toolbar, never inside it. This is the fix for a sort
    // control that read as a fifth filter and wrapped orphaned onto its own row.
    assert!(html.contains(r#"name="psort""#), "sort control renders with results");
    assert!(
        html.contains(r#"class="fr-sort-select""#),
        "sort uses the distinct, borderless style, not a filter box"
    );
    // The control renders in the results area, OUTSIDE the <form> in the search
    // head, so it must associate to the form by id — otherwise its @change
    // submit is dead and selecting a sort option does nothing.
    assert!(
        html.contains(r#"form="searchForm""#),
        "sort select associates to the form by id, since it renders outside it"
    );
    let refine = html.find("fr-refine-controls").expect("filter toolbar renders");
    let sort = html.find(r#"name="psort""#).expect("sort renders");
    assert!(
        refine < sort,
        "sort sits with the results, after the filter toolbar — not among the facets"
    );
}

/// A miss on one tab points at the other. This is the recovery path, and it is
/// pure conditional logic over the two totals.
#[tokio::test]
async fn empty_product_tab_offers_the_matching_discussions() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "products");
    ctx.insert("thread_total", &25u64);
    ctx.insert("total_all", &25u64);

    let html = render(ctx).await;

    assert!(html.contains("tab=threads"), "cross-tab recovery link");
    assert!(html.contains("25 matching discussions"));
    assert!(
        !html.contains(r#"name="psort""#),
        "no sort control when there are no products to sort"
    );
}

/// A contributor who searches for a product that does not exist is offered the
/// action that creates it, pre-filled with what they typed.
#[tokio::test]
async fn empty_results_offer_product_submission_when_permitted() {
    let mut ctx = base_ctx();
    ctx.insert("can_submit_product", &true);

    let html = render(ctx).await;

    assert!(html.contains("add_product=1"), "submission CTA is offered");
    assert!(html.contains("name=sofa"), "pre-filled with the query");
}

#[tokio::test]
async fn search_backend_failure_is_not_reported_as_no_results() {
    let mut ctx = base_ctx();
    ctx.insert("search_error", &true);

    let html = render(ctx).await;

    assert!(html.contains("temporarily unavailable"));
    assert!(
        !html.contains("No results found"),
        "an outage must not tell the user their content does not exist"
    );
}

/// The page must render in every installed locale, not just the source one — a
/// key present in `en` and missing in `vi` renders as the raw key.
#[tokio::test]
async fn renders_in_vietnamese() {
    let mut ctx = base_ctx();
    ctx.insert("products", &json!([product("Ghế ăn Bắc Âu", "ghe-an")]));
    ctx.insert("product_total", &1u64);

    let html = engine()
        .await
        .render(&Locale::parse("vi").expect("vi is a valid tag"), "default/templates/search.html", ctx)
        .await
        .expect("search.html must render in vi");

    assert!(html.contains("Ghế ăn Bắc Âu"));
    assert!(
        !html.contains("ui-search-tab-"),
        "an unresolved key renders as the raw key: {}",
        html.lines()
            .find(|l| l.contains("ui-search-tab-"))
            .unwrap_or_default()
    );
}

// ─── tab-aware filter bar ─────────────────────────────────────────────────────

fn with_one_category(ctx: &mut Context) {
    ctx.insert("search_categories", &json!([{
        "id": "00000000-0000-0000-0000-000000000010", "parent_id": null,
        "slug": "reviews", "name": "Reviews", "description": null, "color": null,
        "thread_count": 0, "view_policy": "public", "post_policy": "members", "can_post": false
    }]));
}

/// Two taxonomies, one page. The *forum* tree (Off-Topic, Programming) files
/// conversation and belongs on the discussion tabs; the *catalogue* tree (Sofa,
/// Ghế, Bàn) files furniture and belongs on the product tab. Offering the forum
/// tree over products is the conflation this split exists to undo — no forum
/// category is a sensible home for a sofa.
#[tokio::test]
async fn products_tab_shows_the_catalogue_taxonomy_not_the_forum_one() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "products");
    with_one_category(&mut ctx);

    let html = render(ctx).await;

    assert!(html.contains(r#"name="pcat""#), "catalogue category is offered");
    assert!(html.contains(r#"name="brand_id""#), "brand facet is offered");
    assert!(html.contains(r#"name="material_id""#), "material facet is offered");
    assert!(
        !html.contains(r#"id="searchCategory""#),
        "the forum category tree must not be offered over products"
    );
}

#[tokio::test]
async fn discussions_tab_keeps_the_category_filter_but_hides_product_facets() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "threads");
    with_one_category(&mut ctx);

    let html = render(ctx).await;

    assert!(html.contains("id=\"searchCategory\""), "category filter is offered");
    assert!(
        !html.contains(r#"<select name="brand_id""#),
        "catalogue facets are not visible controls here"
    );
}

/// Discussions gained their own ordering: relevance (default), newest, and most
/// replies. Like the product sort it renders in the results header, outside the
/// <form>, so it must associate by id.
#[tokio::test]
async fn discussions_tab_offers_a_sort_control() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "threads");
    ctx.insert("results", &json!([thread_hit("Sofa after 2 years", "sofa-2y")]));
    ctx.insert("thread_total", &1u64);

    let html = render(ctx).await;

    assert!(html.contains(r#"name="tsort""#), "discussions can be sorted");
    assert!(html.contains(r#"id="searchTsort""#), "the thread sort control renders");
    assert!(
        html.contains(r#"form="searchForm""#),
        "thread sort associates to the form by id, since it renders outside it"
    );
    assert!(html.contains("Most replies"), "the most-replies ordering is offered");
}

/// The "All" tab is a router, not a workspace: it previews two incomparable
/// result sets. So it carries no order control on either — but each section must
/// still state its size, or a small preview reads as the whole answer.
#[tokio::test]
async fn all_tab_previews_are_sort_free_but_counted() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "all");
    ctx.insert("products", &json!([product("Milano Sofa", "milano-sofa")]));
    ctx.insert("results", &json!([thread_hit("Sofa after 2 years", "sofa-2y")]));
    ctx.insert("product_total", &2u64);
    ctx.insert("thread_total", &1u64);
    ctx.insert("total_all", &3u64);

    let html = render(ctx).await;

    assert!(
        !html.contains(r#"name="tsort""#) && !html.contains(r#"name="psort""#),
        "the All tab offers no order control on either section"
    );
    assert!(
        html.contains("fr-search-section-count"),
        "each All-tab section states its count, not just a conditional 'see all'"
    );
}

/// Filters AND together, so an empty page is often an empty intersection. The
/// recovery names *which* filter to drop and what dropping it restores — a bare
/// "no results" would claim the thing does not exist.
#[tokio::test]
async fn empty_intersection_offers_the_specific_filter_to_drop() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "products");
    with_one_category(&mut ctx);
    ctx.insert("active_category_id", "00000000-0000-0000-0000-000000000010");
    ctx.insert("active_brand", "00000000-0000-0000-0000-0000000000b1");
    ctx.insert("product_filters_active", &true);
    ctx.insert("facet_params", "&brand_id=00000000-0000-0000-0000-0000000000b1");
    ctx.insert("has_relaxation", &true);
    ctx.insert(
        "relaxed",
        &json!({ "without_category": 40, "without_facets": 8 }),
    );

    let html = render(ctx).await;

    assert!(html.contains("Without the product filters: 8 products"));
    assert!(html.contains("Without the category: 40 results"));
}

/// A relaxation that still yields nothing is not a way out, so it is not shown.
#[tokio::test]
async fn a_relaxation_worth_nothing_is_not_offered() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "products");
    ctx.insert("has_relaxation", &false);
    ctx.insert("relaxed", &json!({ "without_category": 0, "without_facets": null }));

    let html = render(ctx).await;

    assert!(!html.contains("No match for this combination"));
}

/// Switching tabs must not throw away the reader's filters. The facets survive
/// a round trip through the discussions tab as hidden state, and the tab link
/// into Products carries them in the URL.
#[tokio::test]
async fn facets_survive_a_trip_through_the_discussions_tab() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "threads");
    ctx.insert("active_type", "furniture");
    ctx.insert("active_brand", "00000000-0000-0000-0000-0000000000b1");
    ctx.insert("active_psort", "top_rated");
    ctx.insert("facet_params", "&type=furniture&psort=top_rated");

    let html = render(ctx).await;

    assert!(
        html.contains(r#"<input type="hidden" name="type" value="furniture">"#),
        "the product type is preserved as hidden state"
    );
    assert!(
        html.contains(r#"<input type="hidden" name="psort" value="top_rated">"#),
        "the product sort is preserved as hidden state"
    );
    // `&amp;` and not `&`: Tera escapes the separator, which is the correct
    // encoding for an href attribute — browsers read it back as `&`. Asserting
    // the raw form here would be asserting an HTML bug.
    assert!(
        html.contains("tab=products&amp;type=furniture&amp;psort=top_rated"),
        "the Products tab link carries the facets"
    );
}

/// The chips are the page's answer to "what is narrowing this right now?".
/// Each is removable on its own, and "clear all" only earns its place once
/// there is more than one thing to clear.
#[tokio::test]
async fn active_filters_render_as_individually_removable_chips() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "products");
    ctx.insert(
        "filter_chips",
        &json!([
            { "kind": "name", "label": "Sofa", "remove_url": "/search?q=sofa&tab=products&brand_id=B1" },
            { "kind": "name", "label": "Milano", "remove_url": "/search?q=sofa&tab=products&pcat=P1" },
        ]),
    );

    let html = render(ctx).await;

    assert!(html.contains("Sofa"), "first chip renders");
    assert!(html.contains("Milano"), "second chip renders");
    // Each chip's link keeps the *other* filter and drops only its own.
    assert!(html.contains("tab=products&amp;brand_id=B1"));
    assert!(html.contains("tab=products&amp;pcat=P1"));
    assert!(html.contains("Clear all"), "two chips earn a clear-all");
}

#[tokio::test]
async fn a_single_filter_gets_no_clear_all() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "products");
    ctx.insert(
        "filter_chips",
        &json!([{ "kind": "name", "label": "Sofa", "remove_url": "/search?q=sofa&tab=products" }]),
    );

    let html = render(ctx).await;
    assert!(html.contains("Sofa"));
    assert!(
        !html.contains("Clear all"),
        "one chip is already its own clear-all"
    );
}

/// Sorting is not filtering, so choosing an ordering must not make the page
/// claim filters are active.
#[tokio::test]
async fn sorting_alone_produces_no_filter_chips() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "products");
    ctx.insert("active_psort", "top_rated");

    let html = render(ctx).await;
    assert!(!html.contains("fr-search-chips"));
}

/// The type facet's label is a translated enum, so the chip carries the code
/// and resolves it through the same macro the cards use — one spelling of these
/// three words across the whole site.
#[tokio::test]
async fn a_product_type_chip_is_rendered_through_the_shared_label_macro() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "products");
    ctx.insert(
        "filter_chips",
        &json!([{ "kind": "code", "label": "furniture", "remove_url": "/search?q=sofa&tab=products" }]),
    );

    let html = render(ctx).await;
    assert!(html.contains("Furniture"), "the code is resolved to its label");
    assert!(
        !html.contains(">furniture<"),
        "the raw enum code must not reach the page"
    );
}

/// Each control's default option is its own name. Four dropdowns all starting
/// "All …" cannot be scanned, and here it was actively wrong: the catalogue
/// category and the product type read as near-duplicates.
#[tokio::test]
async fn facet_controls_are_labelled_by_axis_not_by_all_something() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "products");

    let html = render(ctx).await;

    for gone in ["All product categories", "All types", "All brands", "All materials"] {
        assert!(!html.contains(gone), "{gone:?} should no longer label a control");
    }
    assert!(html.contains(">Category<"), "the catalogue axis names itself");
    assert!(html.contains(">Type<"), "the type axis names itself");
    assert!(html.contains(">Brand<"));
    assert!(html.contains(">Material<"));
}

/// "All" is a router, not a workspace: it says where the matches are so the
/// reader can go there. A filter that narrows only one of the two lists it
/// shows would be answering a question nobody has asked yet.
#[tokio::test]
async fn the_all_tab_carries_no_filter_controls() {
    let mut ctx = base_ctx();
    with_one_category(&mut ctx);

    let html = render(ctx).await;

    assert!(!html.contains(r#"id="searchCategory""#));
    assert!(!html.contains(r#"<select name="brand_id""#));
    assert!(html.contains("fr-search-tabs"), "the tabs still route");
}

// ─── catalogue browse page ────────────────────────────────────────────────────

/// Mirrors `handlers::pages::catalog::catalog_index`.
fn catalog_ctx() -> Context {
    let mut ctx = base_ctx();
    ctx.insert("products", &json!([product("Milano Sofa", "milano-sofa")]));
    ctx.insert("total", &48u64);
    ctx.insert("page", &2u64);
    ctx.insert("has_prev", &true);
    ctx.insert("has_next", &true);
    ctx.insert("active_sort", &json!(null));
    ctx.insert("active_category_id", &json!(null));
    ctx.insert("search_query", &json!(null));
    ctx.insert("catalog_tab", "products");
    ctx.insert("filter_params", "");
    ctx.insert(
        "catalog_categories",
        &json!([{ "id": "00000000-0000-0000-0000-000000000010", "name": "Sofa", "is_child": false }]),
    );
    ctx
}

async fn render_catalog(ctx: Context) -> String {
    engine()
        .await
        .render(
            &Locale::default_locale(),
            "default/templates/catalog/index.html",
            ctx,
        )
        .await
        .expect("catalog/index.html must render")
}

#[tokio::test]
async fn catalog_offers_the_category_filter() {
    let mut ctx = catalog_ctx();
    ctx.insert("active_category_id", "00000000-0000-0000-0000-000000000010");

    let html = render_catalog(ctx).await;

    assert!(html.contains(r#"name="category_id""#), "category filter is offered");
    assert!(
        html.contains(r#"value="00000000-0000-0000-0000-000000000010" selected"#),
        "the active category is reflected back in the control"
    );
}

/// The catalogue filter bar is the same surface as the search page's, so it
/// must use the same building blocks: a self-labelled facet group, a sort that
/// is visibly *not* a filter, auto-submit on change, and a mobile disclosure.
#[tokio::test]
async fn catalog_filters_match_the_search_page_pattern() {
    let ctx = catalog_ctx();
    let html = render_catalog(ctx).await;

    assert!(html.contains("fr-refine-filters"), "shared facet group");
    assert!(html.contains("fr-refine-toggle"), "mobile disclosure, as on search");
    // The catalogue sub-nav uses the same tab component as the search result
    // tabs, not the lighter forum-feed sort tabs.
    assert!(
        html.contains("fr-search-tabs") && !html.contains("fr-sort-tab"),
        "catalogue nav uses the search tab component"
    );
    // The query field is the same element the search page uses — full-width with
    // the accent submit — not a bespoke input-group.
    assert!(
        html.contains("fr-search-field") && html.contains("fr-search-submit"),
        "the search field matches the search page's field"
    );
    assert!(
        html.contains(r#"@change="$el.form.submit()""#),
        "facets submit on change — no separate Lọc button"
    );
    // Sort lives on the results header, OUTSIDE the filter form — same as search.
    // It associates to the form by id and sits after the facet group, never
    // inside the filter row.
    assert!(
        html.contains(r#"form="catalogForm""#),
        "sort is form-associated because it renders outside the filter form"
    );
    let filters = html.find("fr-refine-filters").expect("filter group renders");
    let sort = html.find(r#"class="fr-sort-select""#).expect("sort renders");
    assert!(filters < sort, "sort comes after the filters, on the results header");
    // Facet order matches search: catalogue category leads, then type.
    let cat = html.find(r#"name="category_id""#).expect("category facet renders");
    let typ = html.find(r#"name="type""#).expect("type facet renders");
    assert!(cat < typ, "catalogue category leads the facet row, as on search");
    // Self-labelled defaults, never "All …" (the old ui-all-* strings).
    assert!(
        !html.contains("All types") && !html.contains("All brands") && !html.contains("All materials"),
        "facets are labelled by their axis, not 'All <something>'"
    );
}

/// Active filters render as removable chips, exactly like the search page.
#[tokio::test]
async fn catalog_shows_removable_filter_chips() {
    let mut ctx = catalog_ctx();
    ctx.insert(
        "filter_chips",
        &json!([
            { "kind": "name", "label": "Milano", "remove_url": "/catalog?sort=newest" },
            { "kind": "name", "label": "Sofa", "remove_url": "/catalog?brand_id=x" },
        ]),
    );

    let html = render_catalog(ctx).await;

    assert!(html.contains("fr-search-chips"), "chip row renders");
    assert!(html.contains("fr-search-catchip"), "each chip is a removable pill");
    assert!(html.contains("Milano") && html.contains("Sofa"), "chips are labelled");
    assert!(html.contains("fr-search-clear"), "with a clear-all when several are set");
}

/// The pager used to carry nothing but `page`, so page 2 of a filtered
/// catalogue silently showed an unrelated set. Every active filter must survive
/// the hop.
#[tokio::test]
async fn catalog_pagination_carries_every_active_filter() {
    let mut ctx = catalog_ctx();
    ctx.insert(
        "filter_params",
        "&q=sofa&type=furniture&category_id=00000000-0000-0000-0000-000000000010&sort=top_rated",
    );

    let html = render_catalog(ctx).await;

    for expected in [
        "/catalog?page=1&amp;q=sofa",
        "/catalog?page=3&amp;q=sofa",
        "type=furniture",
        "category_id=00000000-0000-0000-0000-000000000010",
        "sort=top_rated",
    ] {
        assert!(html.contains(expected), "pager must carry {expected}");
    }
}

/// Cause above effect: the tab strip decides which filters exist, so it must
/// come first. With the controls above it, the reader met four dropdowns and
/// only then found the thing that reshapes them — and the row changed under a
/// tab they had already scrolled past.
#[tokio::test]
async fn the_tab_strip_precedes_the_filter_controls() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "products");

    let html = render(ctx).await;

    let tabs = html.find("fr-search-tabs").expect("tab strip renders");
    let refine = html.find("fr-refine-controls").expect("filter row renders");
    assert!(
        tabs < refine,
        "the tab strip must come before the controls it governs"
    );
}

/// Collapsing controls on a phone is fine; collapsing the fact that some are
/// *set* is not. The disclosure carries the active count.
#[tokio::test]
async fn the_mobile_filter_disclosure_shows_how_many_are_active() {
    let mut ctx = base_ctx();
    ctx.insert("active_tab", "products");
    ctx.insert(
        "filter_chips",
        &json!([
            { "kind": "name", "label": "Sofa", "remove_url": "/search?q=sofa" },
            { "kind": "name", "label": "Milano", "remove_url": "/search?q=sofa" },
        ]),
    );

    let html = render(ctx).await;

    assert!(html.contains("fr-refine-toggle"), "the disclosure exists");
    assert!(html.contains("(2)"), "and says how many filters are set");
}

/// Before anything is searched there is nothing to route between and nothing to
/// refine, so neither the tabs nor the controls appear. Also exercises the
/// template's outermost branch, which the other cases never take.
#[tokio::test]
async fn an_unsubmitted_search_shows_neither_tabs_nor_filters() {
    let mut ctx = base_ctx();
    ctx.insert("query", "");
    ctx.insert("query_encoded", "");

    let html = render(ctx).await;

    assert!(!html.contains("fr-search-tabs"));
    assert!(!html.contains("fr-refine-controls"));
    // The field itself is still there — that is the whole page at this point.
    assert!(html.contains(r#"name="q""#));
}

// ─── catalogue rail ───────────────────────────────────────────────────────────
//
// The three catalogue tabs were the only public pages rendering a single column
// while every other surface shipped a right rail. These cover the rail's own
// logic — active state, filter-preserving links, the contribute action — and the
// fact that it survives the two tabs that supply less context than /catalog.

/// The rail's category rows are the browse axis that used to exist only as one
/// of four look-alike selects. They must say which one is showing.
#[tokio::test]
async fn catalog_rail_marks_the_active_category() {
    let mut ctx = catalog_ctx();
    ctx.insert("active_category_id", "00000000-0000-0000-0000-000000000010");

    let html = render_catalog(ctx).await;

    assert!(html.contains("fr-right-col"), "the catalogue renders a right rail");
    assert!(
        html.contains(r#"aria-current="page""#),
        "the active category row is marked for assistive tech, not just visually"
    );
    assert!(html.contains("All products"), "and offers the way back out of it");
}

/// Picking a category from the rail must narrow the current view, not reset it.
/// The rows carry every other active filter; only the category is replaced.
#[tokio::test]
async fn catalog_rail_category_links_keep_the_other_filters() {
    let mut ctx = catalog_ctx();
    ctx.insert("category_link_params", "&q=sofa&brand_id=b1");
    ctx.insert("catalog_all_url", "/catalog?q=sofa&brand_id=b1");

    let html = render_catalog(ctx).await;

    assert!(
        html.contains("/catalog?category_id=00000000-0000-0000-0000-000000000010&amp;q=sofa&amp;brand_id=b1"),
        "a category row carries the query and brand already set"
    );
    assert!(
        !html.contains(r#"href="/catalog?category_id=00000000-0000-0000-0000-000000000010""#),
        "no bare link that would silently drop them"
    );
}

/// The submission action lives in the tab bar, where it reaches every reader at
/// every width. The rail must not carry a second copy of it — that duplicate
/// existed briefly and only ever showed up for desktop readers.
#[tokio::test]
async fn catalog_rail_does_not_duplicate_the_submission_action() {
    let mut ctx = catalog_ctx();
    ctx.insert("can_submit_product", &true);

    let html = render_catalog(ctx).await;

    assert_eq!(
        html.matches("add_product=1").count(),
        1,
        "exactly one submission CTA on the page — the one in the tab bar"
    );
    let nav = html.find("fr-catalog-nav").expect("tab bar renders");
    let rail = html.find("fr-right-col").expect("rail renders");
    let cta = html.find("add_product=1").expect("the CTA renders");
    assert!(cta > nav && cta < rail, "and it is the tab bar's, not the rail's");
}

/// The tab strip shares its row with that action, so the rule under it belongs
/// to the row — on the tabs alone it stopped short of the button.
#[tokio::test]
async fn catalog_tab_rule_spans_the_whole_action_row() {
    let mut ctx = catalog_ctx();
    ctx.insert("can_submit_product", &true);

    let html = render_catalog(ctx).await;

    assert!(
        html.contains("fr-catalog-nav"),
        "the tab row carries the rule, so it runs past the action button"
    );
}

/// /materials and /brands share the rail but supply no category filter and no
/// pager params. Tera raises on an equality test against a *missing* variable,
/// so the rail normalises them — without that, adding the rail to those two tabs
/// takes both pages down at render time, not at parse time.
#[tokio::test]
async fn catalog_rail_renders_on_the_tabs_that_have_no_category_filter() {
    let mut ctx = base_ctx();
    ctx.insert("catalog_tab", "brands");
    ctx.insert(
        "catalog_categories",
        &json!([{ "id": "00000000-0000-0000-0000-000000000010", "name": "Sofa", "is_child": true }]),
    );

    let html = engine()
        .await
        .render(
            &Locale::default_locale(),
            "default/templates/catalog/brands.html", ctx,
        )
        .await
        .expect("brands.html must render with the shared rail");

    assert!(html.contains("fr-right-col"), "the rail renders here too");
    assert!(
        html.contains("fr-panel-row--child"),
        "a child category is drawn as one — the taxonomy is two levels deep"
    );
    // Nothing is active when there is no category filter on the page, so the
    // reset row is the current position.
    assert!(html.contains("All products"));
}
/// The rail's review panel calls a macro, and Tera resolves a macro namespace
/// against the template being *rendered* — not the one the `{% include %}` sits
/// in. So every page that includes the panel must import `macros.html` itself,
/// and a page that forgets 500s at request time while still parsing cleanly at
/// startup. That is exactly how /brands and / broke while every existing test
/// stayed green: the earlier cases left `latest_reviews` unset, so the panel was
/// skipped and the macro was never reached.
///
/// Every catalogue tab is covered because each is a separate root template with
/// its own import line to forget.
#[tokio::test]
async fn every_catalogue_tab_renders_the_rails_review_panel() {
    let review = json!([{
        "slug": "r1", "product_name": "Sofa văng", "product_slug": "sofa-vang",
        "author_display_name": "Alice", "author_username": "alice",
        "author_avatar_url": null, "overall": 5, "created_at": "2026-07-01T10:00:00Z"
    }]);

    for (tab, template) in [
        ("products", "default/templates/catalog/index.html"),
        ("materials", "default/templates/catalog/materials.html"),
        ("brands", "default/templates/catalog/brands.html"),
    ] {
        let mut ctx = catalog_ctx();
        ctx.insert("catalog_tab", tab);
        ctx.insert("material_groups", &json!([]));
        ctx.insert("latest_reviews", &review);

        let html = engine()
            .await
            .render(&Locale::default_locale(), template, ctx)
            .await
            .unwrap_or_else(|e| panic!("{template} must render with a populated rail: {e}"));

        assert!(
            html.contains("Sofa văng"),
            "{template} renders the review row, macro and all"
        );
    }
}

// ─── file_url() ──────────────────────────────────────────────────────────────
//
// Templates stopped hand-assembling `/files/{{ key }}` and now call the
// `file_url()` Tera function, so that `ports::file_url` stays the single
// definition of the resolver path. That move introduced a failure mode the
// parse-only guards in `tera_templates.rs` cannot see: Tera parses a call to an
// unregistered function perfectly happily and only fails when the template is
// rendered. A missing registration would therefore ship as a 500 on the product
// page rather than a build error.

/// Tera auto-escapes `/` to `&#x2F;` in HTML output, so a rendered `src` reads
/// `&#x2F;files&#x2F;…`. Browsers decode character references inside attribute
/// values, so this resolves correctly — and it is not new: the old
/// `src="/files/{{ key }}"` already escaped the slashes *inside* the key the
/// same way. Escaping is kept rather than piped through `| safe`, which would
/// let a storage key containing a quote break out of the attribute.
fn decode_slashes(html: &str) -> String {
    html.replace("&#x2F;", "/")
}

#[tokio::test]
async fn a_product_with_an_image_renders_the_resolver_path() {
    // Every other product fixture in this file carries `primary_image_key:
    // null`, so the `{% if %}` guarding the `<img>` was never taken and the call
    // was never rendered. Supplying a key is the whole point of this test.
    let mut with_image = product("Milano Sofa", "milano-sofa");
    with_image["primary_image_key"] =
        json!("products/0123456789abcdef0123456789abcdef.jpg");

    let mut ctx = catalog_ctx();
    ctx.insert("products", &json!([with_image]));

    let html = render_catalog(ctx).await;

    assert!(
        decode_slashes(&html)
            .contains(r#"src="/files/products/0123456789abcdef0123456789abcdef.jpg""#),
        "file_url() must be registered and must mint the resolver path; got:\n{html}"
    );
}

#[tokio::test]
async fn file_url_is_registered_for_every_installed_locale() {
    // `TeraEngine` builds one Tera per locale. A function registered on only the
    // default instance would leave the Vietnamese catalogue page rendering a
    // 500 while English worked — the kind of fault that reaches production
    // because nobody browses in the second language.
    let mut with_image = product("Ghế Milano", "ghe-milano");
    with_image["primary_image_key"] = json!("products/aaaabbbbccccddddeeeeffff00001111.jpg");

    let mut ctx = catalog_ctx();
    ctx.insert("products", &json!([with_image]));

    let html = engine()
        .await
        .render(
            &Locale::parse("vi").expect("vi is a valid tag"),
            "default/templates/catalog/index.html", ctx,
        )
        .await
        .expect("catalog/index.html must render in Vietnamese too");

    assert!(
        decode_slashes(&html).contains("/files/products/aaaabbbbccccddddeeeeffff00001111.jpg"),
        "file_url() is missing from the `vi` Tera instance"
    );
}
