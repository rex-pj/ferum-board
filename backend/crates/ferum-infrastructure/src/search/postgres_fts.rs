use async_trait::async_trait;
use sea_orm::{DatabaseConnection, DbBackend, FromQueryResult, Statement};
use uuid::Uuid;

fn sanitize_headline(html: String) -> String {
    ammonia::Builder::new()
        .tags(["b"].iter().cloned().collect())
        .clean(&html)
        .to_string()
}

use ferum_application::ports::{
    ProductSearchSort, SearchHit, SearchKind, SearchQuery, SearchResults, SearchService,
    ThreadSearchSort,
};
use ferum_application::shared::AppError;
use ferum_domain::models::product::ProductType;
use ferum_domain::repositories::product_repository::bayesian;

pub struct PostgresFtsService {
    db: DatabaseConnection,
}

impl PostgresFtsService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[derive(Debug, FromQueryResult)]
struct FtsRow {
    id: Uuid,
    slug: String,
    title: String,
    excerpt: Option<String>,
}

#[derive(Debug, FromQueryResult)]
struct CountRow {
    count: i64,
}

/// A ready-to-execute pair of statements: the page of rows, and the size of the
/// full match set. Built per kind so `search` stays a thin dispatcher.
struct Plan {
    data_sql: String,
    count_sql: String,
    /// Values for `count_sql`. `data_sql` takes these plus LIMIT and OFFSET.
    count_values: Vec<sea_orm::Value>,
    limit: i64,
    offset: i64,
}

impl Plan {
    fn data_stmt(&self) -> Statement {
        let mut values = self.count_values.clone();
        values.push(self.limit.into());
        values.push(self.offset.into());
        Statement::from_sql_and_values(DbBackend::Postgres, &self.data_sql, values)
    }

    fn count_stmt(&self) -> Statement {
        Statement::from_sql_and_values(
            DbBackend::Postgres,
            &self.count_sql,
            self.count_values.clone(),
        )
    }
}

/// Builds a prefix-match tsquery: `sofa da` → `sofa:* & da:*`.
///
/// Terms are stripped of everything but alphanumerics and marks before they are
/// interpolated, because `to_tsquery` parses its input as an expression — an
/// unescaped `&`, `|`, `!` or `(` in user text is a syntax error at best and a
/// query-shape injection at worst. `f_unaccent` is applied by Postgres on both
/// sides, so a query typed without tone marks still matches an accented title.
fn build_tsquery(raw: &str) -> Option<String> {
    let terms: Vec<String> = raw
        .split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .map(|w| format!("{}:*", w))
        .collect();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" & "))
    }
}

/// Threads whose *body* matches, as a subquery over `idx_posts_fts`.
///
/// F-ORG-03 asks for "thread title + post content", and the index for the second
/// half has existed since migration 6 — but nothing queried it, so a term that
/// appeared only in a reply was unfindable. On a discussion forum that is most
/// of the text on the site: the title is one line, the answers are the substance.
///
/// Uncorrelated `IN`, exactly like [`BRAND_MATCH_SUBQUERY`] and for the same
/// reason: an `OR` spanning two tables cannot use either index, so written as a
/// join Postgres would sequentially scan `threads`. In this shape it runs one
/// GIN scan over `posts`, hashes the thread ids, and BitmapOrs that against the
/// title index.
///
/// Only `published` posts, and never a soft-deleted one. A pending post is
/// awaiting moderation and must not be reachable — not even by its own author,
/// who can already see it in the thread. Making that viewer-dependent would put
/// a viewer term into `count_sql` for a case nobody asked for.
///
/// Public only so the test suite can `EXPLAIN` this exact string. The
/// `to_tsvector(...)` here has to match `idx_posts_fts` character for character
/// or Postgres quietly ignores the index and sequentially scans `posts` on every
/// search — a failure with no error and no wrong answer, only a bill. Asserting
/// against a copy of the text would assert against the copy, so the test reads
/// the real one.
#[doc(hidden)]
pub const POST_MATCH_SUBQUERY: &str = "SELECT po.thread_id FROM posts po \
     WHERE po.is_deleted = false \
       AND po.status = 'published'::post_status \
       AND to_tsvector('simple', f_unaccent(po.content_md)) \
           @@ to_tsquery('simple', f_unaccent($1))";

