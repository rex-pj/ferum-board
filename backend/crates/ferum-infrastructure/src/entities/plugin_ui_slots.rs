use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "plugin_ui_slots")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub plugin_id: Uuid,
    pub slot_name: String,
    pub asset_url: String,
    pub custom_element_tag: String,
    pub props: Vec<String>,
    pub load_order: i32,
    pub is_active: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::plugins::Entity",
        from = "Column::PluginId",
        to = "super::plugins::Column::Id"
    )]
    Plugin,
}

impl ActiveModelBehavior for ActiveModel {}
