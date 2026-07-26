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
// The catalogue's `?q=` used to be an ILIKE on `name` — no ranking, no prefix
// match, and blind to brand, style and description. On a review platform the
// product *is* the primary entity, so a global search that cannot find one is
// the wrong shape.
//
// Everything indexed here goes through `f_unaccent` (defined in migration
// 000001), and the query side does the same: unaccenting only one side silently
// returns nothing.
//
// ── Why a generated column and not a trigger ─────────────────────────────────
//
// `search_vector` is `GENERATED ALWAYS AS … STORED`: declarative schema rather
// than procedural code. It buys three things a BEFORE-INSERT trigger does not.
// It is visible in `\d products`, so the next person to read the schema learns
// how the vector is built without going hunting for a plpgsql function. No
// write path can bypass or forget it — including the ones that insert through
// Sea-ORM ActiveModels. And adding the column computes it for every existing
// row, so there is no backfill statement whose absence would leave rows
// invisible to search until someone happened to edit them.
//
// The constraint a generated column imposes is that the expression may only
// read its own row. That is the whole reason the brand name is no longer folded
// in here — and losing it is a gain, not a compromise. Denormalising the brand
// meant a rename had to fan back out: a second trigger rewrote every product
// row that brand owned, purely to re-fire the first trigger. One UPDATE on one
// tiny table amplified into an UPDATE per product, and any row it missed stayed
// indexed under a name that no longer existed. Nothing is copied now, so there
// is nothing to keep in sync and nothing to go stale.
//
// A search for a brand name still finds that brand's products: `brands` carries
// its own GIN index and the query reaches it through a semi-join
// (`p.brand_id IN (SELECT …)`). The semi-join shape is deliberate — it keeps the
// two conditions on separate tables so the planner can BitmapOr the products
// GIN scan with a brand_id lookup. Written as `OR b.name @@ q` over a join, the
// OR spans two tables and Postgres falls back to a sequential scan of products.
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
