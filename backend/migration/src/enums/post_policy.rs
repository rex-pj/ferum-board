use sea_orm_migration::prelude::*;

#[derive(Iden)]
pub enum PostPolicyEnum {
    #[iden = "post_policy"]
    Type,
    #[iden = "members"]
    Members,
    #[iden = "trusted"]
    Trusted,
    #[iden = "staff_only"]
    StaffOnly,
    #[iden = "closed"]
    Closed,
}
