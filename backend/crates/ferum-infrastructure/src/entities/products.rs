use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "products")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub slug: String,
    pub name: String,
    pub product_type: ProductType,
    pub status: ProductStatus,
    pub brand_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub style: Option<String>,
    pub price_min: Option<i32>,
    pub price_max: Option<i32>,
    pub currency: String,
    pub dimensions: Value,
    pub origin: Option<String>,
    pub primary_image_key: Option<String>,
    pub description_md: Option<String>,
    pub created_by_id: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::brands::Entity",
        from = "Column::BrandId",
        to = "super::brands::Column::Id"
    )]
    Brand,
    #[sea_orm(
        belongs_to = "super::categories::Entity",
        from = "Column::CategoryId",
        to = "super::categories::Column::Id"
    )]
    Category,
    #[sea_orm(has_many = "super::product_media::Entity")]
    Media,
    #[sea_orm(has_many = "super::product_materials::Entity")]
    ProductMaterials,
}

impl Related<super::brands::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Brand.def()
    }
}

impl Related<super::categories::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Category.def()
    }
}

impl Related<super::product_media::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Media.def()
    }
}

impl Related<super::product_materials::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ProductMaterials.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "product_type")]
pub enum ProductType {
    #[sea_orm(string_value = "furniture")]
    Furniture,
    #[sea_orm(string_value = "material")]
    Material,
    #[sea_orm(string_value = "room")]
    Room,
}

#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "product_status")]
pub enum ProductStatus {
    #[sea_orm(string_value = "draft")]
    Draft,
    #[sea_orm(string_value = "published")]
    Published,
    #[sea_orm(string_value = "archived")]
    Archived,
}
