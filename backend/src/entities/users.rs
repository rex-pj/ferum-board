use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub username: String,
    #[sea_orm(unique)]
    pub email: String,
    pub is_email_verified: bool,
    pub display_name: Option<String>,
    pub password_hash: Option<String>,
    pub role: UserRole,
    pub trust_level: TrustLevel,
    pub is_global_mod: bool,
    pub trust_score: i32,
    pub post_count: i32,
    pub days_visited: i32,
    pub bio: Option<String>,
    pub website: Option<String>,
    pub is_banned: bool,
    pub banned_until: Option<DateTimeWithTimeZone>,
    pub ban_reason: Option<String>,
    pub warn_count: i32,
    pub failed_login_count: i32,
    pub locked_until: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: Option<DateTimeWithTimeZone>,
    pub deleted_at: Option<DateTimeWithTimeZone>,
    pub last_seen_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_one = "super::user_preferences::Entity")]
    UserPreferences,
    #[sea_orm(has_many = "super::threads::Entity")]
    Threads,
    #[sea_orm(has_many = "super::posts::Entity")]
    Posts,
    #[sea_orm(has_many = "super::reactions::Entity")]
    Reactions,
    #[sea_orm(has_many = "super::notifications::Entity")]
    Notifications,
    #[sea_orm(has_many = "super::category_moderators::Entity")]
    CategoryModerators,
    #[sea_orm(has_one = "super::user_avatars::Entity")]
    UserAvatar,
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(
    Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize, Hash,
)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "user_role")]
pub enum UserRole {
    #[sea_orm(string_value = "member")]
    Member,
    #[sea_orm(string_value = "moderator")]
    Moderator,
    #[sea_orm(string_value = "admin")]
    Admin,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, EnumIter, DeriveActiveEnum, Serialize, Deserialize, Hash,
)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "trust_level")]
pub enum TrustLevel {
    #[sea_orm(string_value = "new")]
    New,
    #[sea_orm(string_value = "basic")]
    Basic,
    #[sea_orm(string_value = "member")]
    Member,
    #[sea_orm(string_value = "regular")]
    Regular,
    #[sea_orm(string_value = "leader")]
    Leader,
}
