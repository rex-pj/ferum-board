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

impl NewReviewRating {
    /// Every provided score must fall within 1..=5.
    pub fn scores_in_range(&self) -> bool {
        let ok = |v: Option<i16>| v.map(|n| (1..=5).contains(&n)).unwrap_or(true);
        (1..=5).contains(&self.overall)
            && ok(self.durability)
            && ok(self.materials)
            && ok(self.comfort)
            && ok(self.aesthetics)
            && ok(self.value_for_money)
    }
}
