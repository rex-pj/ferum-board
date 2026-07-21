use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "review_ratings")]
pub struct Model {
    // One structured rating per review thread.
    #[sea_orm(primary_key, auto_increment = false)]
    pub thread_id: Uuid,
    pub overall: i16,
    pub durability: Option<i16>,
    pub materials: Option<i16>,
    pub comfort: Option<i16>,
    pub aesthetics: Option<i16>,
    pub value_for_money: Option<i16>,
    pub verified_purchase: bool,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::threads::Entity",
        from = "Column::ThreadId",
        to = "super::threads::Column::Id"
    )]
    Thread,
}

impl Related<super::threads::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Thread.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
