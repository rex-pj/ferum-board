use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "plugins")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub slug: String,
    pub name: String,
    pub version: String,
    pub tier: PluginTierEntity,
    pub status: PluginStatusEntity,
    pub manifest: Value,
    pub config: Value,
    pub granted_capabilities: Value,
    pub install_path: String,
    pub db_schema_name: Option<String>,
    pub db_schema_version: i32,
    pub installed_by: Option<Uuid>,
    pub installed_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub activated_at: Option<DateTimeWithTimeZone>,
    pub error_message: Option<String>,
    pub last_seen_at: Option<DateTimeWithTimeZone>,
    pub restart_count: i32,
    pub circuit_open: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "plugin_tier")]
pub enum PluginTierEntity {
    #[sea_orm(string_value = "manifest")]
    Manifest,
    #[sea_orm(string_value = "script")]
    Script,
    #[sea_orm(string_value = "service")]
    Service,
}

#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "plugin_status")]
pub enum PluginStatusEntity {
    #[sea_orm(string_value = "installing")]
    Installing,
    #[sea_orm(string_value = "active")]
    Active,
    #[sea_orm(string_value = "inactive")]
    Inactive,
    #[sea_orm(string_value = "error")]
    Error,
    #[sea_orm(string_value = "disabled")]
    Disabled,
    #[sea_orm(string_value = "uninstalling")]
    Uninstalling,
}
