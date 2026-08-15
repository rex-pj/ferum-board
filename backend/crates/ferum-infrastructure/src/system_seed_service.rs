//! System data seeding — the rows the application cannot run without.
//!
//! Every step is insert-if-absent and runs on **every** startup, which is what
//! lets a permission added in a later version reach an existing install with no
//! migration. Migrations stay DDL-only.
//!
//! One asymmetry: role → permission grants are written only for keys this run
//! created. Re-asserting them each boot would silently undo an admin's
//! revocation on the next restart.

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    TransactionTrait,
};
use uuid::{uuid, Uuid};

use ferum_application::constants::{
    DEFAULT_ACCOUNT_LOCKOUT_ATTEMPTS, DEFAULT_ACCOUNT_LOCKOUT_DURATION_MINUTES,
    DEFAULT_AUTH_RATE_LIMIT_PER_MIN, DEFAULT_FORUM_INDEX_THREADS_PER_CATEGORY,
    DEFAULT_MAX_POSTS_PER_PAGE, DEFAULT_MAX_THREADS_PER_PAGE, DEFAULT_POST_EDIT_WINDOW_HOURS,
    DEFAULT_PUBLIC_WRITE_RATE_LIMIT_PER_MIN, DEFAULT_REPORTING_TIMEZONE, DEFAULT_SITE_NAME,
    DEFAULT_THEME_SLUG,
};
use ferum_application::shared::AppError;
use ferum_domain::models::role::{PERMISSIONS, SYSTEM_ROLES};

use crate::crypto::{
    plugin_config_aad, site_config_aad, SecretCipher, ENCRYPTED_CONFIG_KEYS, SEALED_PREFIX,
    WEBHOOK_SECRET_AAD,
};
use crate::entities::{
    permissions, plugins, product_categories, role_permissions, roles, site_config, themes,
    webhooks,
};
use ferum_domain::models::plugin::secret_config_keys;
use crate::repositories::user_repository::domain_trust_to_entity;

/// Fixed so the built-in theme keeps one identity across reinstalls — a theme
/// row is referenced by slug everywhere, but a stable id makes the row
/// recognisable in an audit log.
const DEFAULT_THEME_ID: Uuid = uuid!("70000000-0000-0000-0000-000000000001");

/// The catalogue taxonomy: slug, name, icon, match keywords.
///
/// Keywords drive auto-assignment and are stored unaccented and lowercase — the
/// matcher (`PgProductCategoryRepository`) folds the product name the same way,
/// so "GHẾ", "ghe" and "Ghế" all land on the same row. They are data, editable
/// from /admin/products → Categories, so this list is only the starting point.
const PRODUCT_CATEGORIES: &[(&str, &str, &str, &[&str])] = &[
    ("sofa", "Sofa", "fa-couch", &["sofa", "ghe sofa", "salon"]),
    ("ghe", "Ghế", "fa-chair", &["ghe", "ghe an", "ghe don", "ghe bar", "stool"]),
    ("ban", "Bàn", "fa-table", &["ban", "ban an", "ban tra", "ban lam viec", "desk"]),
    ("tu-ke", "Tủ & Kệ", "fa-boxes-stacked", &["tu", "ke", "tu quan ao", "ke sach", "cabinet"]),
    ("giuong-nem", "Giường & Nệm", "fa-bed", &["giuong", "nem", "dem", "mattress"]),
    ("den", "Đèn", "fa-lightbulb", &["den", "den ban", "den tran", "lamp"]),
    ("trang-tri", "Trang trí", "fa-image", &["tham", "guong", "tranh", "rem", "binh hoa"]),
];

