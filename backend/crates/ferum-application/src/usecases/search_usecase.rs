use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use uuid::Uuid;

use crate::permission::PermissionChecker;
use crate::ports::{
    ProductFacets, SearchKind, SearchQuery, SearchResults, SearchService, ThreadSearchSort,
};
use crate::shared::AppError;
use ferum_domain::AuthUser;
use ferum_domain::repositories::category_repository::CategoryRepository;
use ferum_domain::repositories::product_repository::ProductRepository;
use ferum_domain::repositories::thread_repository::ThreadRepository;

/// How many products the "everything" tab previews above the discussion list.
/// Four fills one card row at every breakpoint; the tab header links to the
/// rest. Enough to answer "does this product exist here?" without burying the
/// discussions under a wall of catalogue.
pub const PRODUCT_PREVIEW_LIMIT: u64 = 4;

/// Which slice of results the caller wants. Mirrors the tabs on the results
/// page, and decides which kind gets paginated and which (if any) is reduced to
/// a count for its tab badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchScope {
    /// Products previewed, discussions paginated.
    #[default]
    All,
    Products,
    Threads,
}

impl SearchScope {
    pub fn as_str(self) -> &'static str {
        match self {
            SearchScope::All => "all",
            SearchScope::Products => "products",
            SearchScope::Threads => "threads",
        }
    }

    /// Unknown values fall back to `All` rather than erroring: a hand-edited or
    /// stale `?tab=` should show results, not a 400.
    pub fn parse(s: &str) -> Self {
        match s {
            "products" => SearchScope::Products,
            "threads" | "discussions" => SearchScope::Threads,
            _ => SearchScope::All,
        }
    }
}

/// Everything a search asks for except *who* is asking, which stays a separate
/// argument so the request can be built (and defaulted) without a borrow.
///
/// A struct rather than seven positional parameters: `Option<Uuid>` next to
/// `u64, u64` is exactly the signature where a transposed pair compiles and
/// then quietly pages through the wrong thing.
#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub q: String,
    pub scope: SearchScope,
    /// A **forum** category. Narrows discussions to it and its children.
    ///
    /// Not applied to products: forum categories organise conversation
    /// (Off-Topic, Programming, Site Feedback) and none of them is a sensible
    /// home for a sofa. Products have their own taxonomy — see
    /// `product_category_ids`.
    pub category_id: Option<Uuid>,
    /// **Catalogue** categories (Sofa, Ghế, Bàn …), already expanded to include
    /// children. Empty = no restriction. Narrows products only.
    pub product_category_ids: Vec<Uuid>,
    pub facets: ProductFacets,
    /// Ordering for discussion hits. The product counterpart lives in `facets`;
    /// threads have no facets, so it sits directly on the request.
    pub thread_sort: ThreadSearchSort,
    pub page: u64,
    /// Clamped to 30 by the use case; callers may pass whatever they were given.
    pub per_page: u64,
}

impl Default for SearchRequest {
    fn default() -> Self {
        Self {
            q: String::new(),
            scope: SearchScope::default(),
            category_id: None,
            product_category_ids: Vec::new(),
            facets: ProductFacets::default(),
            thread_sort: ThreadSearchSort::default(),
            page: 1,
            per_page: 20,
        }
    }
}

/// A thread hit joined with live thread + category data, for SSR result pages.
/// Meta fields are `None` when the indexed thread no longer resolves (e.g. it
/// was deleted after indexing) — the hit itself is still shown.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HydratedSearchHit {
    pub thread_id: Uuid,
    pub thread_slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub category_slug: Option<String>,
    pub category_name: Option<String>,
    pub author_username: Option<String>,
    pub author_display_name: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub reply_count: i32,
}

/// A product hit carrying everything a catalogue card renders: the rating is
/// the whole point of the platform, so a product result without it would be
/// strictly worse than the same product seen while browsing.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HydratedProductHit {
    pub product_id: Uuid,
    pub slug: String,
    pub name: String,
    pub excerpt: Option<String>,
    pub product_type: String,
    pub style: Option<String>,
    pub primary_image_key: Option<String>,
    pub price_min: Option<i32>,
    pub price_max: Option<i32>,
    pub currency: String,
    pub review_count: i32,
    pub avg_overall: Option<f64>,
}

