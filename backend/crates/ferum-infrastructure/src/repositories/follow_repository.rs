use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::prelude::*;
use sea_orm::*;
use uuid::Uuid;

use crate::entities::user_follows;
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

        let count_sql = "SELECT COUNT(*) FROM user_follows WHERE follower_id = $1";
        let count_stmt =
            Statement::from_sql_and_values(DbBackend::Postgres, count_sql, [user_id.into()]);
        let total: u64 = self
            .db
            .query_one(count_stmt)
            .await?
            .map(|r| r.try_get_by_index::<i64>(0).unwrap_or(0) as u64)
            .unwrap_or(0);

        if total == 0 {
            return Ok((vec![], 0));
        }

        let sql = format!(
            r#"SELECT
                uf.id AS follow_id, uf.follower_id, uf.followed_id,
                uf.created_at AS follow_created_at,
                {user_cols}
            FROM user_follows uf
            JOIN users u ON u.id = uf.followed_id
            LEFT JOIN user_avatars ua ON ua.user_id = u.id
            LEFT JOIN user_covers uc ON uc.user_id = u.id
            WHERE uf.follower_id = $1
            ORDER BY uf.created_at DESC
            LIMIT $2 OFFSET $3"#,
            user_cols = USER_SELECT_COLS
        );
        let stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            &sql,
            [user_id.into(), (per_page as i64).into(), (offset as i64).into()],
        );
        let rows = FollowUserRow::find_by_statement(stmt).all(&self.db).await?;
        Ok((rows.into_iter().map(follow_user_row_to_pair).collect(), total))
    }

    async fn list_followers(
        &self,
        user_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<(Follow, User)>, u64), AppError> {
        let offset = page.saturating_sub(1) * per_page;

        let count_sql = "SELECT COUNT(*) FROM user_follows WHERE followed_id = $1";
        let count_stmt =
            Statement::from_sql_and_values(DbBackend::Postgres, count_sql, [user_id.into()]);
        let total: u64 = self
            .db
            .query_one(count_stmt)
            .await?
            .map(|r| r.try_get_by_index::<i64>(0).unwrap_or(0) as u64)
            .unwrap_or(0);

        if total == 0 {
            return Ok((vec![], 0));
        }

        let sql = format!(
            r#"SELECT
                uf.id AS follow_id, uf.follower_id, uf.followed_id,
                uf.created_at AS follow_created_at,
                {user_cols}
            FROM user_follows uf
            JOIN users u ON u.id = uf.follower_id
            LEFT JOIN user_avatars ua ON ua.user_id = u.id
            LEFT JOIN user_covers uc ON uc.user_id = u.id
            WHERE uf.followed_id = $1
            ORDER BY uf.created_at DESC
            LIMIT $2 OFFSET $3"#,
            user_cols = USER_SELECT_COLS
        );
        let stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            &sql,
            [user_id.into(), (per_page as i64).into(), (offset as i64).into()],
        );
        let rows = FollowUserRow::find_by_statement(stmt).all(&self.db).await?;
        Ok((rows.into_iter().map(follow_user_row_to_pair).collect(), total))
    }

    async fn count_following(&self, user_id: Uuid) -> Result<u64, AppError> {
        let sql = "SELECT COUNT(*) FROM user_follows WHERE follower_id = $1";
        let stmt =
            Statement::from_sql_and_values(DbBackend::Postgres, sql, [user_id.into()]);
        Ok(self
            .db
            .query_one(stmt)
            .await?
            .map(|r| r.try_get_by_index::<i64>(0).unwrap_or(0) as u64)
            .unwrap_or(0))
    }

    async fn count_followers(&self, user_id: Uuid) -> Result<u64, AppError> {
        let sql = "SELECT COUNT(*) FROM user_follows WHERE followed_id = $1";
        let stmt =
            Statement::from_sql_and_values(DbBackend::Postgres, sql, [user_id.into()]);
        Ok(self
            .db
            .query_one(stmt)
            .await?
            .map(|r| r.try_get_by_index::<i64>(0).unwrap_or(0) as u64)
            .unwrap_or(0))
    }
}

// Inline columns for the FollowUserRow SELECT (everything after the follow columns).
const USER_SELECT_COLS: &str = r#"u.id, u.username, u.email, u.is_email_verified,
        u.display_name, u.password_hash,
        u.trust_level::TEXT AS trust_level,
        u.trust_score, u.post_count, u.days_visited,
        u.bio, u.website, u.is_banned, u.banned_until, u.ban_reason,
        u.warn_count, u.failed_login_count, u.locked_until,
        u.created_at, u.updated_at, u.deleted_at, u.last_seen_at,
        ua.file_key AS avatar_key,
        uc.file_key AS cover_key,
        (SELECT r.slug FROM user_roles ur2
             JOIN roles r ON r.id = ur2.role_id
             WHERE ur2.user_id = u.id AND ur2.category_id IS NULL
             ORDER BY r.position ASC LIMIT 1) AS primary_role_slug"#;
