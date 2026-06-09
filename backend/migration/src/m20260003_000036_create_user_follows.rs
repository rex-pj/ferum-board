use sea_orm_migration::prelude::*;

use crate::m20260001_000002_create_users::Users;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260003_000036_create_user_follows"
    }
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
            .drop_table(Table::drop().table(UserFollows::Table).if_exists().to_owned())
            .await
    }
}
