use sea_orm_migration::prelude::*;

#[derive(Iden)]
pub enum ProductStatusEnum {
    #[iden = "product_status"]
    Type,
    #[iden = "draft"]
    Draft,
    #[iden = "published"]
    Published,
    #[iden = "archived"]
    Archived,
}
