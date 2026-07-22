use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Structured score attached to a review thread. `overall` is required (1..=5);
/// sub-scores are optional so a reviewer can rate only what they care about.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewRating {
    pub thread_id: Uuid,
    pub overall: i16,
    pub durability: Option<i16>,
    pub materials: Option<i16>,
    pub comfort: Option<i16>,
    pub aesthetics: Option<i16>,
    pub value_for_money: Option<i16>,
    pub verified_purchase: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

pub struct NewReviewRating {
    pub thread_id: Uuid,
    pub overall: i16,
    pub durability: Option<i16>,
    pub materials: Option<i16>,
    pub comfort: Option<i16>,
    pub aesthetics: Option<i16>,
    pub value_for_money: Option<i16>,
    pub verified_purchase: bool,
}

/// Lowest score a reviewer can give. Exposed so the validation error can report
/// the bound instead of restating it as a literal in each translation.
pub const MIN_RATING: i16 = 1;
/// Highest score a reviewer can give.
pub const MAX_RATING: i16 = 5;

impl NewReviewRating {
    /// Every provided score must fall within `MIN_RATING..=MAX_RATING`.
    pub fn scores_in_range(&self) -> bool {
        let range = MIN_RATING..=MAX_RATING;
        let ok = |v: Option<i16>| v.map(|n| range.contains(&n)).unwrap_or(true);
        range.contains(&self.overall)
            && ok(self.durability)
            && ok(self.materials)
            && ok(self.comfort)
            && ok(self.aesthetics)
            && ok(self.value_for_money)
    }
}
