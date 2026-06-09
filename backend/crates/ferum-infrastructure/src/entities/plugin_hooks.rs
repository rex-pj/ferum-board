use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "plugin_hooks")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub plugin_id: Uuid,
    pub hook_name: String,
    pub priority: i32,
    pub is_active: bool,
    pub avg_ms: Option<i32>,
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