/// What a body-only match contributes to a thread's relevance score.
///
/// Deliberately below what a title match scores. `to_tsvector` assigns weight D
/// when nothing calls `setweight`, and `ts_rank`'s default weight for D is 0.1,
/// which puts a single-lexeme title hit at ≈0.0608. Anything at or above that
/// would let a passing mention in a reply outrank a thread whose title is the
/// query — the opposite of what a searcher means. Half of it keeps the two tiers
/// ordered while still giving body-only hits a score to sort by; at 0 they would
/// all tie and fall through to `created_at`.
const POST_MATCH_RANK_BONUS: f32 = 0.03;

/// Threads: title or post-body match, excluding deleted threads and reviews of
/// products that are still drafts (mirrors the public feed — a pending product's
/// review is unlisted until approval). Category visibility is enforced by the
/// caller supplying the allowed set; an empty set means no hits at all.
fn plan_threads(tsquery: &str, query: &SearchQuery) -> Option<Plan> {
    if query.visible_category_ids.is_empty() {
        return None;
    }

    let mut values: Vec<sea_orm::Value> = vec![tsquery.to_owned().into()];
    let placeholders = query
        .visible_category_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 2))
        .collect::<Vec<_>>()
        .join(", ");
    for id in &query.visible_category_ids {
        values.push((*id).into());
    }

    let where_sql = format!(
        "WHERE (to_tsvector('simple', f_unaccent(t.title)) \
                    @@ to_tsquery('simple', f_unaccent($1)) \
                OR t.id IN ({POST_MATCH_SUBQUERY})) \
           AND t.deleted_at IS NULL \
           AND t.status != 'deleted'::thread_status \
           AND t.category_id IN ({placeholders}) \
           AND (t.product_id IS NULL OR EXISTS ( \
                   SELECT 1 FROM products pr \
                   WHERE pr.id = t.product_id AND pr.status = 'published'))"
    );

    let limit_idx = values.len() + 1;
    let offset_idx = values.len() + 2;

    // Each ordering is reduced to one numeric key, selected as a column, so the
    // outer query can re-state the sort without recomputing it. That matters
    // because the body snippet is joined on *after* the page is chosen: a
    // subquery's ORDER BY is not carried through a join by any rule Postgres
    // guarantees, so an outer ORDER BY is required and it must key off something
    // the inner query actually emits.
    //
    // Every ordering still ends with `created_at DESC` as a deterministic
    // tiebreak, so paging is total: without it two equally-ranked (or equal
    // reply_count) threads can swap between pages and the reader sees one twice.
    //
    // Relevance repeats the body subquery rather than joining its result in,
    // because `count_sql` shares `where_sql` and has no joins of its own — the
    // same constraint the brand-match ranking works around. Both occurrences are
    // uncorrelated, so each is one hashed subplan, not a scan per row.
    let sort_key = match query.thread_sort {
        ThreadSearchSort::Relevance => format!(
            "(ts_rank(to_tsvector('simple', f_unaccent(t.title)), \
                      to_tsquery('simple', f_unaccent($1))) \
              + CASE WHEN t.id IN ({POST_MATCH_SUBQUERY}) \
                     THEN {POST_MATCH_RANK_BONUS} ELSE 0 END)"
        ),
        // A constant leaves `created_at DESC` as the only live term.
        ThreadSearchSort::Newest => "0::real".to_string(),
        ThreadSearchSort::MostReplies => "t.reply_count::real".to_string(),
    };

    Some(Plan {
        // The page is selected first and the body snippet joined onto it
        // afterwards, so the LATERAL runs `per_page` times rather than once per
        // matching thread — `ts_headline` reads the whole document, and a
        // popular query can match thousands.
        //
        // Preferring the post snippet over the title one is not a tiebreak but
        // the point: `title` is already a field on the hit, so headlining it
        // again says nothing the reader cannot see. A line of the reply that
        // actually contains the term does.
        data_sql: format!(
            r#"
            SELECT
                hit.id   AS id,
                hit.slug AS slug,
                hit.title,
                COALESCE(
                    body.excerpt,
                    ts_headline(
                        'simple', hit.title,
                        to_tsquery('simple', f_unaccent($1)),
                        'MaxFragments=1,MinWords=6,MaxWords=20'
                    )
                ) AS excerpt
            FROM (
                SELECT t.id, t.slug, t.title, t.created_at,
                       {sort_key} AS sort_key
                FROM threads t
                {where_sql}
                ORDER BY sort_key DESC, t.created_at DESC
                LIMIT ${limit_idx} OFFSET ${offset_idx}
            ) hit
            LEFT JOIN LATERAL (
                SELECT ts_headline(
                           'simple', po.content_md,
                           to_tsquery('simple', f_unaccent($1)),
                           'MaxFragments=1,MinWords=8,MaxWords=28'
                       ) AS excerpt
                FROM posts po
                WHERE po.thread_id = hit.id
                  AND po.is_deleted = false
                  AND po.status = 'published'::post_status
                  AND to_tsvector('simple', f_unaccent(po.content_md))
                      @@ to_tsquery('simple', f_unaccent($1))
                ORDER BY ts_rank(to_tsvector('simple', f_unaccent(po.content_md)),
                                 to_tsquery('simple', f_unaccent($1))) DESC,
                         po.created_at ASC
                LIMIT 1
            ) body ON true
            ORDER BY hit.sort_key DESC, hit.created_at DESC
            "#
        ),
        count_sql: format!("SELECT COUNT(*)::BIGINT AS count FROM threads t {where_sql}"),
        count_values: values,
        limit: query.per_page as i64,
        offset: ((query.page.saturating_sub(1)) * query.per_page) as i64,
    })
}

