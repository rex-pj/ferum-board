use std::collections::HashMap;

use async_trait::async_trait;
use uuid::Uuid;

use ferum_domain::models::reaction::{Reaction, ReactionKind};
use ferum_domain::repositories::reaction_repository::ReactionRepository;
use ferum_domain::AppError;

mockall::mock! {
    pub ReactionRepository {}

    #[async_trait]
    impl ReactionRepository for ReactionRepository {
        async fn find(&self, post_id: Uuid, user_id: Uuid, kind: ReactionKind) -> Result<Option<Reaction>, AppError>;
        async fn counts_by_post(&self, post_id: Uuid) -> Result<Vec<(ReactionKind, u64)>, AppError>;
        async fn counts_by_posts(&self, post_ids: &[Uuid]) -> Result<HashMap<Uuid, Vec<(ReactionKind, u64)>>, AppError>;
        async fn user_reactions_for_posts(&self, user_id: Uuid, post_ids: &[Uuid]) -> Result<HashMap<Uuid, Vec<ReactionKind>>, AppError>;
        async fn add(&self, post_id: Uuid, user_id: Uuid, kind: ReactionKind) -> Result<Reaction, AppError>;
        async fn remove(&self, post_id: Uuid, user_id: Uuid, kind: ReactionKind) -> Result<(), AppError>;
    }
}
