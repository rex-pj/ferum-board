use sea_orm_migration::prelude::*;

#[derive(Iden)]
pub enum PostStatusEnum {
    #[iden = "post_status"]
    Type,
    #[iden = "pending"]
    Pending,
    #[iden = "published"]
    Published,
}
