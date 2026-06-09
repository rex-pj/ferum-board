use sea_orm_migration::prelude::*;

#[derive(Iden)]
pub enum PluginStatusEnum {
    #[iden = "plugin_status"]
    Type,
    #[iden = "installing"]
    Installing,
    #[iden = "active"]
    Active,
    #[iden = "inactive"]
    Inactive,
    #[iden = "error"]
    Error,
    #[iden = "disabled"]
    Disabled,
    #[iden = "uninstalling"]
    Uninstalling,
}
