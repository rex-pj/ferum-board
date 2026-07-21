use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

// Aggregate-first, anonymized rollup of review_ratings per product. Contains no
// personal data; safe to expose in aggregate market-data views. Averages are
// numeric(4,2) — exact, so ratings never carry floating-point display jitter.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "product_rating_stats")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub product_id: Uuid,
    pub review_count: i32,
    pub avg_overall: Option<Decimal>,
    pub avg_durability: Option<Decimal>,
    pub avg_materials: Option<Decimal>,
    pub avg_comfort: Option<Decimal>,
    pub avg_aesthetics: Option<Decimal>,
    pub avg_value_for_money: Option<Decimal>,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::products::Entity",
        from = "Column::ProductId",
        to = "super::products::Column::Id"
    )]
    Product,
}

impl Related<super::products::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Product.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