fn product_type_str(t: ProductType) -> &'static str {
    match t {
        ProductType::Furniture => "furniture",
        ProductType::Material => "material",
        ProductType::Room => "room",
    }
}

/// Brands whose name matches the query, as a subquery over `idx_brands_fts`.
///
/// A searcher types "Nhà Xinh" without knowing or caring that the brand lives in
/// another table. This used to be handled by denormalising the brand name into
/// `products.search_vector`, which meant a rename had to fan out across every
/// product that brand owned; reaching the real table instead means there is
/// nothing to keep in sync.
const BRAND_MATCH_SUBQUERY: &str = "SELECT b2.id FROM brands b2 \
     WHERE to_tsvector('simple', f_unaccent(b2.name)) @@ to_tsquery('simple', f_unaccent($1))";

/// What a brand match adds to a product's relevance score.
///
/// Roughly what a single lexeme at weight B contributed back when the brand name
/// was part of the vector, so the intended ordering survives the change: a hit
/// on the product's own name still outranks a hit on its brand, and a brand-only
/// hit still outranks nothing. Without a term of its own a brand-only match
/// would score exactly 0 and the Relevance sort would order those hits by
/// nothing at all.
const BRAND_MATCH_RANK_BONUS: f32 = 0.25;

/// The `ORDER BY` for a product result page.
///
/// Every ordering falls back to `ts_rank` and then `created_at`, so the sort is
/// total: without a deterministic tiebreak, two products with the same rating
/// can swap places between page 1 and page 2 and the reader sees one twice
/// while never seeing the other.
///
/// `TopRated` uses the same Bayesian shrinkage as the catalogue
/// (`ProductListFilter`'s `bayesian` module) rather than a raw average — one
/// 5★ review must not outrank a 4.6★ backed by two hundred. The `CASE` keeps
/// unrated products NULL so they sort last instead of inheriting the 3.5 prior
/// and landing above products that genuinely scored below it.
fn product_order_by(sort: ProductSearchSort, rank_expr: &str) -> String {
    match sort {
        ProductSearchSort::Relevance => format!("{rank_expr} DESC, p.created_at DESC"),
        ProductSearchSort::Newest => format!("p.created_at DESC, {rank_expr} DESC"),
        ProductSearchSort::MostReviewed => format!(
            "prs.review_count DESC NULLS LAST, {rank_expr} DESC, p.created_at DESC"
        ),
        ProductSearchSort::TopRated => format!(
            "CASE WHEN COALESCE(prs.review_count, 0) > 0 \
             THEN (prs.avg_overall * prs.review_count + {mean} * {weight}) \
                  / (prs.review_count + {weight}) \
             END DESC NULLS LAST, \
             prs.review_count DESC NULLS LAST, {rank_expr} DESC, p.created_at DESC",
            mean = bayesian::PRIOR_MEAN,
            weight = bayesian::PRIOR_WEIGHT,
        ),
    }
}

