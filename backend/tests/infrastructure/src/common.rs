//! Integration-test harness: each [`TestDb`] provisions a throwaway database.
//!
//! Migrations run once into a template database and each test copies it with
//! `CREATE DATABASE … TEMPLATE`, so setup is O(migrations + N) rather than
//! O(migrations × N).
//!
//! Needs a reachable PostgreSQL whose superuser can `CREATE DATABASE`, from
//! `TEST_DATABASE_URL` or `DATABASE_URL`.

use std::sync::{Once, OnceLock};

use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement};
use uuid::Uuid;

use migration::MigratorTrait;

use ferum_domain::models::category::{Category, PostPolicy, ViewPolicy};
use ferum_domain::models::post::{Post, PostStatus};
use ferum_domain::models::role::Role;
use ferum_domain::models::thread::Thread;
use ferum_domain::models::user::User;
use ferum_domain::repositories::category_repository::{CategoryRepository, NewCategory};
use ferum_domain::repositories::post_repository::{NewPost, PostRepository};
use ferum_domain::repositories::role_repository::{NewRole, RoleRepository};
use ferum_domain::repositories::thread_repository::{NewThread, ThreadRepository};
use ferum_domain::repositories::user_repository::{NewUser, UserRepository};
use ferum_infrastructure::repositories::{
    PgCategoryRepository, PgPostRepository, PgRoleRepository, PgThreadRepository,
    PgUserRepository,
};
use ferum_infrastructure::system_seed_service::PgSystemSeedService;

const TEMPLATE_NAME: &str = "ferum_test_template";

static INIT_ENV: Once = Once::new();
/// The outcome of building the template DB, computed once per process.
///
/// **It stores a `Result` rather than `()`, and that is the whole point.**
/// `OnceLock::get_or_init` leaves the cell *uninitialised* when its closure
/// panics, so an `expect` anywhere inside meant the next of ~500 parallel tests
/// re-entered and ran `DROP DATABASE … WITH (FORCE)` a second time — against the
/// template that the tests already running were cloning from. One transient
/// failure to reach Postgres therefore turned into a whole suite reporting
/// `template database "ferum_test_template" does not exist`, with the real cause
/// scrolled off the top and thirty unrelated repository tests named as failures.
///
/// Returning the error instead means the destructive step happens at most once
/// per process no matter what, and every caller reports the same true cause.
static TEMPLATE: OnceLock<Result<(), String>> = OnceLock::new();

/// Build the shared template database on first call, then return immediately
/// on all subsequent calls. Uses a plain OS thread so there is no nested-
/// tokio-runtime problem (each `#[tokio::test]` has its own runtime).
///
/// # Panics
/// With the recorded reason if the build failed — identically for every test, so
/// the failure is attributable to the bootstrap rather than to whichever test
/// happened to run next.
fn ensure_template(server_url: &str) {
    let outcome = TEMPLATE.get_or_init(|| {
        let url = server_url.to_string();
        // Nothing inside may panic: see the note on `TEMPLATE`. `?` on a
        // `Result<_, String>` throughout, and the `join` below converts a panic
        // that slips through anyway into the same stored error.
        let built = std::thread::spawn(move || -> Result<(), String> {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("build runtime for template-db init: {e}"))?
                .block_on(async move {
                    let admin = Database::connect(format!("{url}/postgres"))
                        .await
                        .map_err(|e| format!("connect to postgres for template creation: {e}"))?;
                    // Drop stale template from a previous run (if any). Reached
                    // at most once per process, which is what makes it safe to
                    // do while other tests hold clones of it.
                    try_exec(&admin, &format!("DROP DATABASE IF EXISTS \"{TEMPLATE_NAME}\" WITH (FORCE)")).await?;
                    try_exec(&admin, &format!("CREATE DATABASE \"{TEMPLATE_NAME}\"")).await?;
                    admin.close().await.ok();

                    let conn = Database::connect(format!("{url}/{TEMPLATE_NAME}"))
                        .await
                        .map_err(|e| format!("connect to template database: {e}"))?;
                    migration::Migrator::up(&conn, None)
                        .await
                        .map_err(|e| format!("run migrations on template database: {e}"))?;
                    // Migrations are DDL only; the system roles and permissions
                    // these tests assert on are written here, exactly as they
                    // are at startup.
                    PgSystemSeedService::new(conn.clone())
                        .seed_system()
                        .await
                        .map_err(|e| format!("seed system data on template database: {e}"))?;
                    conn.close().await.ok();
                    Ok(())
                })
        })
        .join();

        match built {
            Ok(result) => result,
            Err(_) => Err("template-db initialisation thread panicked".to_string()),
        }
    });

    if let Err(reason) = outcome {
        panic!("template database unavailable: {reason}");
    }
}

