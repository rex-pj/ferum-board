use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub thread_id: Uuid,
    pub author_id: Uuid,
    pub parent_id: Option<Uuid>,
    #[sea_orm(column_type = "Text")]
    pub content_md: String,
    #[sea_orm(column_type = "Text")]
    pub content_html: String,
    pub status: PostStatus,
    pub is_deleted: bool,
    pub deleted_at: Option<DateTimeWithTimeZone>,
    pub deleted_by_id: Option<Uuid>,
    pub edited_at: Option<DateTimeWithTimeZone>,
    pub edited_by_id: Option<Uuid>,
    pub edit_count: i32,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::threads::Entity",
        from = "Column::ThreadId",
        to = "super::threads::Column::Id"
    )]
    Thread,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::AuthorId",
        to = "super::users::Column::Id"
    )]
    Author,
    #[sea_orm(has_many = "super::reactions::Entity")]
    Reactions,
}

impl Related<super::threads::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Thread.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Author.def()
    }
}

impl Related<super::reactions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Reactions.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "post_status")]
pub enum PostStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "published")]
    Published,
}
