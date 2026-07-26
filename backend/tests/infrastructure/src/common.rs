//! Integration-test harness for repository tests.
//!
//! Each [`TestDb`] provisions an isolated, throwaway PostgreSQL database and
//! exposes a `DatabaseConnection`. Migrations run **once** against a shared
//! template database (`ferum_test_template`); each per-test database is then
//! created with `CREATE DATABASE … TEMPLATE ferum_test_template`, which lets
//! PostgreSQL copy pages at the file-system level instead of re-running the
//! migration chain for every test. This cuts setup time from O(migrations × N)
//! to O(migrations + N × template-copy).
//!
//! ## Requirements
//! A reachable PostgreSQL whose superuser can `CREATE DATABASE`. The base URL is
//! read from `TEST_DATABASE_URL`, falling back to `DATABASE_URL` (loaded from
//! `backend/.env`).
//!
//! ## Running
//! ```text
//! cargo test -p ferum-infrastructure-tests
//! ```

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
/// Ensures the template DB is created exactly once per process, even when
/// many tests race to call `TestDb::new` in parallel. `OnceLock` blocks
/// concurrent callers until the winning thread finishes the init closure.
static TEMPLATE: OnceLock<()> = OnceLock::new();

/// Build the shared template database on first call, then return immediately
/// on all subsequent calls. Uses a plain OS thread so there is no nested-
/// tokio-runtime problem (each `#[tokio::test]` has its own runtime).
fn ensure_template(server_url: &str) {
    TEMPLATE.get_or_init(|| {
        let url = server_url.to_string();
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build runtime for template-db init")
                .block_on(async move {
                    let admin = Database::connect(format!("{url}/postgres"))
                        .await
                        .expect("connect to postgres for template creation");
                    // Drop stale template from a previous run (if any).
                    exec(&admin, &format!("DROP DATABASE IF EXISTS \"{TEMPLATE_NAME}\" WITH (FORCE)")).await;
                    exec(&admin, &format!("CREATE DATABASE \"{TEMPLATE_NAME}\"")).await;
                    admin.close().await.ok();

                    let conn = Database::connect(format!("{url}/{TEMPLATE_NAME}"))
                        .await
                        .expect("connect to template database");
                    migration::Migrator::up(&conn, None)
                        .await
                        .expect("run migrations on template database");
                    // Migrations are DDL only; the system roles and permissions
                    // these tests assert on are written here, exactly as they
                    // are at startup.
                    PgSystemSeedService::new(conn.clone())
                        .seed_system()
                        .await
                        .expect("seed system data on template database");
                    conn.close().await.ok();
                });
        })
        .join()
        .expect("template-db initialisation thread panicked");
    });
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
    pub async fn new(label: &str) -> Self {
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

        let conn = Database::connect(format!("{server_url}/{db_name}"))
            .await
            .expect("connect to per-test database");

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
    conn.execute(Statement::from_string(DbBackend::Postgres, sql.to_owned()))
        .await
        .unwrap_or_else(|e| panic!("admin statement failed ({sql}): {e}"));
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
