use sea_orm::ConnectionTrait;
use sea_orm::Statement;
use sea_orm_migration::prelude::extension::postgres::Type;
use sea_orm_migration::prelude::*;

use crate::enums::{
    notification_kind::NotificationKindEnum, post_policy::PostPolicyEnum,
    post_status::PostStatusEnum, reaction_kind::ReactionKindEnum, report_status::ReportStatusEnum,
    thread_status::ThreadStatusEnum, trust_level::TrustLevelEnum, view_policy::ViewPolicyEnum,
};

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000001_bootstrap_enums_and_functions"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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
                        PostPolicyEnum::Moderated,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_type(
                Type::create()
                    .as_enum(PostStatusEnum::Type)
                    .values([PostStatusEnum::Pending, PostStatusEnum::Published])
                    .to_owned(),
            )
            .await?;

        self.create_unaccent_function(manager).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_type(
                Type::drop()
                    .name(PostStatusEnum::Type)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
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
            .get_connection()
            .execute_unprepared("DROP FUNCTION IF EXISTS f_unaccent(text)")
            .await?;
        Ok(())
    }
}

impl Migration {
    /// `f_unaccent` — accent folding for every full-text surface, so "ghe an"
    /// matches "ghế ăn" on a largely Vietnamese forum.
    ///
    /// **One function for both sides**: unaccenting only the query or only the
    /// index silently returns nothing. A wrapper because `unaccent()` is merely
    /// STABLE while an expression index demands IMMUTABLE; pinning the
    /// dictionary argument makes that honest.
    ///
    /// Degrades to the identity function where the extension is unavailable.
    /// Defined in the first migration because every later index needs it.
    async fn create_unaccent_function(&self, manager: &SchemaManager<'_>) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // Probed rather than attempted: a failed CREATE EXTENSION aborts the
        // surrounding migration transaction, taking the rest of this file with
        // it. Ask first, then only run what will succeed.
        let unaccent_available = conn
            .query_one_raw(Statement::from_string(
                manager.get_database_backend(),
                "SELECT count(*)::bigint AS n FROM pg_available_extensions WHERE name = 'unaccent'"
                    .to_owned(),
            ))
            .await?
            .and_then(|row| row.try_get::<i64>("", "n").ok())
            .unwrap_or(0)
            > 0;

        if unaccent_available {
            conn.execute_unprepared("CREATE EXTENSION IF NOT EXISTS unaccent")
                .await?;
            conn.execute_unprepared(
                "CREATE OR REPLACE FUNCTION f_unaccent(text) RETURNS text AS \
                 $$ SELECT public.unaccent('public.unaccent', $1) $$ \
                 LANGUAGE sql IMMUTABLE PARALLEL SAFE STRICT",
            )
            .await?;
        } else {
            conn.execute_unprepared(
                "CREATE OR REPLACE FUNCTION f_unaccent(text) RETURNS text AS \
                 $$ SELECT $1 $$ \
                 LANGUAGE sql IMMUTABLE PARALLEL SAFE STRICT",
            )
            .await?;
        }

        Ok(())
    }
}
