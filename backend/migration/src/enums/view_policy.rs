use sea_orm_migration::prelude::*;

#[derive(Iden)]
pub enum ViewPolicyEnum {
    #[iden = "view_policy"]
    Type,
    #[iden = "public"]
    Public,
    #[iden = "members_only"]
    MembersOnly,
    #[iden = "staff_only"]
    StaffOnly,
}
