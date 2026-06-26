use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "thread_view_dedup")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub thread_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub viewer_key: String,
    pub viewer_type: String,
    pub last_viewed_date: Date,
    pub total_views: i32,
    pub first_viewed_at: DateTimeWithTimeZone,
    pub last_viewed_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
