pub use sea_orm_migration::prelude::*;

pub mod enums;

mod m20260001_000001_create_enums;
mod m20260001_000002_create_users;
mod m20260001_000003_create_user_preferences;
mod m20260001_000004_create_categories;
mod m20260001_000005_create_threads;
mod m20260001_000006_create_posts;
mod m20260001_000007_create_tags;
mod m20260001_000008_create_reactions;
mod m20260001_000009_create_notifications;
mod m20260001_000010_create_reports;
mod m20260001_000011_create_audit_logs;
mod m20260001_000012_create_site_config;
mod m20260001_000013_create_stored_files;
mod m20260001_000014_create_rbac;
mod m20260001_000015_create_plugins;
mod m20260001_000016_create_webhooks;
mod m20260001_000017_create_themes;
mod m20260001_000018_create_daily_stats;
mod m20260001_000019_create_bookmarks;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260001_000001_create_enums::Migration),
            Box::new(m20260001_000002_create_users::Migration),
            Box::new(m20260001_000003_create_user_preferences::Migration),
            Box::new(m20260001_000004_create_categories::Migration),
            Box::new(m20260001_000005_create_threads::Migration),
            Box::new(m20260001_000006_create_posts::Migration),
            Box::new(m20260001_000007_create_tags::Migration),
            Box::new(m20260001_000008_create_reactions::Migration),
            Box::new(m20260001_000009_create_notifications::Migration),
            Box::new(m20260001_000010_create_reports::Migration),
            Box::new(m20260001_000011_create_audit_logs::Migration),
            Box::new(m20260001_000012_create_site_config::Migration),
            Box::new(m20260001_000013_create_stored_files::Migration),
            Box::new(m20260001_000014_create_rbac::Migration),
            Box::new(m20260001_000015_create_plugins::Migration),
            Box::new(m20260001_000016_create_webhooks::Migration),
            Box::new(m20260001_000017_create_themes::Migration),
            Box::new(m20260001_000018_create_daily_stats::Migration),
            Box::new(m20260001_000019_create_bookmarks::Migration),
        ]
    }
}