/// Site configuration defaults.
///
/// Every value is the same constant the reader falls back to when the key is
/// absent, so seeding changes nothing behaviourally — it makes the admin
/// settings form show the real numbers instead of empty boxes.
///
/// `enabled_locales` and `default_locale` are deliberately absent: an unset
/// `enabled_locales` means "every installed locale", which is not expressible
/// as a value. Writing one here would silently *restrict* the site to whatever
/// list we guessed at build time.
fn config_defaults() -> Vec<(&'static str, String)> {
    vec![
        ("site_name", DEFAULT_SITE_NAME.to_string()),
        ("site_slogan", String::new()),
        ("site_tagline", "A modern self-hosted forum".to_string()),
        ("logo_url", String::new()),
        ("favicon_url", String::new()),
        ("primary_color", "#0d6efd".to_string()),
        ("registration_open", "true".to_string()),
        ("keyword_blacklist", String::new()),
        ("smtp_host", String::new()),
        ("smtp_port", "587".to_string()),
        ("smtp_user", String::new()),
        ("post_approval_enabled", "false".to_string()),
        ("post_approval_min_trust", "new".to_string()),
        ("auth_rate_limit_per_min", DEFAULT_AUTH_RATE_LIMIT_PER_MIN.to_string()),
        ("public_write_rate_limit_per_min", DEFAULT_PUBLIC_WRITE_RATE_LIMIT_PER_MIN.to_string()),
        ("account_lockout_attempts", DEFAULT_ACCOUNT_LOCKOUT_ATTEMPTS.to_string()),
        ("account_lockout_duration_minutes", DEFAULT_ACCOUNT_LOCKOUT_DURATION_MINUTES.to_string()),
        ("post_edit_window_hours", DEFAULT_POST_EDIT_WINDOW_HOURS.to_string()),
        ("max_threads_per_page", DEFAULT_MAX_THREADS_PER_PAGE.to_string()),
        ("max_posts_per_page", DEFAULT_MAX_POSTS_PER_PAGE.to_string()),
        ("forum_index_threads_per_category", DEFAULT_FORUM_INDEX_THREADS_PER_CATEGORY.to_string()),
        // Seeded as UTC so an upgrade changes no existing figure. See the
        // constant for what setting it actually does to the charts.
        ("reporting_timezone", DEFAULT_REPORTING_TIMEZONE.to_string()),
    ]
}

/// Role ids by slug, plus which of them this run created — the second half is
/// what tells `seed_grants` a role needs its full default list rather than only
/// the newly defined keys.
struct SeededRoles {
    ids: HashMap<String, Uuid>,
    new_slugs: Vec<&'static str>,
}

pub struct PgSystemSeedService {
    db: DatabaseConnection,
}

