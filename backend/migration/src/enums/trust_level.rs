use sea_orm_migration::prelude::*;

#[derive(Iden)]
pub enum TrustLevelEnum {
    #[iden = "trust_level"]
    Type,
    #[iden = "new"]
    New,
    #[iden = "basic"]
    Basic,
    #[iden = "member"]
    Member,
    #[iden = "regular"]
    Regular,
    #[iden = "leader"]
    Leader,
}