/// What the active tab would return if one filter were dropped.
///
/// Filters are AND-combined, which is the only reading a person expects — each
/// choice narrows. The cost of AND is that two reasonable choices can intersect
/// to nothing, and "no results" then tells the reader their thing does not
/// exist when really their combination does not. These counts turn that dead
/// end into a specific, one-click way out: *which* filter to drop, and what
/// dropping it would yield.
///
/// Computed only when the active tab came back empty and something was actually
/// narrowing — one extra count query each, on the miss path only.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct RelaxedCounts {
    /// Hits on the active tab with the category filter removed.
    pub without_category: Option<u64>,
    /// Products with the type/brand/material facets removed (category kept).
    /// Never set on a discussion tab — facets do not apply there.
    pub without_facets: Option<u64>,
}

impl RelaxedCounts {
    /// Whether there is anything worth offering. A relaxation that still yields
    /// zero is not a way out, so it is not surfaced.
    pub fn has_any(&self) -> bool {
        self.without_category.is_some_and(|n| n > 0)
            || self.without_facets.is_some_and(|n| n > 0)
    }
}

/// One answer covering both entity types. The totals are always for the whole
/// match set, not the returned slice — the tabs show them as badges, including
/// for the tab the user is not currently on.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SearchOutcome {
    pub threads: Vec<HydratedSearchHit>,
    pub products: Vec<HydratedProductHit>,
    pub thread_total: u64,
    pub product_total: u64,
    pub relaxed: RelaxedCounts,
}

impl SearchOutcome {
    pub fn is_empty(&self) -> bool {
        self.thread_total == 0 && self.product_total == 0
    }
}

pub struct SearchUseCase {
    pub search: Arc<dyn SearchService>,
    pub threads: Arc<dyn ThreadRepository>,
    pub categories: Arc<dyn CategoryRepository>,
    pub products: Arc<dyn ProductRepository>,
}

impl SearchUseCase {
    pub fn new(
        search: Arc<dyn SearchService>,
        threads: Arc<dyn ThreadRepository>,
        categories: Arc<dyn CategoryRepository>,
        products: Arc<dyn ProductRepository>,
    ) -> Self {
        Self {
            search,
            threads,
            categories,
            products,
        }
    }

    /// The set of categories whose threads `actor` may see, optionally narrowed
    /// to one subtree.
    ///
    /// This is the only thing standing between a guest and the titles of
    /// `staff_only` threads, so it is resolved here — in the use case — rather
    /// than trusted from the caller. An actor who can see nothing yields an
    /// empty set, and the search port treats an empty set as "no thread hits";
    /// the failure mode is silence, not disclosure.
    async fn visible_category_ids(
        &self,
        actor: Option<&AuthUser>,
        within: Option<Uuid>,
    ) -> Vec<Uuid> {
        let all = self.categories.list_all().await.unwrap_or_default();
        all.iter()
            .filter(|c| PermissionChecker::can_view_category(actor, c).is_ok())
            .filter(|c| match within {
                // A parent category selection includes its children, matching
                // how the rest of the site treats the two-level hierarchy.
                Some(root) => c.id == root || c.parent_id == Some(root),
                None => true,
            })
            .map(|c| c.id)
            .collect()
    }

