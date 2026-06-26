use sea_orm_migration::prelude::*;

use crate::enums::trust_level::TrustLevelEnum;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000002_create_users"
    }
}

#[derive(Iden)]
pub enum Users {
    Table,
    Id,
    Username,
    Email,
    IsEmailVerified,
    DisplayName,
    PasswordHash,
    TrustLevel,
    TrustScore,
    PostCount,
    DaysVisited,
    Bio,
    Website,
    IsBanned,
    BannedUntil,
    BanReason,
    WarnCount,
    FailedLoginCount,
    LockedUntil,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
    LastSeenAt,
}

#[derive(Iden)]
pub enum UserFollows {
    Table,
    Id,
    FollowerId,
    FollowedId,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Users::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(
                        ColumnDef::new(Users::Username)
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Users::Email).text().not_null().unique_key())
                    .col(
                        ColumnDef::new(Users::IsEmailVerified)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Users::DisplayName).text().null())
                    .col(ColumnDef::new(Users::PasswordHash).text().null())
                    .col(
                        ColumnDef::new(Users::TrustLevel)
                            .custom(TrustLevelEnum::Type)
                            .not_null()
                            .extra("DEFAULT 'new'"),
                    )
                    .col(
                        ColumnDef::new(Users::TrustScore)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Users::PostCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Users::DaysVisited)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(Users::Bio).text().null())
                    .col(ColumnDef::new(Users::Website).text().null())
                    .col(
                        ColumnDef::new(Users::IsBanned)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Users::BannedUntil)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(ColumnDef::new(Users::BanReason).text().null())
                    .col(
                        ColumnDef::new(Users::WarnCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Users::FailedLoginCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Users::LockedUntil)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Users::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(
                        ColumnDef::new(Users::UpdatedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Users::DeletedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Users::LastSeenAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        // Stats/DAU/MAU indexes
        manager
            .create_index(
                Index::create()
                    .name("idx_users_last_seen_at")
                    .table(Users::Table)
                    .col(Users::LastSeenAt)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_users_stats_created")
                    .table(Users::Table)
                    .col(Users::CreatedAt)
                    .to_owned(),
            )
            .await?;

        // User follows
        manager
            .create_table(
                Table::create()
                    .table(UserFollows::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserFollows::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(UserFollows::FollowerId).uuid().not_null())
                    .col(ColumnDef::new(UserFollows::FollowedId).uuid().not_null())
                    .col(
                        ColumnDef::new(UserFollows::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_follows_follower")
                            .from(UserFollows::Table, UserFollows::FollowerId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_follows_followed")
                            .from(UserFollows::Table, UserFollows::FollowedId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .index(
                        Index::create()
                            .name("uq_user_follows")
                            .col(UserFollows::FollowerId)
                            .col(UserFollows::FollowedId)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_user_follows_follower")
                    .table(UserFollows::Table)
                    .col(UserFollows::FollowerId)
                    .col(UserFollows::CreatedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_user_follows_followed")
                    .table(UserFollows::Table)
                    .col(UserFollows::FollowedId)
                    .col(UserFollows::CreatedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(UserFollows::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(Users::Table).if_exists().to_owned())
            .await
    }
}
