//! Integration-test harness for repository tests.
//!
//! Each [`TestDb`] provisions an isolated, throwaway PostgreSQL database, runs
//! the full migration chain against it, and exposes a `DatabaseConnection`.
//! Because every repository query is built from generated entity columns, these
//! tests are the safety net for the residual `Expr::cust` fragments (enum casts,
//! aggregate `FILTER`, `LATERAL`, …) that the compiler cannot verify.
//!
//! ## Requirements
//! A reachable PostgreSQL whose superuser can `CREATE DATABASE`. The base URL is
//! read from `TEST_DATABASE_URL`, falling back to `DATABASE_URL` (loaded from
//! `backend/.env`). The database segment of that URL is replaced per test.
//!
//! ## Running
//! ```text
//! cargo test -p ferum-infrastructure --features db-tests
//! ```
//! Without the `db-tests` feature these files are not compiled, so the default
//! `cargo test` stays green on machines without a database.

use std::sync::Once;

use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement};

use migration::MigratorTrait;

static INIT_ENV: Once = Once::new();

/// A migrated, isolated database bound to the lifetime of one test.
pub struct TestDb {
    /// Connection to the freshly migrated test database.
    pub conn: DatabaseConnection,
    /// `postgres://…@host:port` without a trailing database segment.
    server_url: String,
    /// Name of the throwaway database (also used to drop it on teardown).
    db_name: String,
}

impl TestDb {
    /// Provision `ferum_test_<label>`: drop any stale copy, create it fresh, and
    /// run every migration. `label` must be unique per test to allow `cargo test`
    /// to run them in parallel.
    pub async fn new(label: &str) -> Self {
        INIT_ENV.call_once(|| {
            // Load backend/.env so DATABASE_URL is available without exporting it.
            let _ = dotenvy::from_filename("../../.env");
            let _ = dotenvy::dotenv();
        });

        let base = std::env::var("TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .expect("TEST_DATABASE_URL or DATABASE_URL must be set for db-tests");

        let (server_url, _existing_db) = base
            .rsplit_once('/')
            .expect("DATABASE_URL must contain a database segment");
        let server_url = server_url.to_string();
        let db_name = format!("ferum_test_{label}");

        // Connect to the maintenance database to manage the test database.
        let admin = Database::connect(format!("{server_url}/postgres"))
            .await
            .expect("connect to maintenance database 'postgres'");
        exec(&admin, &format!("DROP DATABASE IF EXISTS \"{db_name}\" WITH (FORCE)")).await;
        exec(&admin, &format!("CREATE DATABASE \"{db_name}\"")).await;
        admin.close().await.ok();

        let conn = Database::connect(format!("{server_url}/{db_name}"))
            .await
            .expect("connect to fresh test database");
        migration::Migrator::up(&conn, None)
            .await
            .expect("run migrations on test database");

        Self {
            conn,
            server_url,
            db_name,
        }
    }

    /// Drop the throwaway database. Call at the end of a test; if skipped, the
    /// next run with the same label reclaims the name via `DROP … IF EXISTS`.
    pub async fn teardown(self) {
        let TestDb {
            conn,
            server_url,
            db_name,
        } = self;
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