/// A per-test database cloned from `ferum_test_template`.
pub struct TestDb {
    /// Connection to the per-test database.
    pub conn: DatabaseConnection,
    /// `postgres://…@host:port` without a trailing database segment.
    server_url: String,
    /// Name of the throwaway database (dropped on [`teardown`]).
    db_name: String,
}

impl TestDb {
    /// Clone the shared template into `ferum_test_<label>` and return a live
    /// connection. `label` must be unique per test so parallel tests don't
    /// collide.
    ///
    /// The session runs with `TimeZone=UTC` — not because anything here sets
    /// it, but because `sqlx-postgres` puts `("TimeZone", "UTC")` in every
    /// startup packet. See [`TestDb::new_in_timezone`].
    pub async fn new(label: &str) -> Self {
        Self::connect(label, None).await
    }

    /// Same, but with the session `TimeZone` forced to `tz`.
    ///
    /// Asserts that the SQL names its own day boundary rather than inheriting
    /// the session's — the property that survives a driver change or a pooler.
    /// A non-whole-hour zone (`Asia/Kathmandu`, +05:45) is the useful value: it
    /// catches code that truncates instead of converting.
    ///
    /// **Single-connection pool**, so queries must be issued sequentially.
    pub async fn new_in_timezone(label: &str, tz: &str) -> Self {
        Self::connect(label, Some(tz)).await
    }

