use sea_orm_migration::prelude::*;

#[derive(Iden)]
pub enum ReactionKindEnum {
    #[iden = "reaction_kind"]
    Type,
    #[iden = "like"]
    Like,
    #[iden = "helpful"]
    Helpful,
    #[iden = "insightful"]
    Insightful,
    #[iden = "funny"]
    Funny,
}
