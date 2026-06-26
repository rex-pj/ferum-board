use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::prelude::*;
use sea_orm::sea_query::{Alias, Expr, JoinType, Order, PostgresQueryBuilder, Query, SimpleExpr, SubQueryStatement};
use sea_orm::*;
use uuid::Uuid;

use crate::entities::{roles, user_avatars, user_covers, user_follows, user_roles, users};
use ferum_application::shared::AppError;
use ferum_domain::models::follow::Follow;
use ferum_domain::models::user::{TrustLevel, User};
use ferum_domain::repositories::follow_repository::FollowRepository;

pub struct PgFollowRepository {
    db: DatabaseConnection,
}

impl PgFollowRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn follow_to_domain(m: user_follows::Model) -> Follow {
    Follow {
        id: m.id,
        follower_id: m.follower_id,
        followed_id: m.followed_id,
        created_at: m.created_at.with_timezone(&Utc),
    }
}

// ─── FollowUserRow combines follow + user in a single query ──────────────────

#[derive(Debug, FromQueryResult)]
struct FollowUserRow {
    follow_id: Uuid,
    follower_id: Uuid,
    followed_id: Uuid,
    follow_created_at: DateTime<FixedOffset>,
    id: Uuid,
    username: String,
    email: String,
    is_email_verified: bool,
    display_name: Option<String>,
    password_hash: Option<String>,
    trust_level: String,
    trust_score: i32,
    post_count: i32,
    days_visited: i32,
    bio: Option<String>,
    website: Option<String>,
    is_banned: bool,
    banned_until: Option<DateTime<FixedOffset>>,
    ban_reason: Option<String>,
    warn_count: i32,
    failed_login_count: i32,
    locked_until: Option<DateTime<FixedOffset>>,
    created_at: DateTime<FixedOffset>,
    updated_at: Option<DateTime<FixedOffset>>,
    deleted_at: Option<DateTime<FixedOffset>>,
    last_seen_at: Option<DateTime<FixedOffset>>,
    avatar_key: Option<String>,
    cover_key: Option<String>,
    primary_role_slug: Option<String>,
}

fn follow_user_row_to_pair(row: FollowUserRow) -> (Follow, User) {
    let follow = Follow {
        id: row.follow_id,
        follower_id: row.follower_id,
        followed_id: row.followed_id,
        created_at: row.follow_created_at.with_timezone(&Utc),
    };
    let user = User {
        id: row.id,
        username: row.username,
        email: row.email,
        is_email_verified: row.is_email_verified,
        display_name: row.display_name,
        password_hash: row.password_hash,
        trust_level: match row.trust_level.as_str() {
            "basic" => TrustLevel::Basic,
            "member" => TrustLevel::Member,
            "regular" => TrustLevel::Regular,
            "leader" => TrustLevel::Leader,
            _ => TrustLevel::New,
        },
        primary_role_slug: row.primary_role_slug,
        trust_score: row.trust_score,
        post_count: row.post_count,
        days_visited: row.days_visited,
        avatar_url: row.avatar_key.map(|k| format!("/files/{k}")),
        cover_url: row.cover_key.map(|k| format!("/files/{k}")),
        bio: row.bio,
        website: row.website,
        is_banned: row.is_banned,
        banned_until: row.banned_until.map(|t| t.with_timezone(&Utc)),
        ban_reason: row.ban_reason,
        warn_count: row.warn_count,
        failed_login_count: row.failed_login_count,
        locked_until: row.locked_until.map(|t| t.with_timezone(&Utc)),
        created_at: row.created_at.with_timezone(&Utc),
        updated_at: row.updated_at.map(|t| t.with_timezone(&Utc)),
        deleted_at: row.deleted_at.map(|t| t.with_timezone(&Utc)),
        last_seen_at: row.last_seen_at.map(|t| t.with_timezone(&Utc)),
    };
    (follow, user)
}

#[async_trait]
impl FollowRepository for PgFollowRepository {
    async fn find(
        &self,
        follower_id: Uuid,
        followed_id: Uuid,
    ) -> Result<Option<Follow>, AppError> {
        Ok(user_follows::Entity::find()
            .filter(user_follows::Column::FollowerId.eq(follower_id))
            .filter(user_follows::Column::FollowedId.eq(followed_id))
            .one(&self.db)
            .await?
            .map(follow_to_domain))
    }

    async fn add(&self, follower_id: Uuid, followed_id: Uuid) -> Result<Follow, AppError> {
        let model = user_follows::ActiveModel {
            id: Set(Uuid::new_v4()),
            follower_id: Set(follower_id),
            followed_id: Set(followed_id),
            ..Default::default()
        };
        Ok(follow_to_domain(model.insert(&self.db).await?))
    }

