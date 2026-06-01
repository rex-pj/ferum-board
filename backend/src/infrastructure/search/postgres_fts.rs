use async_trait::async_trait;
use sea_orm::{DatabaseConnection, FromQueryResult, Statement};
use sea_orm::DbBackend;
use uuid::Uuid;

use crate::application::ports::{SearchHit, SearchQuery, SearchResults, SearchService};
use crate::application::shared::AppError;

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

#[async_trait]
impl SearchService for PostgresFtsService {
    async fn search(&self, query: SearchQuery) -> Result<SearchResults, AppError> {
        if query.q.trim().is_empty() {
            return Ok(SearchResults { hits: vec![], total: 0 });
        }

        let tsquery = query.q.trim().split_whitespace()
            .map(|w| format!("{}:*", w))
            .collect::<Vec<_>>()
            .join(" & ");

        let category_filter = if query.category_id.is_some() {
            "AND t.category_id = $4"
        } else {
            ""
        };

        let offset = (query.page.saturating_sub(1)) * query.per_page;

        let sql = format!(
            r#"SELECT
                t.id AS thread_id,
                t.slug AS thread_slug,
                t.title,
                ts_headline('simple', t.title, to_tsquery('simple', $1), 'MaxFragments=1') AS excerpt
               FROM threads t
               WHERE t.search_vector @@ to_tsquery('simple', $1)
                 AND t.status != 'deleted'
                 {category_filter}
               ORDER BY ts_rank(t.search_vector, to_tsquery('simple', $1)) DESC
               LIMIT $2 OFFSET $3"#,
        );

        let stmt = if let Some(cat_id) = query.category_id {
            Statement::from_sql_and_values(
                DbBackend::Postgres,
                &sql,
                vec![
                    tsquery.into(),
                    (query.per_page as i64).into(),
                    (offset as i64).into(),
                    cat_id.into(),
                ],
            )
        } else {
            Statement::from_sql_and_values(
                DbBackend::Postgres,
                &sql,
                vec![
                    tsquery.into(),
                    (query.per_page as i64).into(),
                    (offset as i64).into(),
                ],
            )
        };

        let rows = FtsRow::find_by_statement(stmt)
            .all(&self.db)
            .await
            .map_err(AppError::from)?;

        let hits = rows
            .into_iter()
            .map(|r| SearchHit {
                thread_id: r.thread_id,
                thread_slug: r.thread_slug,
                title: r.title,
                excerpt: r.excerpt,
            })
            .collect::<Vec<_>>();

        let total = hits.len() as u64;

        Ok(SearchResults { hits, total })
    }
}
