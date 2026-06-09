use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "categories")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    #[sea_orm(unique)]
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub position: i32,
    pub view_policy: ViewPolicy,
    pub post_policy: PostPolicy,
    pub color: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: Option<DateTimeWithTimeZone>,
    pub created_by_id: Option<Uuid>,
    pub updated_by_id: Option<Uuid>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::threads::Entity")]
    Threads,
    #[sea_orm(has_many = "super::user_roles::Entity")]
    UserRoles,
}

impl Related<super::user_roles::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UserRoles.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "view_policy")]
pub enum ViewPolicy {
    #[sea_orm(string_value = "public")]
    Public,
    #[sea_orm(string_value = "members_only")]
    MembersOnly,
    #[sea_orm(string_value = "staff_only")]
    StaffOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "post_policy")]
pub enum PostPolicy {
    #[sea_orm(string_value = "members")]
    Members,
    #[sea_orm(string_value = "trusted")]
    Trusted,
    #[sea_orm(string_value = "staff_only")]
    StaffOnly,
    #[sea_orm(string_value = "closed")]
    Closed,
    #[sea_orm(string_value = "moderated")]
    Moderated,
}
