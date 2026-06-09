use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "user_follows")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub follower_id: Uuid,
    pub followed_id: Uuid,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::FollowerId",
        to = "super::users::Column::Id"
    )]
    Follower,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::FollowedId",
        to = "super::users::Column::Id"
    )]
    Followed,
}

impl ActiveModelBehavior for ActiveModel {}
