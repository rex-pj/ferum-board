use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Aggregate-first, anonymized rollup of `ReviewRating` per product. Holds no
/// personal data — this is the row the aggregate market-data layer reads from.
/// Averages are exact decimals (numeric(4,2)) rather than floats.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductRatingStats {
    pub product_id: Uuid,
    pub review_count: i32,
    pub avg_overall: Option<Decimal>,
    pub avg_durability: Option<Decimal>,
    pub avg_materials: Option<Decimal>,
    pub avg_comfort: Option<Decimal>,
    pub avg_aesthetics: Option<Decimal>,
    pub avg_value_for_money: Option<Decimal>,
    pub updated_at: DateTime<Utc>,
}
