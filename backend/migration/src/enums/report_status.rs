use sea_orm_migration::prelude::*;

#[derive(Iden)]
pub enum ReportStatusEnum {
    #[iden = "report_status"]
    Type,
    #[iden = "pending"]
    Pending,
    #[iden = "resolved"]
    Resolved,
    #[iden = "dismissed"]
    Dismissed,
}
