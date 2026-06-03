use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "permissions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub key: String,
    pub description: String,
    pub group_name: String,
    pub min_trust: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::role_permissions::Entity")]
    RolePermissions,
}

impl Related<super::role_permissions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RolePermissions.def()
    }
}

impl Related<super::roles::Entity> for Entity {
    fn to() -> RelationDef {
        super::role_permissions::Relation::Role.def()
    }
    fn via() -> Option<RelationDef> {
        Some(super::role_permissions::Relation::Permission.def().rev())
    }
}

impl ActiveModelBehavior for ActiveModel {}
