use async_trait::async_trait;
use sea_orm::{DatabaseConnection, DbBackend, FromQueryResult, Statement};
use uuid::Uuid;

fn sanitize_headline(html: String) -> String {
    ammonia::Builder::new()
        .tags(["b"].iter().cloned().collect())
        .clean(&html)
        .to_string()
}

use ferum_application::ports::{SearchHit, SearchQuery, SearchResults, SearchService};
use ferum_application::shared::AppError;

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
    thread_id: Uuid,
    thread_slug: String,
    title: String,
    excerpt: Option<String>,
}

#[derive(Debug, FromQueryResult)]
struct CountRow {
    count: i64,
}

#[async_trait]
impl SearchService for PostgresFtsService {
    async fn search(&self, query: SearchQuery) -> Result<SearchResults, AppError> {
        if query.q.trim().is_empty() {
            return Ok(SearchResults {
                hits: vec![],
                total: 0,
            });
        }

        // Build prefix-match tsquery: "hello world" → "hello:* & world:*"
        let tsquery = query
            .q
            .trim()
            .split_whitespace()
            .map(|w| format!("{}:*", w))
            .collect::<Vec<_>>()
            .join(" & ");

        let offset = (query.page.saturating_sub(1)) * query.per_page;

        // $1 = tsquery string (reused for match, rank, headline)
        // $2 = category_id filter (optional, appended below)
        // last two params = LIMIT, OFFSET
        let (cat_clause, mut values): (&str, Vec<sea_orm::Value>) = match query.category_id {
            Some(cat_id) => ("AND t.category_id = $2", vec![tsquery.clone().into(), cat_id.into()]),
            None => ("", vec![tsquery.clone().into()]),
        };

        let limit_idx = values.len() + 1;
        let offset_idx = values.len() + 2;
        values.push((query.per_page as i64).into());
        values.push((offset as i64).into());

        let data_sql = format!(
            r#"
            SELECT
                t.id         AS thread_id,
                t.slug       AS thread_slug,
                t.title,
                ts_headline(
                    'simple', t.title,
                    to_tsquery('simple', $1),
                    'MaxFragments=1,MinWords=6,MaxWords=20'
                )            AS excerpt
            FROM threads t
            WHERE to_tsvector('simple', t.title) @@ to_tsquery('simple', $1)
              AND t.deleted_at IS NULL
              AND t.status != 'deleted'::thread_status
              {cat_clause}
            ORDER BY ts_rank(to_tsvector('simple', t.title), to_tsquery('simple', $1)) DESC
            LIMIT ${limit_idx} OFFSET ${offset_idx}
            "#
        );

        let count_sql = format!(
            r#"
            SELECT COUNT(*)::BIGINT AS count
            FROM threads t
            WHERE to_tsvector('simple', t.title) @@ to_tsquery('simple', $1)
              AND t.deleted_at IS NULL
              AND t.status != 'deleted'::thread_status
              {cat_clause}
            "#
        );

        // Run data + count queries concurrently
        let data_stmt = Statement::from_sql_and_values(DbBackend::Postgres, &data_sql, values.clone());
        let count_stmt = Statement::from_sql_and_values(DbBackend::Postgres, &count_sql, values[..values.len() - 2].to_vec());

        let (rows, count_row) = tokio::try_join!(
            FtsRow::find_by_statement(data_stmt).all(&self.db),
            CountRow::find_by_statement(count_stmt).one(&self.db),
        )
        .map_err(|e| {
            tracing::error!(error = %e, q = %query.q, "fts_search_failed");
            AppError::from(e)
        })?;

        let total = count_row.and_then(|r| u64::try_from(r.count).ok()).unwrap_or(0);

        let hits = rows
            .into_iter()
            .map(|r| SearchHit {
                thread_id: r.thread_id,
                thread_slug: r.thread_slug,
                title: r.title,
                // ts_headline wraps matched terms in <b>; strip everything else to prevent XSS.
                excerpt: r.excerpt.map(sanitize_headline),
            })
            .collect();

        Ok(SearchResults { hits, total })
    }
}
