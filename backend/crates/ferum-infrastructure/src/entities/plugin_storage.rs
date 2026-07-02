use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "plugin_storage")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub plugin_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub key: String,
    pub value: Value,
    pub updated_at: DateTimeWithTimeZone,
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
