use sea_orm_migration::prelude::*;

use crate::enums::reaction_kind::ReactionKindEnum;
use crate::m20260001_000002_create_users::Users;
use crate::m20260001_000006_create_posts::Posts;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000008_create_reactions"
    }
}

#[derive(Iden)]
pub enum Reactions {
    Table,
    PostId,
    UserId,
    Kind,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Reactions::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Reactions::PostId).uuid().not_null())
                    .col(ColumnDef::new(Reactions::UserId).uuid().not_null())
                    .col(
                        ColumnDef::new(Reactions::Kind)
                            .custom(ReactionKindEnum::Type)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Reactions::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .primary_key(
                        Index::create()
                            .col(Reactions::PostId)
                            .col(Reactions::UserId)
                            .col(Reactions::Kind),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_reactions_post_id")
                            .from(Reactions::Table, Reactions::PostId)
                            .to(Posts::Table, Posts::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_reactions_user_id")
                            .from(Reactions::Table, Reactions::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_reactions_post")
                    .table(Reactions::Table)
                    .col(Reactions::PostId)
                    .col(Reactions::Kind)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_reactions_user_post")
                    .table(Reactions::Table)
                    .col(Reactions::UserId)
                    .col(Reactions::PostId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Reactions::Table).if_exists().to_owned())
            .await
    }
}