    async fn remove(&self, follower_id: Uuid, followed_id: Uuid) -> Result<(), AppError> {
        user_follows::Entity::delete_many()
            .filter(user_follows::Column::FollowerId.eq(follower_id))
            .filter(user_follows::Column::FollowedId.eq(followed_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn list_following(
        &self,
        user_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<(Follow, User)>, u64), AppError> {
        let offset = page.saturating_sub(1) * per_page;
        let (sql, values) = follow_list_query(
            user_follows::Column::FollowedId,
            user_follows::Column::FollowerId,
            user_id,
            per_page,
            offset,
        );
        let data_stmt = Statement::from_sql_and_values(DbBackend::Postgres, sql, values);
        let (total, rows) = tokio::try_join!(
            user_follows::Entity::find()
                .filter(user_follows::Column::FollowerId.eq(user_id))
                .count(&self.db),
            FollowUserRow::find_by_statement(data_stmt).all(&self.db),
        )?;
        Ok((rows.into_iter().map(follow_user_row_to_pair).collect(), total))
    }

    async fn list_followers(
        &self,
        user_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<(Follow, User)>, u64), AppError> {
        let offset = page.saturating_sub(1) * per_page;
        let (sql, values) = follow_list_query(
            user_follows::Column::FollowerId,
            user_follows::Column::FollowedId,
            user_id,
            per_page,
            offset,
        );
        let data_stmt = Statement::from_sql_and_values(DbBackend::Postgres, sql, values);
        let (total, rows) = tokio::try_join!(
            user_follows::Entity::find()
                .filter(user_follows::Column::FollowedId.eq(user_id))
                .count(&self.db),
            FollowUserRow::find_by_statement(data_stmt).all(&self.db),
        )?;
        Ok((rows.into_iter().map(follow_user_row_to_pair).collect(), total))
    }

    async fn count_following(&self, user_id: Uuid) -> Result<u64, AppError> {
        Ok(user_follows::Entity::find()
            .filter(user_follows::Column::FollowerId.eq(user_id))
            .count(&self.db)
            .await?)
    }

    async fn count_followers(&self, user_id: Uuid) -> Result<u64, AppError> {
        Ok(user_follows::Entity::find()
            .filter(user_follows::Column::FollowedId.eq(user_id))
            .count(&self.db)
            .await?)
    }
}

/// Builds the paginated follow list query. `user_join_col` is the `user_follows` column
/// that points to the user being listed (followed_id for following, follower_id for followers).
/// `filter_col` is the column used in the WHERE clause (follower_id or followed_id).
fn follow_list_query(
    user_join_col: user_follows::Column,
    filter_col: user_follows::Column,
    user_id: Uuid,
    per_page: u64,
    offset: u64,
) -> (String, sea_orm::sea_query::Values) {
    let primary_role_subq = Query::select()
        .column((roles::Entity, roles::Column::Slug))
        .from(user_roles::Entity)
        .inner_join(
            roles::Entity,
            Expr::col((roles::Entity, roles::Column::Id))
                .equals((user_roles::Entity, user_roles::Column::RoleId)),
        )
        .and_where(
            Expr::col((user_roles::Entity, user_roles::Column::UserId))
                .equals((users::Entity, users::Column::Id)),
        )
        .and_where(
            Expr::col((user_roles::Entity, user_roles::Column::CategoryId)).is_null(),
        )
        .order_by((roles::Entity, roles::Column::Position), Order::Asc)
        .limit(1)
        .to_owned();

    let primary_role_expr = SimpleExpr::SubQuery(
        None,
        Box::new(SubQueryStatement::SelectStatement(primary_role_subq)),
    );

    Query::select()
        .expr_as(
            Expr::col((user_follows::Entity, user_follows::Column::Id)),
            Alias::new("follow_id"),
        )
        .column((user_follows::Entity, user_follows::Column::FollowerId))
        .column((user_follows::Entity, user_follows::Column::FollowedId))
        .expr_as(
            Expr::col((user_follows::Entity, user_follows::Column::CreatedAt)),
            Alias::new("follow_created_at"),
        )
        .column((users::Entity, users::Column::Id))
        .column((users::Entity, users::Column::Username))
        .column((users::Entity, users::Column::Email))
        .column((users::Entity, users::Column::IsEmailVerified))
        .column((users::Entity, users::Column::DisplayName))
        .column((users::Entity, users::Column::PasswordHash))
        .expr_as(
            Expr::col((users::Entity, users::Column::TrustLevel))
                .cast_as(Alias::new("text")),
            Alias::new("trust_level"),
        )
        .column((users::Entity, users::Column::TrustScore))
        .column((users::Entity, users::Column::PostCount))
        .column((users::Entity, users::Column::DaysVisited))
        .column((users::Entity, users::Column::Bio))
        .column((users::Entity, users::Column::Website))
        .column((users::Entity, users::Column::IsBanned))
        .column((users::Entity, users::Column::BannedUntil))
        .column((users::Entity, users::Column::BanReason))
        .column((users::Entity, users::Column::WarnCount))
        .column((users::Entity, users::Column::FailedLoginCount))
        .column((users::Entity, users::Column::LockedUntil))
        .column((users::Entity, users::Column::CreatedAt))
        .column((users::Entity, users::Column::UpdatedAt))
        .column((users::Entity, users::Column::DeletedAt))
        .column((users::Entity, users::Column::LastSeenAt))
        .expr_as(
            Expr::col((user_avatars::Entity, user_avatars::Column::FileKey)),
            Alias::new("avatar_key"),
        )
        .expr_as(
            Expr::col((user_covers::Entity, user_covers::Column::FileKey)),
            Alias::new("cover_key"),
        )
        .expr_as(primary_role_expr, Alias::new("primary_role_slug"))
        .from(user_follows::Entity)
        .inner_join(
            users::Entity,
            Expr::col((users::Entity, users::Column::Id))
                .equals((user_follows::Entity, user_join_col)),
        )
        .join(
            JoinType::LeftJoin,
            user_avatars::Entity,
            Expr::col((user_avatars::Entity, user_avatars::Column::UserId))
                .equals((users::Entity, users::Column::Id)),
        )
        .join(
            JoinType::LeftJoin,
            user_covers::Entity,
            Expr::col((user_covers::Entity, user_covers::Column::UserId))
                .equals((users::Entity, users::Column::Id)),
        )
        .and_where(Expr::col((user_follows::Entity, filter_col)).eq(user_id))
        .order_by(
            (user_follows::Entity, user_follows::Column::CreatedAt),
            Order::Desc,
        )
        .limit(per_page)
        .offset(offset)
        .build(PostgresQueryBuilder)
}
