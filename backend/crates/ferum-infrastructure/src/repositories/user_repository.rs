use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::prelude::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::*;
use uuid::Uuid;

use sea_orm::sea_query::extension::postgres::PgExpr;
use sea_orm::sea_query::{Alias, CaseStatement, Expr, Func, Order, PostgresQueryBuilder, Query, SimpleExpr, SubQueryStatement};

use crate::entities::{roles, sea_orm_active_enums, user_avatars, user_covers, user_muted_categories, user_preferences, user_roles, user_watched_categories, users};
use ferum_application::shared::AppError;
use ferum_domain::models::user::{TrustLevel, User, UserPreferences};
use ferum_domain::Locale;
use ferum_domain::repositories::user_repository::{NewUser, UpdateUser, UserRepository};

use crate::observability::slow_query_threshold_ms;

fn primary_role_subexpr() -> SimpleExpr {
    let subq = Query::select()
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
        .and_where(Expr::col((user_roles::Entity, user_roles::Column::CategoryId)).is_null())
        .order_by((roles::Entity, roles::Column::Position), Order::Asc)
        .limit(1)
        .to_owned();
    SimpleExpr::SubQuery(None, Box::new(SubQueryStatement::SelectStatement(subq)))
}

pub struct PgUserRepository {
    db: DatabaseConnection,
}

impl PgUserRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

// ─── Base select helper ───────────────────────────────────────────────────────
//
// Builds a `Select<users::Entity>` with left-joins for avatar/cover and the
// correlated sub-select for primary_role_slug.  Call `.filter()` / `.order_by()`
// / `.limit()` / `.offset()` on the returned value, then `.into_model::<UserRow>()`.

fn user_select() -> sea_orm::Select<users::Entity> {
    use sea_orm::JoinType;
    users::Entity::find()
        .select_only()
        .columns([
            users::Column::Id,
            users::Column::Username,
            users::Column::Email,
            users::Column::IsEmailVerified,
            users::Column::DisplayName,
            users::Column::PasswordHash,
            // TrustLevel is added below as a ::text cast
            users::Column::TrustScore,
            users::Column::PostCount,
            users::Column::DaysVisited,
            users::Column::Bio,
            users::Column::Website,
            users::Column::IsBanned,
            users::Column::BannedUntil,
            users::Column::BanReason,
            users::Column::WarnCount,
            users::Column::FailedLoginCount,
            users::Column::LockedUntil,
            users::Column::CreatedAt,
            users::Column::UpdatedAt,
            users::Column::DeletedAt,
            users::Column::LastSeenAt,
        ])
        .column_as(
            Expr::col((users::Entity, users::Column::TrustLevel))
                .cast_as(Alias::new("text")),
            "trust_level",
        )
        .column_as(user_avatars::Column::FileKey, "avatar_key")
        .column_as(user_covers::Column::FileKey, "cover_key")
        .column_as(primary_role_subexpr(), "primary_role_slug")
        .join(JoinType::LeftJoin, users::Relation::UserAvatars.def())
        .join(JoinType::LeftJoin, users::Relation::UserCovers.def())
}

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