/// Products: weighted vector match over name / brand / style / origin /
/// description, narrowed by the catalogue facets. Only published products are
/// public; a submitter additionally sees their own drafts, matching
/// `ProductListFilter::include_own`.
fn plan_products(tsquery: &str, query: &SearchQuery) -> Plan {
    let mut values: Vec<sea_orm::Value> = vec![tsquery.to_owned().into()];
    let mut clauses: Vec<String> = Vec::new();

    match query.viewer_id {
        Some(viewer) => {
            values.push(viewer.into());
            clauses.push(format!(
                "AND (p.status = 'published' OR p.created_by_id = ${})",
                values.len()
            ));
        }
        None => clauses.push("AND p.status = 'published'".to_string()),
    }

    if !query.in_category_ids.is_empty() {
        let placeholders = query
            .in_category_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("${}", values.len() + 1 + i))
            .collect::<Vec<_>>()
            .join(", ");
        for id in &query.in_category_ids {
            values.push((*id).into());
        }
        // No `OR p.category_id IS NULL`: an unfiled product is not in the
        // selected category, and quietly including it would mean the filter
        // never actually excludes anything.
        clauses.push(format!("AND p.category_id IN ({placeholders})"));
    }

    if let Some(t) = query.facets.product_type {
        values.push(product_type_str(t).into());
        clauses.push(format!("AND p.product_type = ${}::product_type", values.len()));
    }
    if let Some(brand_id) = query.facets.brand_id {
        values.push(brand_id.into());
        clauses.push(format!("AND p.brand_id = ${}", values.len()));
    }
    if let Some(material_id) = query.facets.material_id {
        values.push(material_id.into());
        clauses.push(format!(
            "AND EXISTS (SELECT 1 FROM product_materials pm \
                         WHERE pm.product_id = p.id AND pm.material_id = ${})",
            values.len()
        ));
    }

    // Two independently indexed conditions, ORed: the product's own vector, and
    // "this product belongs to a brand whose name matches". The brand half is a
    // semi-join rather than `OR b.name @@ q` over a join because an OR spanning
    // two tables cannot use either index — Postgres would sequentially scan
    // `products`. In this shape it can BitmapOr the GIN scan with a brand_id
    // lookup. The brand name is not copied into the product's vector, so a brand
    // rename takes effect on the next search with nothing to re-index.
    let where_sql = format!(
        "WHERE (p.search_vector @@ to_tsquery('simple', f_unaccent($1)) \
                OR p.brand_id IN ({BRAND_MATCH_SUBQUERY})) {}",
        clauses.join(" ")
    );

    // The rating rollup is joined for every sort, not just the rating-based
    // ones: it is a 1:1 rollup so it cannot multiply rows, and branching the
    // FROM clause on the sort would make the count and data queries diverge.
    // `brands` is joined for ranking only — the WHERE clause above must stay
    // free of it so `count_sql`, which has no joins, can share it.
    let rank_expr = format!(
        "(ts_rank(p.search_vector, to_tsquery('simple', f_unaccent($1))) \
          + CASE WHEN to_tsvector('simple', f_unaccent(coalesce(b.name, ''))) \
                      @@ to_tsquery('simple', f_unaccent($1)) \
                 THEN {BRAND_MATCH_RANK_BONUS} ELSE 0 END)"
    );
    let order_by = product_order_by(query.facets.sort, &rank_expr);

    let limit_idx = values.len() + 1;
    let offset_idx = values.len() + 2;

    Plan {
        // The excerpt is the description, headlined — it is the only prose a
        // product has. NULL when there is none; the card falls back to price
        // and rating, which is what a product card leads with anyway.
        data_sql: format!(
            r#"
            SELECT
                p.id   AS id,
                p.slug AS slug,
                p.name AS title,
                ts_headline(
                    'simple', coalesce(p.description_md, ''),
                    to_tsquery('simple', f_unaccent($1)),
                    'MaxFragments=1,MinWords=6,MaxWords=24'
                )      AS excerpt
            FROM products p
            LEFT JOIN product_rating_stats prs ON prs.product_id = p.id
            LEFT JOIN brands b ON b.id = p.brand_id
            {where_sql}
            ORDER BY {order_by}
            LIMIT ${limit_idx} OFFSET ${offset_idx}
            "#
        ),
        count_sql: format!("SELECT COUNT(*)::BIGINT AS count FROM products p {where_sql}"),
        count_values: values,
        limit: query.per_page as i64,
        offset: ((query.page.saturating_sub(1)) * query.per_page) as i64,
    }
}

