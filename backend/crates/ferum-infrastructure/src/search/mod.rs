#[cfg(feature = "meilisearch")]
pub mod meilisearch;
pub mod postgres_fts;

#[cfg(feature = "meilisearch")]
pub use meilisearch::MeilisearchService;
pub use postgres_fts::PostgresFtsService;
