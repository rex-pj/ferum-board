use sea_orm_migration::prelude::*;
use sea_orm::ConnectionTrait;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000031_create_product_search"
    }
}

// Full-text search over products.
//
// Indexed through `f_unaccent` (migration 000001); the query side must do the
// same, or unaccenting one side alone silently returns nothing.
//
// `GENERATED ALWAYS AS … STORED`, not a trigger: no write path can bypass it,
// and adding the column backfills every existing row. The cost is that the
// expression may read only its own row, so the brand name is NOT folded in —
// which also removes the rename fan-out a denormalised copy required.
//
// Brand search still works via a semi-join (`p.brand_id IN (SELECT …)`) against
// the `brands` GIN index. Written as `OR b.name @@ q` over a join the OR spans
// two tables and Postgres sequentially scans products.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // Weights encode what a searcher actually typed. A: the product's own
        // name. B: style — "scandinavian" is how people search when they cannot
        // recall the exact model. C: origin and the description body, which
        // match broadly and must not outrank a name.
        //
        // Every function here is IMMUTABLE, which a generated column requires:
        // `setweight`, the two-argument `to_tsvector(regconfig, text)` — note
        // the explicit 'simple', the one-argument form is only STABLE — and
        // `f_unaccent`, which migration 000001 declares immutable by pinning its
        // dictionary argument.
        conn.execute_unprepared(
            "ALTER TABLE products ADD COLUMN search_vector tsvector \
             GENERATED ALWAYS AS ( \
                 setweight(to_tsvector('simple', f_unaccent(coalesce(name, ''))),           'A') || \
                 setweight(to_tsvector('simple', f_unaccent(coalesce(style, ''))),          'B') || \
                 setweight(to_tsvector('simple', f_unaccent(coalesce(origin, ''))),         'C') || \
                 setweight(to_tsvector('simple', f_unaccent(coalesce(description_md, ''))), 'C') \
             ) STORED",
        )
        .await?;

        conn.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_products_fts ON products USING GIN(search_vector)",
        )
        .await?;

        // The brand side of the semi-join. An expression index rather than a
        // column: `brands` is small and read far less often than `products`, so
        // there is nothing to gain from materialising a vector for it.
        conn.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_brands_fts ON brands \
             USING GIN(to_tsvector('simple', f_unaccent(name)))",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared("DROP INDEX IF EXISTS idx_brands_fts").await?;
        conn.execute_unprepared("DROP INDEX IF EXISTS idx_products_fts").await?;
        conn.execute_unprepared("ALTER TABLE products DROP COLUMN IF EXISTS search_vector").await?;

        // f_unaccent is left alone: it is owned by migration 000001 and the
        // thread and post indexes still depend on it.
        Ok(())
    }
}
