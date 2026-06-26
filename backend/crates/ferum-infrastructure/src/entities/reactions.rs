use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "reactions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub post_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub kind: ReactionKind,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::posts::Entity",
        from = "Column::PostId",
        to = "super::posts::Column::Id"
    )]
    Post,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id"
    )]
    User,
}

impl Related<super::posts::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Post.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize, Hash)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "reaction_kind")]
pub enum ReactionKind {
    #[sea_orm(string_value = "like")]
    Like,
    #[sea_orm(string_value = "helpful")]
    Helpful,
    #[sea_orm(string_value = "insightful")]
    Insightful,
    #[sea_orm(string_value = "funny")]
    Funny,
}
