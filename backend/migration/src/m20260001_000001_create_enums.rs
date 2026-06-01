use sea_orm_migration::prelude::extension::postgres::Type;
use sea_orm_migration::prelude::*;

use crate::enums::{
    notification_kind::NotificationKindEnum, post_policy::PostPolicyEnum,
    reaction_kind::ReactionKindEnum, report_status::ReportStatusEnum,
    thread_status::ThreadStatusEnum, trust_level::TrustLevelEnum, user_role::UserRoleEnum,
    view_policy::ViewPolicyEnum,
};

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000001_create_enums"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_type(
                Type::create()
                    .as_enum(UserRoleEnum::Type)
                    .values([
                        UserRoleEnum::Member,
                        UserRoleEnum::Moderator,
                        UserRoleEnum::Admin,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_type(
                Type::create()
                    .as_enum(TrustLevelEnum::Type)
                    .values([
                        TrustLevelEnum::New,
                        TrustLevelEnum::Basic,
                        TrustLevelEnum::Member,
                        TrustLevelEnum::Regular,
                        TrustLevelEnum::Leader,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_type(
                Type::create()
                    .as_enum(ThreadStatusEnum::Type)
                    .values([
                        ThreadStatusEnum::Open,
                        ThreadStatusEnum::Locked,
                        ThreadStatusEnum::Deleted,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_type(
                Type::create()
                    .as_enum(ReactionKindEnum::Type)
                    .values([
                        ReactionKindEnum::Like,
                        ReactionKindEnum::Helpful,
                        ReactionKindEnum::Insightful,
                        ReactionKindEnum::Funny,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_type(
                Type::create()
                    .as_enum(NotificationKindEnum::Type)
                    .values([
                        NotificationKindEnum::Reply,
                        NotificationKindEnum::Mention,
                        NotificationKindEnum::Reaction,
                        NotificationKindEnum::BestAnswer,
                        NotificationKindEnum::Warn,
                        NotificationKindEnum::System,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_type(
                Type::create()
                    .as_enum(ReportStatusEnum::Type)
                    .values([
                        ReportStatusEnum::Pending,
                        ReportStatusEnum::Resolved,
                        ReportStatusEnum::Dismissed,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_type(
                Type::create()
                    .as_enum(ViewPolicyEnum::Type)
                    .values([
                        ViewPolicyEnum::Public,
                        ViewPolicyEnum::MembersOnly,
                        ViewPolicyEnum::StaffOnly,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_type(
                Type::create()
                    .as_enum(PostPolicyEnum::Type)
                    .values([
                        PostPolicyEnum::Members,
                        PostPolicyEnum::Trusted,
                        PostPolicyEnum::StaffOnly,
                        PostPolicyEnum::Closed,
                    ])
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_type(
                Type::drop()
                    .name(PostPolicyEnum::Type)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_type(
                Type::drop()
                    .name(ViewPolicyEnum::Type)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_type(
                Type::drop()
                    .name(ReportStatusEnum::Type)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_type(
                Type::drop()
                    .name(NotificationKindEnum::Type)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_type(
                Type::drop()
                    .name(ReactionKindEnum::Type)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_type(
                Type::drop()
                    .name(ThreadStatusEnum::Type)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_type(
                Type::drop()
                    .name(TrustLevelEnum::Type)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_type(Type::drop().name(UserRoleEnum::Type).if_exists().to_owned())
            .await?;
        Ok(())
    }
}
