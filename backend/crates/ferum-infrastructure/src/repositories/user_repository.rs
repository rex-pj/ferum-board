use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::prelude::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::*;
use uuid::Uuid;

use crate::entities::{user_avatars, user_covers, user_muted_categories, user_preferences, user_watched_categories, users};
use ferum_application::shared::AppError;
use ferum_domain::models::user::{TrustLevel, User, UserPreferences};
use ferum_domain::repositories::user_repository::{NewUser, UpdateUser, UserRepository};

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
        u.trust_level::TEXT AS trust_level,
        u.trust_score, u.post_count, u.days_visited,
        u.bio, u.website, u.is_banned, u.banned_until, u.ban_reason,
        u.warn_count, u.failed_login_count, u.locked_until,
        u.created_at, u.updated_at, u.deleted_at, u.last_seen_at,
        ua.file_key AS avatar_key,
        uc.file_key AS cover_key,
        (SELECT r.slug FROM user_roles ur
             JOIN roles r ON r.id = ur.role_id
             WHERE ur.user_id = u.id AND ur.category_id IS NULL
             ORDER BY r.position ASC LIMIT 1) AS primary_role_slug
    FROM users u
    LEFT JOIN user_avatars ua ON ua.user_id = u.id
    LEFT JOIN user_covers uc ON uc.user_id = u.id
"#;

