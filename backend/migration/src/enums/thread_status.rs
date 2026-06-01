use sea_orm_migration::prelude::*;

#[derive(Iden)]
pub enum ThreadStatusEnum {
    #[iden = "thread_status"]
    Type,
    #[iden = "open"]
    Open,
    #[iden = "locked"]
    Locked,
    #[iden = "deleted"]
    Deleted,
}
