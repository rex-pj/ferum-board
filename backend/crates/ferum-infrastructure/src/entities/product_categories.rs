use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "product_categories")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub slug: String,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub position: i32,
    pub icon: Option<String>,
    pub match_keywords: Vec<String>,
    pub created_at: DateTimeWithTimeZone,
}

/// Only the self-reference is declared. The link to `products` is exercised
/// through `products.category_id` directly (and in the matcher's raw SQL), so a
/// `has_many` here would only be a second, unused way to say the same thing —
/// and one that requires a matching `belongs_to` on the products entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "Entity",
        from = "Column::ParentId",
        to = "Column::Id",
        on_delete = "Restrict"
    )]
    Parent,
}

impl ActiveModelBehavior for ActiveModel {}
