use async_trait::async_trait;
use meilisearch_sdk::client::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ferum_application::ports::{
    ProductSearchSort, SearchHit, SearchKind, SearchQuery, SearchResults, SearchService,
    ThreadSearchSort,
};
use ferum_application::shared::AppError;
use ferum_domain::models::product::ProductType;

fn product_type_str(t: ProductType) -> &'static str {
    match t {
        ProductType::Furniture => "furniture",
        ProductType::Material => "material",
        ProductType::Room => "room",
    }
}

/// The `content` field indexed in Meilisearch is raw thread/post text, not
/// pre-sanitized HTML (unlike the Postgres FTS path, which sanitizes
/// `ts_headline` output before returning it). Search excerpts are rendered
/// with Tera's `| safe` filter, so any tags left in here become stored XSS —
/// strip everything down to plain text before it leaves this service.
fn sanitize_excerpt(text: String) -> String {
    EXCERPT_SANITIZER.clean(&text).to_string()
}

/// Built once — this runs per result row, and `Builder::new()` populates the
/// crate's default allow-lists before `.tags()` empties them again.
static EXCERPT_SANITIZER: std::sync::LazyLock<ammonia::Builder<'static>> =
    std::sync::LazyLock::new(|| {
        let mut builder = ammonia::Builder::new();
        builder.tags(std::collections::HashSet::new());
        builder
    });

pub struct MeilisearchService {
    client: Client,
    thread_index: String,
    product_index: String,
}

impl MeilisearchService {
    pub fn new(url: &str, api_key: Option<&str>, thread_index: &str, product_index: &str) -> Self {
        let client = Client::new(url, api_key).expect("valid Meilisearch URL");
        Self {
            client,
            thread_index: thread_index.to_string(),
            product_index: product_index.to_string(),
        }
    }
}

/// Shared shape for both indexes. `title` carries the thread title or the
/// product name, `content` the post body or the product description — the two
/// documents are normalised at index time so one target deserialises both.
#[derive(Debug, Serialize, Deserialize)]
struct Doc {
    id: String,
    slug: String,
    title: String,
    content: Option<String>,
}

#[async_trait]
impl SearchService for MeilisearchService {
    async fn search(&self, query: SearchQuery) -> Result<SearchResults, AppError> {
        if query.q.trim().is_empty() {
            return Ok(SearchResults { hits: vec![], total: 0 });
        }

        // Mirrors the Postgres adapter: an empty allowed-category set means the
        // caller resolved visibility to "nothing", not "no filter".
        if query.kind == SearchKind::Thread && query.visible_category_ids.is_empty() {
            return Ok(SearchResults { hits: vec![], total: 0 });
        }

        let index_name = match query.kind {
            SearchKind::Thread => &self.thread_index,
            SearchKind::Product => &self.product_index,
        };
        let index = self.client.index(index_name);

        // count_only still costs a round trip — Meilisearch exposes no cheaper
        // count endpoint — but limit 1 keeps the payload to a single document.
        let (limit, offset) = if query.count_only {
            (1usize, 0usize)
        } else {
            (
                query.per_page as usize,
                ((query.page.saturating_sub(1)) * query.per_page) as usize,
            )
        };

        let mut search = index.search();
        search.with_query(&query.q).with_limit(limit).with_offset(offset);

        let filter = match query.kind {
            SearchKind::Thread => {
                let ids = query
                    .visible_category_ids
                    .iter()
                    .map(|id| format!("\"{}\"", id))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("category_id IN [{}]", ids)
            }
            // Same visibility rule and same facets as the SQL path.
            SearchKind::Product => {
                let mut parts: Vec<String> = Vec::new();
                parts.push(match query.viewer_id {
                    Some(viewer) => {
                        format!("(status = \"published\" OR created_by_id = \"{}\")", viewer)
                    }
                    None => "status = \"published\"".to_string(),
                });
                if let Some(t) = query.facets.product_type {
                    parts.push(format!("product_type = \"{}\"", product_type_str(t)));
                }
                if let Some(brand_id) = query.facets.brand_id {
                    parts.push(format!("brand_id = \"{}\"", brand_id));
                }
                if let Some(material_id) = query.facets.material_id {
                    parts.push(format!("material_ids = \"{}\"", material_id));
                }
                if !query.in_category_ids.is_empty() {
                    let ids = query
                        .in_category_ids
                        .iter()
                        .map(|id| format!("\"{}\"", id))
                        .collect::<Vec<_>>()
                        .join(", ");
                    parts.push(format!("category_id IN [{}]", ids));
                }
                parts.join(" AND ")
            }
        };
        search.with_filter(&filter);

        // Meilisearch's own ranking is the relevance ordering; the alternatives
        // are explicit sort rules over indexed numeric attributes. `avg_overall`
        // is the raw mean here, unlike the SQL path's Bayesian shrinkage — the
        // index would have to carry a precomputed shrunk score for the two to
        // agree, so `sortable_attributes` must include `bayesian_score` when
        // this index is built. Until then TopRated degrades to review-weighted
        // order, which is the safer of the two wrong answers.
        let sort_rules: &[&str] = match query.kind {
            SearchKind::Product => match query.facets.sort {
                ProductSearchSort::Relevance => &[],
                ProductSearchSort::TopRated => &["bayesian_score:desc", "review_count:desc"],
                ProductSearchSort::MostReviewed => &["review_count:desc"],
                ProductSearchSort::Newest => &["created_at:desc"],
            },
            // Thread ordering mirrors the SQL path in `postgres_fts::plan_threads`.
            // The thread index must expose `created_at` and `reply_count` in its
            // `sortable_attributes` for these to take effect — same build-time
            // requirement as `bayesian_score` above.
            SearchKind::Thread => match query.thread_sort {
                ThreadSearchSort::Relevance => &[],
                ThreadSearchSort::Newest => &["created_at:desc"],
                ThreadSearchSort::MostReplies => &["reply_count:desc"],
            },
        };
        if !sort_rules.is_empty() {
            search.with_sort(sort_rules);
        }

        let results = search
            .execute::<Doc>()
            .await
            .map_err(|e| AppError::internal(format!("Meilisearch error: {}", e)))?;

        let total = results.estimated_total_hits.unwrap_or(0) as u64;
        if query.count_only {
            return Ok(SearchResults { hits: vec![], total });
        }

        let hits = results
            .hits
            .into_iter()
            .filter_map(|h| {
                let doc = h.result;
                let id = Uuid::parse_str(&doc.id).ok()?;
                Some(SearchHit {
                    kind: query.kind,
                    id,
                    slug: doc.slug,
                    title: doc.title,
                    excerpt: doc
                        .content
                        .map(|c| sanitize_excerpt(c.chars().take(200).collect()))
                        .filter(|s| !s.trim().is_empty()),
                })
            })
            .collect();

        Ok(SearchResults { hits, total })
    }
}
