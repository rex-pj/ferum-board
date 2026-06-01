use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::prelude::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::*;
use uuid::Uuid;

use crate::application::shared::AppError;
use crate::domain::models::user::{TrustLevel, User, UserPreferences, UserRole};
use crate::domain::repositories::user_repository::{NewUser, UpdateUser, UserRepository};
use crate::entities::{user_avatars, user_preferences, users};

pub struct PgUserRepository {
    db: DatabaseConnection,
}

impl PgUserRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

// ─── Single JOIN query ────────────────────────────────────────────────────────

const USER_SELECT: &str = r#"
    SELECT
        u.id, u.username, u.email, u.is_email_verified,
        u.display_name, u.password_hash,
        u.role::TEXT AS role, u.trust_level::TEXT AS trust_level,
        u.is_global_mod, u.trust_score, u.post_count, u.days_visited,
        u.bio, u.website, u.is_banned, u.banned_until, u.ban_reason,
        u.warn_count, u.failed_login_count, u.locked_until,
        u.created_at, u.updated_at, u.deleted_at, u.last_seen_at,
        ua.file_key AS avatar_key
    FROM users u
    LEFT JOIN user_avatars ua ON ua.user_id = u.id
"#;

#[derive(Debug, FromQueryResult)]
struct UserRow {
    id: Uuid,
    username: String,
    email: String,
    is_email_verified: bool,
    display_name: Option<String>,
    password_hash: Option<String>,
    role: String,
    trust_level: String,
    is_global_mod: bool,
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
}

fn row_to_domain(row: UserRow) -> User {
    User {
        id: row.id,
        username: row.username,
        email: row.email,
        is_email_verified: row.is_email_verified,
        display_name: row.display_name,
        password_hash: row.password_hash,
        role: match row.role.as_str() {
            "moderator" => UserRole::Moderator,
            "admin" => UserRole::Admin,
            _ => UserRole::Member,
        },
        trust_level: match row.trust_level.as_str() {
            "basic" => TrustLevel::Basic,
            "member" => TrustLevel::Member,
            "regular" => TrustLevel::Regular,
            "leader" => TrustLevel::Leader,
            _ => TrustLevel::New,
        },
        is_global_mod: row.is_global_mod,
        trust_score: row.trust_score,
        post_count: row.post_count,
        days_visited: row.days_visited,
        avatar_url: row.avatar_key.map(|k| format!("/files/{k}")),
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
    }
}

// Used only for create() which has no avatar yet.
fn entity_to_domain(m: users::Model, avatar_key: Option<String>) -> User {
    User {
        id: m.id,
        username: m.username,
        email: m.email,
        is_email_verified: m.is_email_verified,
        display_name: m.display_name,
        password_hash: m.password_hash,
        role: match m.role {
            users::UserRole::Member => UserRole::Member,
            users::UserRole::Moderator => UserRole::Moderator,
            users::UserRole::Admin => UserRole::Admin,
        },
        trust_level: match m.trust_level {
            users::TrustLevel::New => TrustLevel::New,
            users::TrustLevel::Basic => TrustLevel::Basic,
            users::TrustLevel::Member => TrustLevel::Member,
            users::TrustLevel::Regular => TrustLevel::Regular,
            users::TrustLevel::Leader => TrustLevel::Leader,
        },
        is_global_mod: m.is_global_mod,
        trust_score: m.trust_score,
        post_count: m.post_count,
        days_visited: m.days_visited,
        avatar_url: avatar_key.map(|k| format!("/files/{k}")),
        bio: m.bio,
        website: m.website,
        is_banned: m.is_banned,
        banned_until: m.banned_until.map(|t| t.with_timezone(&Utc)),
        ban_reason: m.ban_reason,
        warn_count: m.warn_count,
        failed_login_count: m.failed_login_count,
        locked_until: m.locked_until.map(|t| t.with_timezone(&Utc)),
        created_at: m.created_at.with_timezone(&Utc),
        updated_at: m.updated_at.map(|t| t.with_timezone(&Utc)),
        deleted_at: m.deleted_at.map(|t| t.with_timezone(&Utc)),
        last_seen_at: m.last_seen_at.map(|t| t.with_timezone(&Utc)),
    }
}

fn domain_role_to_entity(role: &UserRole) -> users::UserRole {
    match role {
        UserRole::Member => users::UserRole::Member,
        UserRole::Moderator => users::UserRole::Moderator,
        UserRole::Admin => users::UserRole::Admin,
    }
}

fn domain_trust_to_entity(level: &TrustLevel) -> users::TrustLevel {
    match level {
        TrustLevel::New => users::TrustLevel::New,
        TrustLevel::Basic => users::TrustLevel::Basic,
        TrustLevel::Member => users::TrustLevel::Member,
        TrustLevel::Regular => users::TrustLevel::Regular,
        TrustLevel::Leader => users::TrustLevel::Leader,
    }
}

