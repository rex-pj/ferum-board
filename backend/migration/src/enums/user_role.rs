use sea_orm_migration::prelude::*;

#[derive(Iden)]
pub enum UserRoleEnum {
    #[iden = "user_role"]
    Type,
    #[iden = "member"]
    Member,
    #[iden = "moderator"]
    Moderator,
    #[iden = "admin"]
    Admin,
}