    async fn connect(label: &str, force_tz: Option<&str>) -> Self {
        INIT_ENV.call_once(|| {
            let _ = dotenvy::from_filename("../../.env");
            let _ = dotenvy::dotenv();
        });

        let base = std::env::var("TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .expect("TEST_DATABASE_URL or DATABASE_URL must be set");

        // Safety guard: refuse to run against a non-local server so tests can
        // never accidentally create/drop databases on a staging or production host.
        assert!(
            base.contains("localhost") || base.contains("127.0.0.1"),
            "Refusing to run integration tests against a non-local database.\n\
             Set TEST_DATABASE_URL to a local PostgreSQL instance.\n\
             Current URL: {base}",
        );

        let (server_url, _existing_db) = base
            .rsplit_once('/')
            .expect("DATABASE_URL must contain a database segment");
        let server_url = server_url.to_string();

        // Build the template once; all parallel tests block here until ready.
        ensure_template(&server_url);

        let db_name = format!("ferum_test_{label}");

        let admin = Database::connect(format!("{server_url}/postgres"))
            .await
            .expect("connect to maintenance database 'postgres'");
        // Drop any stale copy left over from a previous failed run.
        exec(&admin, &format!("DROP DATABASE IF EXISTS \"{db_name}\" WITH (FORCE)")).await;
        // Clone template — no migration run needed.
        exec(&admin, &format!("CREATE DATABASE \"{db_name}\" TEMPLATE \"{TEMPLATE_NAME}\"")).await;
        admin.close().await.ok();

        let conn = match force_tz {
            // Ordinary path: a normal pool, whatever size sea-orm defaults to.
            None => Database::connect(format!("{server_url}/{db_name}"))
                .await
                .expect("connect to per-test database"),

            // One connection, because neither alternative works: `?options=-c
            // TimeZone=…` is beaten by the `("TimeZone", "UTC")` sqlx hardcodes
            // into every startup packet, and a plain `SET` on a normal pool
            // lands on one arbitrary connection — flaky by construction.
            Some(tz) => {
                let mut opts = sea_orm::ConnectOptions::new(format!("{server_url}/{db_name}"));
                opts.max_connections(1).min_connections(1).sqlx_logging(false);
                let conn = Database::connect(opts)
                    .await
                    .expect("connect to per-test database");
                exec(&conn, &format!("SET TimeZone TO '{tz}'")).await;
                conn
            }
        };

        Self { conn, server_url, db_name }
    }

    /// Drop the per-test database. If skipped, the next run with the same
    /// label reclaims the name via `DROP … IF EXISTS`.
    pub async fn teardown(self) {
        let TestDb { conn, server_url, db_name } = self;
        conn.close().await.ok();
        if let Ok(admin) = Database::connect(format!("{server_url}/postgres")).await {
            exec(&admin, &format!("DROP DATABASE IF EXISTS \"{db_name}\" WITH (FORCE)")).await;
            admin.close().await.ok();
        }
    }
}

async fn exec(conn: &DatabaseConnection, sql: &str) {
    conn.execute_raw(Statement::from_string(DbBackend::Postgres, sql.to_owned()))
        .await
        .unwrap_or_else(|e| panic!("admin statement failed ({sql}): {e}"));
}

/// [`exec`] for the template bootstrap, which must not panic — see [`TEMPLATE`].
async fn try_exec(conn: &DatabaseConnection, sql: &str) -> Result<(), String> {
    conn.execute_raw(Statement::from_string(DbBackend::Postgres, sql.to_owned()))
        .await
        .map(|_| ())
        .map_err(|e| format!("admin statement failed ({sql}): {e}"))
}

// ─── Fixture helpers ──────────────────────────────────────────────────────────

pub async fn insert_user(conn: &DatabaseConnection, n: u8) -> User {
    PgUserRepository::new(conn.clone())
        .create(NewUser {
            username: format!("user{n}"),
            email: format!("user{n}@example.com"),
            password_hash: Some("$2b$12$fakehash".to_string()),
        })
        .await
        .unwrap_or_else(|e| panic!("insert_user({n}): {e}"))
}

pub async fn insert_category(conn: &DatabaseConnection, n: u8) -> Category {
    PgCategoryRepository::new(conn.clone())
        .create(NewCategory {
            slug: format!("cat-{n}"),
            name: format!("Category {n}"),
            description: None,
            parent_id: None,
            position: n as i32,
            view_policy: ViewPolicy::Public,
            post_policy: PostPolicy::Members,
            color: None,
            created_by_id: None,
        })
        .await
        .unwrap_or_else(|e| panic!("insert_category({n}): {e}"))
}

pub async fn insert_thread(
    conn: &DatabaseConnection,
    n: u8,
    category_id: Uuid,
    author_id: Uuid,
) -> Thread {
    PgThreadRepository::new(conn.clone())
        .create(NewThread {
            id: Uuid::new_v4(),
            category_id,
            author_id,
            title: format!("Thread {n}"),
            slug: format!("thread-{n}"), product_id: None,
        })
        .await
        .unwrap_or_else(|e| panic!("insert_thread({n}): {e}"))
}

pub async fn insert_post(conn: &DatabaseConnection, thread_id: Uuid, author_id: Uuid) -> Post {
    PgPostRepository::new(conn.clone())
        .create(NewPost {
            thread_id,
            author_id,
            parent_id: None,
            content_md: "Test post content".to_string(),
            content_html: "<p>Test post content</p>".to_string(),
            status: PostStatus::Published,
        })
        .await
        .unwrap_or_else(|e| panic!("insert_post: {e}"))
}

pub async fn insert_role(conn: &DatabaseConnection, slug: &str) -> Role {
    let repo = PgRoleRepository::new(conn.clone());
    if let Some(existing) = repo.find_by_slug(slug).await
        .unwrap_or_else(|e| panic!("insert_role({slug}) find: {e}"))
    {
        return existing;
    }
    repo.create(NewRole {
        slug: slug.to_string(),
        name: format!("{slug} role"),
        description: None,
        color: None,
        position: 0,
    })
    .await
    .unwrap_or_else(|e| panic!("insert_role({slug}): {e}"))
}