    /// Search products and discussions in one pass and hydrate both.
    ///
    /// The two searches run concurrently and are kept as separate lists on
    /// purpose: `ts_rank` over a thread title and over a product vector are not
    /// comparable numbers, so interleaving them by score would produce an order
    /// with no meaning. The page presents them as sections instead.
    pub async fn search_all(
        &self,
        actor: Option<&AuthUser>,
        req: SearchRequest,
    ) -> Result<SearchOutcome, AppError> {
        let SearchRequest {
            q,
            scope,
            category_id,
            product_category_ids,
            facets,
            thread_sort,
            page,
            per_page,
        } = req;

        if q.trim().is_empty() {
            return Ok(SearchOutcome::default());
        }

        let per_page = per_page.clamp(1, 30);
        let viewer_id = actor.map(|u| u.id);

        // Threads are narrowed by the *forum* category, intersected with what
        // the viewer may see — that intersection is an access control and must
        // stay narrow. Products are narrowed by the *catalogue* category, which
        // the caller has already resolved; a product is not access-controlled by
        // the category it is filed under, only by its own `status`.
        let visible_narrowed = self.visible_category_ids(actor, category_id).await;
        let chosen_subtree = product_category_ids;

        let thread_query = |page: u64, per_page: u64, count_only: bool| SearchQuery {
            q: q.clone(),
            kind: SearchKind::Thread,
            facets: ProductFacets::default(),
            thread_sort,
            visible_category_ids: visible_narrowed.clone(),
            in_category_ids: Vec::new(),
            viewer_id,
            page,
            per_page,
            count_only,
        };
        let product_query = |page: u64, per_page: u64, count_only: bool| SearchQuery {
            q: q.clone(),
            kind: SearchKind::Product,
            facets: facets.clone(),
            thread_sort: ThreadSearchSort::default(),
            visible_category_ids: Vec::new(),
            in_category_ids: chosen_subtree.clone(),
            viewer_id,
            page,
            per_page,
            count_only,
        };

        // Per scope: one kind is paginated, the other is reduced to a badge
        // count — except on `All`, where products get a small preview row.
        let (thread_q, product_q) = match scope {
            SearchScope::All => (
                thread_query(page, per_page, false),
                product_query(1, PRODUCT_PREVIEW_LIMIT, false),
            ),
            SearchScope::Threads => (
                thread_query(page, per_page, false),
                product_query(1, 1, true),
            ),
            SearchScope::Products => (
                thread_query(1, 1, true),
                product_query(page, per_page, false),
            ),
        };

        let (thread_res, product_res) =
            tokio::try_join!(self.search.search(thread_q), self.search.search(product_q))?;

        let (threads, products) = tokio::join!(
            self.hydrate_threads(&thread_res),
            self.hydrate_products(&product_res),
        );

        // Whichever kind the active tab leads with is the one whose emptiness
        // the reader is staring at, so that is the one worth explaining.
        let active_total = match scope {
            SearchScope::Products => product_res.total,
            SearchScope::Threads => thread_res.total,
            // On `All`, only a double miss is a dead end — one populated
            // section already gives the reader somewhere to go.
            SearchScope::All => thread_res.total + product_res.total,
        };

        let relaxed = if active_total == 0 {
            self.relaxation_counts(
                &q,
                scope,
                viewer_id,
                &chosen_subtree,
                category_id.is_some(),
                &facets,
                actor,
            )
            .await
        } else {
            RelaxedCounts::default()
        };

        Ok(SearchOutcome {
            threads,
            products,
            thread_total: thread_res.total,
            product_total: product_res.total,
            relaxed,
        })
    }

    /// Counts for "what if you dropped one filter", for the empty state.
    ///
    /// Only the filters that are actually set are probed, and each probe is a
    /// `count_only` query — no rows are fetched to produce a number the page
    /// will render as a link. Failures degrade to `None`: a missing suggestion
    /// is a worse empty state, not a broken page.
    #[allow(clippy::too_many_arguments)]
    async fn relaxation_counts(
        &self,
        q: &str,
        scope: SearchScope,
        viewer_id: Option<Uuid>,
        chosen_subtree: &[Uuid],
        category_chosen: bool,
        facets: &ProductFacets,
        actor: Option<&AuthUser>,
    ) -> RelaxedCounts {
        let count_products = |facets: ProductFacets, in_category_ids: Vec<Uuid>| SearchQuery {
            q: q.to_string(),
            kind: SearchKind::Product,
            facets,
            thread_sort: ThreadSearchSort::default(),
            visible_category_ids: Vec::new(),
            in_category_ids,
            viewer_id,
            page: 1,
            per_page: 1,
            count_only: true,
        };

        let mut out = RelaxedCounts::default();

        // "The category" means whichever taxonomy narrows the tab the reader is
        // actually looking at: the catalogue tree on Products, the forum tree
        // otherwise. Probing the wrong one would offer to drop a filter that
        // was never applied to what they can see.
        if scope == SearchScope::Products {
            if !chosen_subtree.is_empty() {
                let widened = self.search.search(count_products(facets.clone(), vec![])).await;
                out.without_category = widened.ok().map(|r| r.total);
            }
        } else if category_chosen {
            // Widening back to everything the viewer may see has to be
            // re-resolved: the set in hand is already cut to the chosen subtree.
            let all_visible = self.visible_category_ids(actor, None).await;
            let widened = self
                .search
                .search(SearchQuery {
                    q: q.to_string(),
                    kind: SearchKind::Thread,
                    facets: ProductFacets::default(),
                    thread_sort: ThreadSearchSort::default(),
                    visible_category_ids: all_visible,
                    in_category_ids: Vec::new(),
                    viewer_id,
                    page: 1,
                    per_page: 1,
                    count_only: true,
                })
                .await;
            out.without_category = widened.ok().map(|r| r.total);
        }

        // Facets only exist on the product side, so this is only meaningful
        // where products are what the reader is looking at.
        if facets.is_narrowed() && scope != SearchScope::Threads {
            let widened = self
                .search
                .search(count_products(
                    ProductFacets { sort: facets.sort, ..ProductFacets::default() },
                    chosen_subtree.to_vec(),
                ))
                .await;
            out.without_facets = widened.ok().map(|r| r.total);
        }

        out
    }

