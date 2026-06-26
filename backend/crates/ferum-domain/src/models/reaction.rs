
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reaction {
    pub post_id: Uuid,
    pub user_id: Uuid,
    pub kind: ReactionKind,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash, Copy)]
pub enum ReactionKind {
    Like,
    Helpful,
    Insightful,
    Funny,
}

impl ReactionKind {
    pub fn trust_score_weight(&self) -> i32 {
        match self {
            ReactionKind::Like => 1,
            ReactionKind::Helpful => 3,
            ReactionKind::Insightful => 2,
            ReactionKind::Funny => 0,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ReactionKind::Like => "like",
            ReactionKind::Helpful => "helpful",
            ReactionKind::Insightful => "insightful",
            ReactionKind::Funny => "funny",
        }
    }
}

impl std::str::FromStr for ReactionKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "like" => Ok(ReactionKind::Like),
            "helpful" => Ok(ReactionKind::Helpful),
            "insightful" => Ok(ReactionKind::Insightful),
            "funny" => Ok(ReactionKind::Funny),
            _ => Err(format!("unknown reaction kind: {}", s)),
        }
    }
}