pub(crate) fn domain_trust_to_entity(level: &TrustLevel) -> sea_orm_active_enums::TrustLevel {
    match level {
        TrustLevel::New => sea_orm_active_enums::TrustLevel::New,
        TrustLevel::Basic => sea_orm_active_enums::TrustLevel::Basic,
        TrustLevel::Member => sea_orm_active_enums::TrustLevel::Member,
        TrustLevel::Regular => sea_orm_active_enums::TrustLevel::Regular,
        TrustLevel::Leader => sea_orm_active_enums::TrustLevel::Leader,
    }
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, AppError> {
        Ok(user_select()
            .filter(users::Column::Id.eq(id))
            .into_model::<UserRow>()
            .one(&self.db)
            .await?
            .map(row_to_domain))
    }

    async fn find_many_by_ids(&self, ids: &[Uuid]) -> Result<Vec<User>, AppError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        Ok(user_select()
            .filter(users::Column::Id.is_in(ids.to_vec()))
            .into_model::<UserRow>()
            .all(&self.db)
            .await?
            .into_iter()
            .map(row_to_domain)
            .collect())
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, AppError> {
        let t0 = std::time::Instant::now();
        let result = user_select()
            .filter(users::Column::Email.eq(email.to_lowercase()))
            .into_model::<UserRow>()
            .one(&self.db)
            .await?
            .map(row_to_domain);
        let elapsed = t0.elapsed();
        if elapsed.as_millis() > slow_query_threshold_ms() {
            tracing::warn!(elapsed_ms = elapsed.as_millis(), "slow_query: find_by_email");
        }
        Ok(result)
    }

    /// Case-insensitive, and load-bearing: `extract_mentions` lowercases what it
    /// captures, so a `=` comparison made `@TrungLe` resolve to no row and the
    /// notification silently never arrive. The same lookup backs `register`'s
    /// uniqueness check, so `Alice` could join an existing `alice`.
    ///
    /// `lower()` on both sides, matching `idx_users_username_lower` so this stays
    /// an index scan.
    async fn find_by_username(&self, username: &str) -> Result<Option<User>, AppError> {
        Ok(user_select()
            .filter(Expr::expr(Func::lower(Expr::col(users::Column::Username))).eq(username.to_lowercase()))
            .into_model::<UserRow>()
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

        let (sql, values) = Query::update()
            .table(users::Entity)
            .value(
                users::Column::FailedLoginCount,
                Expr::col(users::Column::FailedLoginCount).add(1i32),
            )
            .and_where(users::Column::Id.eq(id))
            .returning_col(users::Column::FailedLoginCount)
            .build(PostgresQueryBuilder);

        let row = FailedCount::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
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
        #[derive(FromQueryResult)]
        struct CountRow {
            cnt: i64,
        }

        let (sql, values) = Query::select()
            .expr_as(
                Expr::cust("COUNT(DISTINCT user_id)::BIGINT"),
                Alias::new("cnt"),
            )
            .from(user_roles::Entity)
            .inner_join(
                roles::Entity,
                Expr::col((roles::Entity, roles::Column::Id))
                    .equals((user_roles::Entity, user_roles::Column::RoleId)),
            )
            .and_where(Expr::col((roles::Entity, roles::Column::Slug)).eq("admin"))
            .and_where(
                Expr::col((user_roles::Entity, user_roles::Column::CategoryId)).is_null(),
            )
            .build(PostgresQueryBuilder);

        let row = CountRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
        .one(&self.db)
        .await?
        .ok_or_else(|| AppError::internal("count query failed".to_string()))?;
        Ok(row.cnt as u64)
    }

    async fn list_paginated<'a>(
        &self,
        page: u64,
        per_page: u64,
        search: Option<&'a str>,
        sort_by: Option<&'a str>,
        sort_dir: Option<&'a str>,
    ) -> Result<(Vec<User>, u64), AppError> {
        let offset = page.saturating_sub(1) * per_page;
        let order = match sort_dir.unwrap_or("desc") {
            "asc" => Order::Asc,
            _ => Order::Desc,
        };

        let mut base = user_select();
        let mut count_q = users::Entity::find();

        if let Some(q) = search.filter(|s| !s.is_empty()) {
            let pattern = format!("%{q}%");
            let cond = Condition::any()
                .add(Expr::col(users::Column::Username).ilike(pattern.clone()))
                .add(Expr::col(users::Column::Email).ilike(pattern.clone()))
                .add(Expr::col(users::Column::DisplayName).ilike(pattern));
            base = base.filter(cond.clone());
            count_q = count_q.filter(cond);
        }

        let total = count_q.count(&self.db).await?;

        match sort_by.unwrap_or("created_at") {
            "username"    => { base = base.order_by(users::Column::Username, order); }
            "post_count"  => { base = base.order_by(users::Column::PostCount, order); }
            "trust_level" => { base = base.order_by(users::Column::TrustLevel, order); }
            "is_banned"   => {
                base = base
                    .order_by(users::Column::IsBanned, order)
                    .order_by(users::Column::CreatedAt, Order::Desc);
            }
            _ => { base = base.order_by(users::Column::CreatedAt, order); }
        }

        let t0 = std::time::Instant::now();
        let rows = base
            .limit(per_page)
            .offset(offset)
            .into_model::<UserRow>()
            .all(&self.db)
            .await?;
        let elapsed = t0.elapsed();
        if elapsed.as_millis() > slow_query_threshold_ms() {
            tracing::warn!(elapsed_ms = elapsed.as_millis(), page, "slow_query: list_paginated");
        }
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
        let (prefs_row, muted_rows, watched_rows) = tokio::try_join!(
            user_preferences::Entity::find_by_id(user_id).one(&self.db),
            user_muted_categories::Entity::find()
                .filter(user_muted_categories::Column::UserId.eq(user_id))
                .all(&self.db),
            user_watched_categories::Entity::find()
                .filter(user_watched_categories::Column::UserId.eq(user_id))
                .all(&self.db),
        )?;

        let (theme, font_size, layout, email_notifications, locale, timezone) = match prefs_row {
            Some(m) => (
                m.theme,
                m.font_size,
                m.layout,
                m.email_notifications,
                // A tag that no longer parses — a locale removed from the roster,
                // or a hand-edited row — reads as "never chosen" rather than
                // failing the whole preferences load.
                m.locale.as_deref().and_then(Locale::parse),
                // Same tolerance for the zone: an empty string is not a value
                // any consumer can use, and treating it as "never chosen" means
                // the client falls back to the device zone rather than
                // rendering nothing.
                m.timezone.filter(|t| !t.trim().is_empty()),
            ),
            None => {
                let d = UserPreferences::default();
                (
                    d.theme,
                    d.font_size,
                    d.layout,
                    d.email_notifications,
                    d.locale,
                    d.timezone,
                )
            }
        };

        Ok(UserPreferences {
            user_id,
            theme,
            font_size,
            layout,
            email_notifications,
            muted_categories: muted_rows.into_iter().map(|m| m.category_id).collect(),
            watched_categories: watched_rows.into_iter().map(|m| m.category_id).collect(),
            locale,
            timezone,
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
            locale,
            timezone,
        } = prefs;

        let txn = self.db.begin().await?;

        user_preferences::Entity::insert(user_preferences::ActiveModel {
            user_id: Set(user_id),
            theme: Set(theme),
            font_size: Set(font_size),
            layout: Set(layout),
            email_notifications: Set(email_notifications),
            locale: Set(locale.map(|l| l.to_string())),
            timezone: Set(timezone),
            // `NotSet`, deliberately: the previous hand-maintained entity did not
            // model this column at all, so no write path has ever populated it.
            // Regeneration surfaced it; leaving it unwritten keeps behaviour
            // identical rather than quietly starting to maintain a timestamp
            // nothing reads. See the note in the upgrade summary.
            updated_at: NotSet,
        })
        .on_conflict(
            OnConflict::column(user_preferences::Column::UserId)
                .update_columns([
                    user_preferences::Column::Theme,
                    user_preferences::Column::FontSize,
                    user_preferences::Column::Layout,
                    user_preferences::Column::EmailNotifications,
                    user_preferences::Column::Locale,
                    user_preferences::Column::Timezone,
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

    async fn update_last_seen(&self, user_id: Uuid) -> Result<(), AppError> {
        // Increment days_visited only when the UTC calendar date has advanced since last visit.
        let (sql, values) = Query::update()
            .table(users::Entity)
            .value(users::Column::LastSeenAt, Expr::current_timestamp())
            .value(
                users::Column::DaysVisited,
                Expr::col(users::Column::DaysVisited).add(
                    CaseStatement::new()
                        .case(
                            Expr::col(users::Column::LastSeenAt)
                                .is_null()
                                .or(Expr::cust(
                                    "DATE(last_seen_at AT TIME ZONE 'UTC') < CURRENT_DATE",
                                )),
                            1i32,
                        )
                        .finally(0i32),
                ),
            )
            .and_where(users::Column::Id.eq(user_id))
            .build(PostgresQueryBuilder);

        self.db
            .execute_raw(Statement::from_sql_and_values(DbBackend::Postgres, sql, values))
            .await?;
        Ok(())
    }

    async fn increment_post_count(&self, user_id: Uuid, delta: i32) -> Result<(), AppError> {
        users::Entity::update_many()
            .col_expr(
                users::Column::PostCount,
                Func::greatest(vec![
                    Expr::val(0i32),
                    Expr::col(users::Column::PostCount).add(delta),
                ])
                .into(),
            )
            .filter(users::Column::Id.eq(user_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn increment_trust_score(&self, user_id: Uuid, amount: i32) -> Result<(), AppError> {
        users::Entity::update_many()
            .col_expr(
                users::Column::TrustScore,
                Func::greatest(vec![
                    Expr::val(0i32),
                    Func::least(vec![
                        Expr::val(100i32),
                        Expr::col(users::Column::TrustScore).add(amount),
                    ])
                    .into(),
                ])
                .into(),
            )
            .filter(users::Column::Id.eq(user_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn get_watched_categories(&self, user_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        let rows = user_watched_categories::Entity::find()
            .filter(user_watched_categories::Column::UserId.eq(user_id))
            .all(&self.db)
            .await?;
        Ok(rows.into_iter().map(|r| r.category_id).collect())
    }

    async fn get_muted_categories(&self, user_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        let rows = user_muted_categories::Entity::find()
            .filter(user_muted_categories::Column::UserId.eq(user_id))
            .all(&self.db)
            .await?;
        Ok(rows.into_iter().map(|r| r.category_id).collect())
    }

    async fn watch_category(&self, user_id: Uuid, category_id: Uuid) -> Result<(), AppError> {
        user_watched_categories::Entity::insert(user_watched_categories::ActiveModel {
            user_id: Set(user_id),
            category_id: Set(category_id),
        })
        .on_conflict(OnConflict::columns([
            user_watched_categories::Column::UserId,
            user_watched_categories::Column::CategoryId,
        ]).do_nothing().to_owned())
        .exec(&self.db)
        .await?;
        Ok(())
    }

    async fn unwatch_category(&self, user_id: Uuid, category_id: Uuid) -> Result<(), AppError> {
        user_watched_categories::Entity::delete_many()
            .filter(user_watched_categories::Column::UserId.eq(user_id))
            .filter(user_watched_categories::Column::CategoryId.eq(category_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn mute_category(&self, user_id: Uuid, category_id: Uuid) -> Result<(), AppError> {
        user_muted_categories::Entity::insert(user_muted_categories::ActiveModel {
            user_id: Set(user_id),
            category_id: Set(category_id),
        })
        .on_conflict(OnConflict::columns([
            user_muted_categories::Column::UserId,
            user_muted_categories::Column::CategoryId,
        ]).do_nothing().to_owned())
        .exec(&self.db)
        .await?;
        Ok(())
    }

    async fn unmute_category(&self, user_id: Uuid, category_id: Uuid) -> Result<(), AppError> {
        user_muted_categories::Entity::delete_many()
            .filter(user_muted_categories::Column::UserId.eq(user_id))
            .filter(user_muted_categories::Column::CategoryId.eq(category_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }
}