    /// Joins thread hits with author, category and stats. Best-effort: a lookup
    /// failure degrades to bare hits rather than failing the whole search.
    async fn hydrate_threads(&self, results: &SearchResults) -> Vec<HydratedSearchHit> {
        if results.hits.is_empty() {
            return vec![];
        }

        let ids: Vec<Uuid> = results.hits.iter().map(|h| h.id).collect();
        let threads = self.threads.find_many_by_ids(&ids).await.unwrap_or_default();
        let categories = self.categories.list_all().await.unwrap_or_default();

        let thread_map: HashMap<Uuid, _> = threads.iter().map(|t| (t.id, t)).collect();
        let cat_map: HashMap<Uuid, _> = categories.iter().map(|c| (c.id, c)).collect();

        // Preserve the relevance order of the raw hits.
        results
            .hits
            .iter()
            .map(|h| {
                let thread = thread_map.get(&h.id);
                let category = thread.and_then(|t| cat_map.get(&t.category_id));
                HydratedSearchHit {
                    thread_id: h.id,
                    thread_slug: h.slug.clone(),
                    title: h.title.clone(),
                    excerpt: h.excerpt.clone(),
                    category_slug: category.map(|c| c.slug.clone()),
                    category_name: category.map(|c| c.name.clone()),
                    author_username: thread.and_then(|t| t.author_username.clone()),
                    author_display_name: thread.and_then(|t| {
                        t.author_display_name
                            .clone()
                            .or_else(|| t.author_username.clone())
                    }),
                    created_at: thread.map(|t| t.created_at),
                    reply_count: thread.map(|t| t.reply_count).unwrap_or(0),
                }
            })
            .collect()
    }

    /// Joins product hits with price, cover and aggregate rating. A hit whose
    /// product no longer resolves is dropped rather than rendered as a blank
    /// card — unlike a thread, a product card is nothing but its metadata.
    async fn hydrate_products(&self, results: &SearchResults) -> Vec<HydratedProductHit> {
        if results.hits.is_empty() {
            return vec![];
        }

        let ids: Vec<Uuid> = results.hits.iter().map(|h| h.id).collect();
        let items = self.products.find_many_by_ids(&ids).await.unwrap_or_default();
        let item_map: HashMap<Uuid, _> = items.iter().map(|i| (i.product.id, i)).collect();

        results
            .hits
            .iter()
            .filter_map(|h| {
                let item = item_map.get(&h.id)?;
                let p = &item.product;
                Some(HydratedProductHit {
                    product_id: p.id,
                    slug: p.slug.clone(),
                    name: p.name.clone(),
                    excerpt: h.excerpt.clone(),
                    product_type: match p.product_type {
                        ferum_domain::models::product::ProductType::Furniture => "furniture",
                        ferum_domain::models::product::ProductType::Material => "material",
                        ferum_domain::models::product::ProductType::Room => "room",
                    }
                    .to_string(),
                    style: p.style.clone(),
                    primary_image_key: p.primary_image_key.clone(),
                    price_min: p.price_min,
                    price_max: p.price_max,
                    currency: p.currency.clone(),
                    review_count: item.review_count,
                    avg_overall: item.avg_overall.and_then(|v| v.to_f64()),
                })
            })
            .collect()
    }
}