const EMPTY: SearchResults = SearchResults {
    hits: Vec::new(),
    total: 0,
};

#[async_trait]
impl SearchService for PostgresFtsService {
    async fn search(&self, query: SearchQuery) -> Result<SearchResults, AppError> {
        let Some(tsquery) = build_tsquery(query.q.trim()) else {
            return Ok(EMPTY);
        };

        let plan = match query.kind {
            SearchKind::Thread => match plan_threads(&tsquery, &query) {
                Some(p) => p,
                None => return Ok(EMPTY),
            },
            SearchKind::Product => plan_products(&tsquery, &query),
        };

        if query.count_only {
            let count_row = CountRow::find_by_statement(plan.count_stmt())
                .one(&self.db)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, q = %query.q, "fts_count_failed");
                    AppError::from(e)
                })?;
            return Ok(SearchResults {
                hits: vec![],
                total: count_row.and_then(|r| u64::try_from(r.count).ok()).unwrap_or(0),
            });
        }

        // Rows and total run concurrently — neither depends on the other.
        let (rows, count_row) = tokio::try_join!(
            FtsRow::find_by_statement(plan.data_stmt()).all(&self.db),
            CountRow::find_by_statement(plan.count_stmt()).one(&self.db),
        )
        .map_err(|e| {
            tracing::error!(error = %e, q = %query.q, "fts_search_failed");
            AppError::from(e)
        })?;

        let total = count_row.and_then(|r| u64::try_from(r.count).ok()).unwrap_or(0);

        let hits = rows
            .into_iter()
            .map(|r| SearchHit {
                kind: query.kind,
                id: r.id,
                slug: r.slug,
                title: r.title,
                // ts_headline wraps matched terms in <b>; strip everything else
                // to prevent XSS, and drop an excerpt that carries no text.
                excerpt: r
                    .excerpt
                    .map(sanitize_headline)
                    .filter(|s| !s.trim().is_empty()),
            })
            .collect();

        Ok(SearchResults { hits, total })
    }
}