#[derive(Debug, FromQueryResult)]
struct UserRow {
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

fn row_to_domain(row: UserRow) -> User {
    User {
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
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, AppError> {
        let sql = format!("{USER_SELECT} WHERE u.id = $1");
        let stmt = Statement::from_sql_and_values(DbBackend::Postgres, &sql, [id.into()]);
        Ok(UserRow::find_by_statement(stmt)
            .one(&self.db)
            .await?
            .map(row_to_domain))
    }

    async fn find_many_by_ids(&self, ids: &[Uuid]) -> Result<Vec<User>, AppError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("${i}")).collect();
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

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, AppError> {
        let sql = format!("{USER_SELECT} WHERE u.email = $1");
        let stmt = Statement::from_sql_and_values(DbBackend::Postgres, &sql, [email.into()]);
        Ok(UserRow::find_by_statement(stmt)
            .one(&self.db)
            .await?
            .map(row_to_domain))
    }

    async fn find_by_username(&self, username: &str) -> Result<Option<User>, AppError> {
        let sql = format!("{USER_SELECT} WHERE u.username = $1");
        let stmt = Statement::from_sql_and_values(DbBackend::Postgres, &sql, [username.into()]);
        Ok(UserRow::find_by_statement(stmt)
            .one(&self.db)
            .await?
            .map(row_to_domain))
    }

    async fn create(&self, cmd: NewUser) -> Result<User, AppError> {
        let id = Uuid::new_v4();
        let model = users::ActiveModel {
            id: Set(id),
            username: Set(cmd.username),
            email: Set(cmd.email),
            password_hash: Set(cmd.password_hash),
            ..Default::default()
        };
        model.insert(&self.db).await?;
        self.find_by_id(id).await?.ok_or(AppError::NotFound)
    }

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
        if let Some(v) = patch.display_name {
            active.display_name = Set(v);
        }
        if let Some(v) = patch.bio {
            active.bio = Set(v);
        }
        if let Some(v) = patch.website {
            active.website = Set(v);
        }
        if let Some(v) = patch.is_banned {
            active.is_banned = Set(v);
        }
        if let Some(v) = patch.banned_until {
            active.banned_until = Set(v.map(|t| t.fixed_offset()));
        }
        if let Some(v) = patch.ban_reason {
            active.ban_reason = Set(v);
        }
        if let Some(v) = patch.last_seen_at {
            active.last_seen_at = Set(Some(v.fixed_offset()));
        }

        if active.is_changed() {
            active.update(&self.db).await?;
        }

        self.find_by_id(id).await?.ok_or(AppError::NotFound)
    }

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
            .col_expr(
                users::Column::LockedUntil,
                Expr::value(until.fixed_offset()),
            )
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
        // Count users who have the admin role assigned globally via user_roles table
        #[derive(FromQueryResult)]
        struct CountRow {
            cnt: i64,
        }
        let row = CountRow::find_by_statement(Statement::from_string(
            DbBackend::Postgres,
            r#"
                SELECT COUNT(DISTINCT ur.user_id)::BIGINT AS cnt
                FROM user_roles ur
                JOIN roles r ON r.id = ur.role_id
                WHERE r.slug = 'admin' AND ur.category_id IS NULL
            "#,
        ))
        .one(&self.db)
        .await?
        .ok_or_else(|| AppError::internal("count query failed".to_string()))?;
        Ok(row.cnt as u64)
    }

    async fn list_paginated(
        &self,
        page: u64,
        per_page: u64,
        search: Option<&str>,
    ) -> Result<(Vec<User>, u64), AppError> {
        let offset = page.saturating_sub(1) * per_page;

        let mut count_query = users::Entity::find().order_by_desc(users::Column::CreatedAt);
        if let Some(q) = search.filter(|s| !s.is_empty()) {
            count_query = count_query.filter(
                users::Column::Username
                    .contains(q)
                    .or(users::Column::Email.contains(q))
                    .or(users::Column::DisplayName.contains(q)),
            );
        }
        let total = count_query.count(&self.db).await?;

        let (where_clause, values): (String, Vec<Value>) =
            if let Some(q) = search.filter(|s| !s.is_empty()) {
                let pattern = format!("%{q}%");
                (
                    "WHERE (u.username ILIKE $1 OR u.email ILIKE $1 OR u.display_name ILIKE $1)"
                        .to_string(),
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
        let (theme, font_size, layout, email_notifications) =
            match user_preferences::Entity::find_by_id(user_id)
                .one(&self.db)
                .await?
            {
                Some(m) => (m.theme, m.font_size, m.layout, m.email_notifications),
                None => {
                    let d = UserPreferences::default();
                    (d.theme, d.font_size, d.layout, d.email_notifications)
                }
            };

        let muted_categories = user_muted_categories::Entity::find()
            .filter(user_muted_categories::Column::UserId.eq(user_id))
            .all(&self.db)
            .await?
            .into_iter()
            .map(|m| m.category_id)
            .collect();

        let watched_categories = user_watched_categories::Entity::find()
            .filter(user_watched_categories::Column::UserId.eq(user_id))
            .all(&self.db)
            .await?
            .into_iter()
            .map(|m| m.category_id)
            .collect();

        Ok(UserPreferences {
            user_id,
            theme,
            font_size,
            layout,
            email_notifications,
            muted_categories,
            watched_categories,
        })
    }

    async fn upsert_preferences(&self, prefs: UserPreferences) -> Result<(), AppError> {
        let UserPreferences {
            user_id,
            theme,
            font_size,
            layout,
            email_notifications,
            muted_categories,
            watched_categories,
        } = prefs;

        let txn = self.db.begin().await?;

        user_preferences::Entity::insert(user_preferences::ActiveModel {
            user_id: Set(user_id),
            theme: Set(theme),
            font_size: Set(font_size),
            layout: Set(layout),
            email_notifications: Set(email_notifications),
        })
        .on_conflict(
            OnConflict::column(user_preferences::Column::UserId)
                .update_columns([
                    user_preferences::Column::Theme,
                    user_preferences::Column::FontSize,
                    user_preferences::Column::Layout,
                    user_preferences::Column::EmailNotifications,
                ])
                .to_owned(),
        )
        .exec(&txn)
        .await?;

        user_muted_categories::Entity::delete_many()
            .filter(user_muted_categories::Column::UserId.eq(user_id))
            .exec(&txn)
            .await?;

        if !muted_categories.is_empty() {
            let models: Vec<_> = muted_categories
                .into_iter()
                .map(|category_id| user_muted_categories::ActiveModel {
                    user_id: Set(user_id),
                    category_id: Set(category_id),
                })
                .collect();
            user_muted_categories::Entity::insert_many(models)
                .exec(&txn)
                .await?;
        }

        user_watched_categories::Entity::delete_many()
            .filter(user_watched_categories::Column::UserId.eq(user_id))
            .exec(&txn)
            .await?;

        if !watched_categories.is_empty() {
            let models: Vec<_> = watched_categories
                .into_iter()
                .map(|category_id| user_watched_categories::ActiveModel {
                    user_id: Set(user_id),
                    category_id: Set(category_id),
                })
                .collect();
            user_watched_categories::Entity::insert_many(models)
                .exec(&txn)
                .await?;
        }

        txn.commit().await?;
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

    async fn set_cover(&self, user_id: Uuid, file_key: String) -> Result<(), AppError> {
        let model = user_covers::ActiveModel {
            user_id: Set(user_id),
            file_key: Set(file_key),
            ..Default::default()
        };
        user_covers::Entity::insert(model)
            .on_conflict(
                OnConflict::column(user_covers::Column::UserId)
                    .update_columns([
                        user_covers::Column::FileKey,
                        user_covers::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn remove_cover(&self, user_id: Uuid) -> Result<(), AppError> {
        user_covers::Entity::delete_by_id(user_id)
            .exec(&self.db)
            .await?;
        Ok(())
    }
}
