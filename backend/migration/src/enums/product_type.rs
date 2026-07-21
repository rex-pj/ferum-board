use sea_orm_migration::prelude::*;

#[derive(Iden)]
pub enum ProductTypeEnum {
    #[iden = "product_type"]
    Type,
    #[iden = "furniture"]
    Furniture,
    #[iden = "material"]
    Material,
    #[iden = "room"]
    Room,
}