#[async_trait]
impl UserRepository for PgUserRepository {
    // Single JOIN query: 1 roundtrip (was 2).
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, AppError> {
        let sql = format!("{USER_SELECT} WHERE u.id = $1");
        let stmt = Statement::from_sql_and_values(DbBackend::Postgres, &sql, [id.into()]);
        Ok(UserRow::find_by_statement(stmt).one(&self.db).await?.map(row_to_domain))
    }

    // Single JOIN query: 1 roundtrip (was 2).
    async fn find_many_by_ids(&self, ids: &[Uuid]) -> Result<Vec<User>, AppError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders: Vec<String> =
            (1..=ids.len()).map(|i| format!("${i}")).collect();
        let in_clause = placeholders.join(", ");
        let sql = format!("{USER_SELECT} WHERE u.id IN ({in_clause})");
        let values: Vec<Value> = ids.iter().map(|id| (*id).into()).collect();
        let stmt = Statement::from_sql_and_values(DbBackend::Postgres, &sql, values);
        Ok(UserRow::find_by_statement(stmt)
            .all(&self.db)
            .await?
            .into_iter()
            .map(row_to_domain)
            .collect())
    }

    // Single JOIN query: 1 roundtrip (was 2).
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, AppError> {
        let sql = format!("{USER_SELECT} WHERE u.email = $1");
        let stmt =
            Statement::from_sql_and_values(DbBackend::Postgres, &sql, [email.into()]);
        Ok(UserRow::find_by_statement(stmt).one(&self.db).await?.map(row_to_domain))
    }

    // Single JOIN query: 1 roundtrip (was 2).
    async fn find_by_username(&self, username: &str) -> Result<Option<User>, AppError> {
        let sql = format!("{USER_SELECT} WHERE u.username = $1");
        let stmt =
            Statement::from_sql_and_values(DbBackend::Postgres, &sql, [username.into()]);
        Ok(UserRow::find_by_statement(stmt).one(&self.db).await?.map(row_to_domain))
    }

    async fn create(&self, cmd: NewUser) -> Result<User, AppError> {
        let model = users::ActiveModel {
            id: Set(Uuid::new_v4()),
            username: Set(cmd.username),
            email: Set(cmd.email),
            role: Set(domain_role_to_entity(&cmd.role)),
            password_hash: Set(cmd.password_hash),
            ..Default::default()
        };
        let inserted = model.insert(&self.db).await?;
        Ok(entity_to_domain(inserted, None))
    }

    // Direct update without pre-read: 2 roundtrips (was 3).
    // warn_count_delta uses a separate update_many to avoid reading current value.
    async fn update(&self, id: Uuid, patch: UpdateUser) -> Result<User, AppError> {
        if let Some(delta) = patch.warn_count_delta {
            users::Entity::update_many()
                .col_expr(
                    users::Column::WarnCount,
                    Expr::col(users::Column::WarnCount).add(delta),
                )
                .filter(users::Column::Id.eq(id))
                .exec(&self.db)
                .await?;
        }

        let mut active = users::ActiveModel {
            id: Set(id),
            ..Default::default()
        };
        if let Some(v) = patch.display_name { active.display_name = Set(v); }
        if let Some(v) = patch.bio { active.bio = Set(v); }
        if let Some(v) = patch.website { active.website = Set(v); }
        if let Some(v) = patch.is_banned { active.is_banned = Set(v); }
        if let Some(v) = patch.banned_until {
            active.banned_until = Set(v.map(|t| t.fixed_offset()));
        }
        if let Some(v) = patch.ban_reason { active.ban_reason = Set(v); }
        if let Some(v) = patch.role { active.role = Set(domain_role_to_entity(&v)); }
        if let Some(v) = patch.is_global_mod { active.is_global_mod = Set(v); }
        if let Some(v) = patch.last_seen_at {
            active.last_seen_at = Set(Some(v.fixed_offset()));
        }

        if active.is_changed() {
            active.update(&self.db).await?;
        }

        // JOIN fetch replaces the old entity_to_domain + fetch_avatar_key roundtrip.
        self.find_by_id(id).await?.ok_or(AppError::NotFound)
    }

    // UPDATE...RETURNING: 1 roundtrip (was 2).
    async fn increment_failed_login(&self, id: Uuid) -> Result<i32, AppError> {
        #[derive(Debug, FromQueryResult)]
        struct FailedCount {
            failed_login_count: i32,
        }

        let stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE users \
             SET failed_login_count = failed_login_count + 1 \
             WHERE id = $1 \
             RETURNING failed_login_count",
            [id.into()],
        );
        let row = FailedCount::find_by_statement(stmt)
            .one(&self.db)
            .await?
            .ok_or(AppError::NotFound)?;
        Ok(row.failed_login_count)
    }

    async fn reset_failed_login(&self, id: Uuid) -> Result<(), AppError> {
        users::Entity::update_many()
            .col_expr(users::Column::FailedLoginCount, Expr::value(0))
            .col_expr(
                users::Column::LockedUntil,
                Expr::value(sea_orm::Value::ChronoDateTimeWithTimeZone(None)),
            )
            .filter(users::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn lock_until(&self, id: Uuid, until: DateTime<Utc>) -> Result<(), AppError> {
        users::Entity::update_many()
            .col_expr(users::Column::LockedUntil, Expr::value(until.fixed_offset()))
            .filter(users::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn set_email_verified(&self, id: Uuid) -> Result<(), AppError> {
        users::Entity::update_many()
            .col_expr(users::Column::IsEmailVerified, Expr::value(true))
            .filter(users::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn set_trust_level(&self, id: Uuid, level: TrustLevel) -> Result<(), AppError> {
        users::ActiveModel {
            id: ActiveValue::Set(id),
            trust_level: ActiveValue::Set(domain_trust_to_entity(&level)),
            ..Default::default()
        }
        .update(&self.db)
        .await?;
        Ok(())
    }

    async fn count_admins(&self) -> Result<u64, AppError> {
        Ok(users::Entity::find()
            .filter(users::Column::Role.eq(users::UserRole::Admin))
            .count(&self.db)
            .await?)
    }

    // Single JOIN query for data (was users SELECT + avatar batch): 2 roundtrips (COUNT + data).
    async fn list_paginated(
        &self,
        page: u64,
        per_page: u64,
        search: Option<&str>,
    ) -> Result<(Vec<User>, u64), AppError> {
        let offset = page.saturating_sub(1) * per_page;

        // COUNT query (still needed for pagination metadata).
        let mut count_query =
            users::Entity::find().order_by_desc(users::Column::CreatedAt);
        if let Some(q) = search.filter(|s| !s.is_empty()) {
            count_query = count_query.filter(
                users::Column::Username
                    .contains(q)
                    .or(users::Column::Email.contains(q))
                    .or(users::Column::DisplayName.contains(q)),
            );
        }
        let total = count_query.count(&self.db).await?;

        // Data query via JOIN — no separate avatar batch needed.
        let (where_clause, values): (String, Vec<Value>) =
            if let Some(q) = search.filter(|s| !s.is_empty()) {
                let pattern = format!("%{q}%");
                (
                    "WHERE (u.username ILIKE $1 OR u.email ILIKE $1 OR u.display_name ILIKE $1)".to_string(),
                    vec![pattern.into()],
                )
            } else {
                (String::new(), vec![])
            };

        let limit_pos = values.len() + 1;
        let offset_pos = values.len() + 2;
        let sql = format!(
            "{USER_SELECT} {where_clause} \
             ORDER BY u.created_at DESC \
             LIMIT ${limit_pos} OFFSET ${offset_pos}"
        );
        let mut all_values = values;
        all_values.push((per_page as i64).into());
        all_values.push((offset as i64).into());

        let stmt = Statement::from_sql_and_values(DbBackend::Postgres, &sql, all_values);
        let rows = UserRow::find_by_statement(stmt).all(&self.db).await?;
        Ok((rows.into_iter().map(row_to_domain).collect(), total))
    }

    async fn set_password_hash(&self, id: Uuid, hash: String) -> Result<(), AppError> {
        users::Entity::update_many()
            .col_expr(users::Column::PasswordHash, Expr::value(hash))
            .filter(users::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn get_preferences(&self, user_id: Uuid) -> Result<UserPreferences, AppError> {
        match user_preferences::Entity::find_by_id(user_id)
            .one(&self.db)
            .await?
        {
            Some(m) => Ok(UserPreferences {
                user_id: m.user_id,
                theme: m.theme,
                font_size: m.font_size,
                layout: m.layout,
                email_notifications: m.email_notifications,
                muted_categories: m.muted_categories,
                watched_categories: m.watched_categories,
            }),
            None => Ok(UserPreferences { user_id, ..Default::default() }),
        }
    }

    async fn upsert_preferences(&self, prefs: UserPreferences) -> Result<(), AppError> {
        let model = user_preferences::ActiveModel {
            user_id: Set(prefs.user_id),
            theme: Set(prefs.theme),
            font_size: Set(prefs.font_size),
            layout: Set(prefs.layout),
            email_notifications: Set(prefs.email_notifications),
            muted_categories: Set(prefs.muted_categories),
            watched_categories: Set(prefs.watched_categories),
        };
        user_preferences::Entity::insert(model)
            .on_conflict(
                OnConflict::column(user_preferences::Column::UserId)
                    .update_columns([
                        user_preferences::Column::Theme,
                        user_preferences::Column::FontSize,
                        user_preferences::Column::Layout,
                        user_preferences::Column::EmailNotifications,
                        user_preferences::Column::MutedCategories,
                        user_preferences::Column::WatchedCategories,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn set_avatar(&self, user_id: Uuid, file_key: String) -> Result<(), AppError> {
        let model = user_avatars::ActiveModel {
            user_id: Set(user_id),
            file_key: Set(file_key),
            ..Default::default()
        };
        user_avatars::Entity::insert(model)
            .on_conflict(
                OnConflict::column(user_avatars::Column::UserId)
                    .update_columns([
                        user_avatars::Column::FileKey,
                        user_avatars::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn remove_avatar(&self, user_id: Uuid) -> Result<(), AppError> {
        user_avatars::Entity::delete_by_id(user_id)
            .exec(&self.db)
            .await?;
        Ok(())
    }
}
