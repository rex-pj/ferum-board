use serde::{Deserialize, Serialize};

use ferum_domain::models::reaction::ReactionKind;

#[derive(Debug, Deserialize)]
pub struct AddReactionRequest {
    pub kind: String,
}

#[derive(Serialize)]
pub struct ReactionCountResponse {
    pub kind: String,
    pub count: u64,
}

pub fn counts_to_response(counts: Vec<(ReactionKind, u64)>) -> Vec<ReactionCountResponse> {
    counts
        .into_iter()
        .map(|(k, c)| ReactionCountResponse {
            kind: k.as_str().to_string(),
            count: c,
        })
        .collect()
}
