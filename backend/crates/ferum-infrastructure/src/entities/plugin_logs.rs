use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "plugin_logs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub plugin_id: Uuid,
    pub level: String,
    pub hook_name: Option<String>,
    pub duration_ms: Option<i32>,
    pub message: String,
    pub context: Option<Value>,
    pub created_at: DateTimeWithTimeZone,
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
