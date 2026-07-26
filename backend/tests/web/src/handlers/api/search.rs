use ferum_web::view_models::search::SearchQuery;
use uuid::Uuid;

#[test]
fn search_query_all_none_by_default() {
    let q: SearchQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.q.is_none());
    assert!(q.category_id.is_none());
    assert!(q.page.is_none());
    assert!(q.per_page.is_none());
}

#[test]
fn search_query_with_text_and_pagination() {
    let q: SearchQuery = serde_json::from_str(r#"{"q":"axum tutorial","page":2,"per_page":10}"#).unwrap();
    assert_eq!(q.q.as_deref(), Some("axum tutorial"));
    assert_eq!(q.page, Some(2));
    assert_eq!(q.per_page, Some(10));
}

#[test]
fn search_query_with_category_filter() {
    let cat_id = Uuid::new_v4();
    let json = format!(r#"{{"q":"test","category_id":"{cat_id}"}}"#);
    let q: SearchQuery = serde_json::from_str(&json).unwrap();
    assert_eq!(q.category_id, Some(cat_id));
}

#[test]
fn search_query_invalid_category_uuid_fails() {
    let result: Result<SearchQuery, _> =
        serde_json::from_str(r#"{"category_id":"not-a-uuid"}"#);
    assert!(result.is_err());
}

// ─── Blank facet params ───────────────────────────────────────────────────────
//
// Every product facet on the search page is an HTML `<select>` whose placeholder
// option has `value=""`, and the form auto-submits on change. A native submit
// includes *all* named controls, so a request carries `brand_id=` and
// `material_id=` the moment any one dropdown is touched. With a bare
// `Option<Uuid>` that fails the whole extractor and 400s the request.

#[test]
fn blank_category_id_is_treated_as_absent() {
    let q: SearchQuery = serde_json::from_str(r#"{"category_id":""}"#).unwrap();
    assert!(q.category_id.is_none());
}

#[test]
fn blank_brand_and_material_ids_are_treated_as_absent() {
    let q: SearchQuery =
        serde_json::from_str(r#"{"q":"sofa","brand_id":"","material_id":""}"#).unwrap();
    assert!(q.brand_id.is_none());
    assert!(q.material_id.is_none());
    assert_eq!(q.q.as_deref(), Some("sofa"));
}

#[test]
fn blank_facets_do_not_prevent_a_real_one_from_parsing() {
    let brand = Uuid::new_v4();
    let json = format!(r#"{{"brand_id":"{brand}","material_id":""}}"#);
    let q: SearchQuery = serde_json::from_str(&json).unwrap();
    assert_eq!(q.brand_id, Some(brand));
    assert!(q.material_id.is_none());
}

/// Blank is tolerated; garbage is not. Quietly dropping a malformed filter would
/// hand the caller unfiltered results they never asked for.
#[test]
fn malformed_brand_id_still_fails() {
    let result: Result<SearchQuery, _> = serde_json::from_str(r#"{"brand_id":"nope"}"#);
    assert!(result.is_err());
}