impl PgSystemSeedService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Bring the database up to the system baseline. Idempotent.
    ///
    /// Must run after migrations and before `RolePermissionCache::load()` —
    /// the cache resolves the rows written here.
    #[tracing::instrument(skip_all)]
    pub async fn seed_system(&self) -> Result<(), AppError> {
        let SeededRoles { ids: role_ids, new_slugs } = self.seed_roles().await?;
        let new_permission_keys = self.seed_permissions().await?;
        self.seed_grants(&role_ids, &new_slugs, &new_permission_keys).await?;
        self.seed_site_config().await?;
        self.seed_default_theme().await?;
        self.seed_product_categories().await?;

        tracing::info!(
            roles = role_ids.len(),
            new_permissions = new_permission_keys.len(),
            "system seed complete"
        );
        Ok(())
    }

    /// Encrypts plaintext secrets and re-seals anything that only opened under
    /// the previous key. Returns the row count. Also the rotation mechanism.
    ///
    /// Eager rather than lazy-on-write because "the next time an admin edits
    /// SMTP" may be never, and until then the plaintext is in every backup.
    /// Not a migration: the key is in the environment, which DDL cannot see.
    ///
    /// Idempotent, and one transaction so a partial failure leaves no mixture.
    #[tracing::instrument(skip_all)]
    pub async fn seal_existing_secrets(&self, cipher: &SecretCipher) -> Result<usize, AppError> {
        let txn = self.db.begin().await?;
        let mut rewritten = 0usize;

        for key in ENCRYPTED_CONFIG_KEYS {
            let Some(row) = site_config::Entity::find_by_id(*key).one(&txn).await? else {
                continue;
            };
            // A blank value is "not set"; sealing it would make it look set.
            if row.value.is_empty() {
                continue;
            }
            let aad = site_config_aad(key);
            let opened = cipher.open(&aad, &row.value)?;
            if !opened.needs_reseal {
                continue;
            }
            let sealed = cipher.seal(&aad, &opened.value)?;
            let mut active: site_config::ActiveModel = row.into();
            active.value = Set(sealed);
            active.updated_at = Set(Utc::now().fixed_offset());
            active.update(&txn).await?;
            rewritten += 1;
        }

        for row in webhooks::Entity::find().all(&txn).await? {
            let Some(stored) = row.secret.clone() else {
                continue;
            };
            if stored.is_empty() {
                continue;
            }
            let opened = cipher.open(WEBHOOK_SECRET_AAD, &stored)?;
            if !opened.needs_reseal {
                continue;
            }
            let sealed = cipher.seal(WEBHOOK_SECRET_AAD, &opened.value)?;
            let mut active: webhooks::ActiveModel = row.into();
            active.secret = Set(Some(sealed));
            active.update(&txn).await?;
            rewritten += 1;
        }

        // Plugin credentials. Unlike the two above, which fields are secret is not
        // known here — it comes from each plugin's own manifest.
        for row in plugins::Entity::find().all(&txn).await? {
            let keys = secret_config_keys(&row.manifest);
            if keys.is_empty() {
                continue;
            }
            let mut config = row.config.clone();
            let Some(obj) = config.as_object_mut() else {
                continue;
            };
            let mut touched = false;
            for key in keys {
                let Some(stored) = obj.get(&key).and_then(|v| v.as_str()) else {
                    continue;
                };
                if stored.is_empty() {
                    continue;
                }
                let aad = plugin_config_aad(&row.slug, &key);
                let opened = cipher.open(&aad, stored)?;
                if !opened.needs_reseal {
                    continue;
                }
                let sealed = cipher.seal(&aad, &opened.value)?;
                obj.insert(key, serde_json::Value::String(sealed));
                touched = true;
            }
            if !touched {
                continue;
            }
            let mut active: plugins::ActiveModel = row.into();
            active.config = Set(config);
            active.updated_at = Set(Utc::now().fixed_offset());
            active.update(&txn).await?;
            rewritten += 1;
        }

        txn.commit().await?;
        // A count, never a value.
        if rewritten > 0 {
            tracing::info!(count = rewritten, "sealed secrets at rest");
        }
        Ok(rewritten)
    }

    /// Proves the configured key matches the data already in this database.
    ///
    /// Returns `Err` when a sealed value will not open. That has to abort startup
    /// rather than warn: running on would leave the site unable to read its own
    /// secrets, and the first save of any of them would re-seal under the new key
    /// and make the originals permanently unrecoverable.
    ///
    /// **Must run before [`Self::seal_existing_secrets`]** — sweeping first with a
    /// wrong key is precisely the unrecoverable event above.
    pub async fn verify_secret_key(&self, cipher: &SecretCipher) -> Result<(), AppError> {
        for key in ENCRYPTED_CONFIG_KEYS {
            if let Some(row) = site_config::Entity::find_by_id(*key).one(&self.db).await? {
                if SecretCipher::is_sealed(&row.value) {
                    cipher.open(&site_config_aad(key), &row.value)?;
                }
            }
        }
        if let Some(row) = webhooks::Entity::find()
            .filter(webhooks::Column::Secret.is_not_null())
            .one(&self.db)
            .await?
        {
            if let Some(stored) = row.secret.as_deref() {
                if SecretCipher::is_sealed(stored) {
                    cipher.open(WEBHOOK_SECRET_AAD, stored)?;
                }
            }
        }
        // Plugin config. Every plugin is scanned rather than the first one found:
        // `LIKE '%enc:v1:%'` cannot say *which* field is sealed, and a plugin with
        // no secret fields at all is the common case, so a one-row probe would
        // usually verify nothing.
        for row in plugins::Entity::find().all(&self.db).await? {
            for key in secret_config_keys(&row.manifest) {
                let Some(stored) = row.config.get(&key).and_then(|v| v.as_str()) else {
                    continue;
                };
                if SecretCipher::is_sealed(stored) {
                    cipher.open(&plugin_config_aad(&row.slug, &key), stored)?;
                }
            }
        }
        Ok(())
    }

    /// True when any secret in this database is already encrypted.
    ///
    /// Used to refuse startup when the key has been *removed* from a deployment
    /// that has sealed data: without it those values are unreadable, and carrying
    /// on would look like a working boot.
    pub async fn has_sealed_secrets(&self) -> Result<bool, AppError> {
        for key in ENCRYPTED_CONFIG_KEYS {
            if let Some(row) = site_config::Entity::find_by_id(*key).one(&self.db).await? {
                if SecretCipher::is_sealed(&row.value) {
                    return Ok(true);
                }
            }
        }
        let sealed_webhook = webhooks::Entity::find()
            .filter(webhooks::Column::Secret.starts_with(SEALED_PREFIX))
            .one(&self.db)
            .await?;
        if sealed_webhook.is_some() {
            return Ok(true);
        }
        // `contains`, not `starts_with`: the sealed value is a field *inside* a
        // JSONB document, so the prefix appears mid-string. A false positive here
        // would need a plugin storing the literal text `enc:v1:` in its config,
        // and the cost of one is a refused boot with an actionable message — the
        // right way round for a check whose job is to catch a removed key.
        let sealed_plugin = plugins::Entity::find()
            .filter(Expr::cust_with_values(
                "plugins.config::text LIKE $1",
                [format!("%{SEALED_PREFIX}%")],
            ))
            .one(&self.db)
            .await?;
        Ok(sealed_plugin.is_some())
    }

    /// Insert missing system roles. An existing row is never touched: admins
    /// rename and recolour roles, and `is_default` can legitimately be moved to
    /// a custom role.
    async fn seed_roles(&self) -> Result<SeededRoles, AppError> {
        let existing: HashMap<String, Uuid> = roles::Entity::find()
            .all(&self.db)
            .await?
            .into_iter()
            .map(|r| (r.slug, r.id))
            .collect();

        let mut ids = existing.clone();
        let mut new_slugs: Vec<&'static str> = Vec::new();
        let missing: Vec<roles::ActiveModel> = SYSTEM_ROLES
            .iter()
            .filter(|def| !existing.contains_key(def.slug))
            .map(|def| {
                let id = Uuid::new_v4();
                ids.insert(def.slug.to_string(), id);
                new_slugs.push(def.slug);
                roles::ActiveModel {
                    id: Set(id),
                    slug: Set(def.slug.to_string()),
                    name: Set(def.name.to_string()),
                    description: Set(Some(def.description.to_string())),
                    color: Set(Some(def.color.to_string())),
                    is_system: Set(true),
                    is_default: Set(def.is_default),
                    position: Set(def.position),
                    created_at: Set(Utc::now().fixed_offset()),
                    updated_at: Set(None),
                }
            })
            .collect();

        if !missing.is_empty() {
            roles::Entity::insert_many(missing).exec(&self.db).await?;
        }
        Ok(SeededRoles { ids, new_slugs })
    }

    /// Insert missing permissions and refresh the metadata of existing ones.
    ///
    /// Description, group and `min_trust` are owned by the code — no admin
    /// endpoint edits them — so an existing row is brought back in line rather
    /// than left to drift.
    ///
    /// Returns the keys that did not exist before this call; those are the only
    /// ones that get default grants.
    async fn seed_permissions(&self) -> Result<Vec<&'static str>, AppError> {
        let existing: HashMap<String, permissions::Model> = permissions::Entity::find()
            .all(&self.db)
            .await?
            .into_iter()
            .map(|p| (p.key.clone(), p))
            .collect();

        let mut new_keys: Vec<&'static str> = Vec::new();
        let mut to_insert: Vec<permissions::ActiveModel> = Vec::new();

        for def in PERMISSIONS {
            let min_trust = domain_trust_to_entity(&def.min_trust);
            match existing.get(def.key) {
                None => {
                    new_keys.push(def.key);
                    to_insert.push(permissions::ActiveModel {
                        id: Set(Uuid::new_v4()),
                        key: Set(def.key.to_string()),
                        description: Set(def.description.to_string()),
                        group_name: Set(def.group_name.to_string()),
                        min_trust: Set(min_trust),
                    });
                }
                Some(row) => {
                    let stale = row.description != def.description
                        || row.group_name != def.group_name
                        || row.min_trust != min_trust;
                    if stale {
                        permissions::Entity::update(permissions::ActiveModel {
                            id: Set(row.id),
                            key: Set(def.key.to_string()),
                            description: Set(def.description.to_string()),
                            group_name: Set(def.group_name.to_string()),
                            min_trust: Set(min_trust),
                        })
                        .exec(&self.db)
                        .await?;
                    }
                }
            }
        }

        if !to_insert.is_empty() {
            permissions::Entity::insert_many(to_insert).exec(&self.db).await?;
        }
        Ok(new_keys)
    }

    /// Write the default grants that are genuinely new, and only those:
    ///
    /// - a permission created in this run → every system role that grants it,
    /// - a role created in this run → its whole default list.
    ///
    /// Anything already present is left exactly as the admin left it. Without
    /// the second case a system role that had somehow been removed would come
    /// back as an empty shell, which is worse than it being absent.
    async fn seed_grants(
        &self,
        role_ids: &HashMap<String, Uuid>,
        new_role_slugs: &[&'static str],
        new_keys: &[&'static str],
    ) -> Result<(), AppError> {
        if new_keys.is_empty() && new_role_slugs.is_empty() {
            return Ok(());
        }

        // Every key when a role is new, otherwise only the new keys.
        let mut wanted: HashSet<&'static str> = new_keys.iter().copied().collect();
        if !new_role_slugs.is_empty() {
            wanted.extend(PERMISSIONS.iter().map(|p| p.key));
        }

        let permission_ids: HashMap<String, Uuid> = permissions::Entity::find()
            .filter(permissions::Column::Key.is_in(wanted.iter().copied()))
            .all(&self.db)
            .await?
            .into_iter()
            .map(|p| (p.key, p.id))
            .collect();

        let mut rows: Vec<role_permissions::ActiveModel> = Vec::new();
        for def in SYSTEM_ROLES {
            let Some(&role_id) = role_ids.get(def.slug) else {
                continue;
            };
            let role_is_new = new_role_slugs.contains(&def.slug);
            for (key, &permission_id) in &permission_ids {
                let key = key.as_str();
                // An established role only receives keys that are themselves new.
                if !role_is_new && !new_keys.contains(&key) {
                    continue;
                }
                if !def.grants_key(key) {
                    continue;
                }
                rows.push(role_permissions::ActiveModel {
                    role_id: Set(role_id),
                    permission_id: Set(permission_id),
                });
            }
        }

        if !rows.is_empty() {
            // DO NOTHING rather than an error: a partially completed previous
            // run must be able to finish.
            role_permissions::Entity::insert_many(rows)
                .on_conflict(
                    sea_orm::sea_query::OnConflict::columns([
                        role_permissions::Column::RoleId,
                        role_permissions::Column::PermissionId,
                    ])
                    .do_nothing()
                    .to_owned(),
                )
                // `InsertMany::do_nothing` was deprecated in 2.0; `try_insert` is
                // documented as the same wrapper — it maps `DbErr::RecordNotInserted`
                // to `TryInsertResult::Conflicted` rather than erroring, which is
                // what keeps a fully-seeded database idempotent here.
                .try_insert()
                .exec(&self.db)
                .await?;
        }
        Ok(())
    }

    async fn seed_site_config(&self) -> Result<(), AppError> {
        let existing: HashSet<String> = site_config::Entity::find()
            .all(&self.db)
            .await?
            .into_iter()
            .map(|c| c.key)
            .collect();

        let now = Utc::now().fixed_offset();
        let rows: Vec<site_config::ActiveModel> = config_defaults()
            .into_iter()
            .filter(|(key, _)| !existing.contains(*key))
            .map(|(key, value)| site_config::ActiveModel {
                key: Set(key.to_string()),
                value: Set(value),
                updated_at: Set(now),
                updated_by_id: Set(None),
            })
            .collect();

        if !rows.is_empty() {
            site_config::Entity::insert_many(rows).exec(&self.db).await?;
        }
        Ok(())
    }

    /// The built-in theme row. Absent-only: `is_active` must never be reclaimed
    /// from a theme the admin activated.
    async fn seed_default_theme(&self) -> Result<(), AppError> {
        let exists = themes::Entity::find()
            .filter(themes::Column::Slug.eq(DEFAULT_THEME_SLUG))
            .one(&self.db)
            .await?
            .is_some();
        if exists {
            return Ok(());
        }

        themes::Entity::insert(themes::ActiveModel {
            id: Set(DEFAULT_THEME_ID),
            slug: Set(DEFAULT_THEME_SLUG.to_string()),
            name: Set("Default".to_string()),
            author: Set(Some("Ferum Board".to_string())),
            version: Set("1.0.0".to_string()),
            description: Set(Some("Built-in default theme".to_string())),
            // Its own parent: the resolution chain is always rooted here, and a
            // NULL would make the root a special case for every walker.
            parent_slug: Set(DEFAULT_THEME_SLUG.to_string()),
            is_system: Set(true),
            is_active: Set(true),
            preview_url: Set(None),
            created_at: Set(Utc::now().fixed_offset()),
        })
        .exec(&self.db)
        .await?;
        Ok(())
    }

    async fn seed_product_categories(&self) -> Result<(), AppError> {
        let existing: HashSet<String> = product_categories::Entity::find()
            .all(&self.db)
            .await?
            .into_iter()
            .map(|c| c.slug)
            .collect();

        let now = Utc::now().fixed_offset();
        let rows: Vec<product_categories::ActiveModel> = PRODUCT_CATEGORIES
            .iter()
            .enumerate()
            .filter(|(_, (slug, ..))| !existing.contains(*slug))
            .map(|(i, (slug, name, icon, keywords))| product_categories::ActiveModel {
                id: Set(Uuid::new_v4()),
                slug: Set((*slug).to_string()),
                name: Set((*name).to_string()),
                parent_id: Set(None),
                position: Set(i as i32),
                icon: Set(Some((*icon).to_string())),
                match_keywords: Set(keywords.iter().map(|k| (*k).to_string()).collect()),
                created_at: Set(now),
            })
            .collect();

        if !rows.is_empty() {
            product_categories::Entity::insert_many(rows).exec(&self.db).await?;
        }
        Ok(())
    }
}
