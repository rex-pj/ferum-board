use sea_orm_migration::prelude::*;

#[derive(Iden)]
pub enum PluginTierEnum {
    #[iden = "plugin_tier"]
    Type,
    #[iden = "manifest"]
    Manifest,
    #[iden = "script"]
    Script,
    #[iden = "service"]
    Service,
}
