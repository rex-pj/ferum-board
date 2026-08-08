pub use sea_orm_migration::prelude::*;

pub mod enums;

// Migrations are DDL only. Every row the application needs in order to run —
// system roles, permissions and their default grants, site_config defaults, the
// built-in theme, the catalogue taxonomy — is written by
// `ferum-infrastructure/src/system_seed_service.rs`, which runs after these on
// every startup. Seeding from code keeps a permission's definition next to the
// `perm::` constant the code checks against; seeding from SQL did not.

mod m20260001_000001_bootstrap_enums_and_functions;
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
mod m20260001_000020_create_plugin_storage;
mod m20260001_000021_create_furniture_enums;
mod m20260001_000022_create_brands;
mod m20260001_000023_create_materials;
mod m20260001_000024_create_product_categories;
mod m20260001_000025_create_products;
mod m20260001_000026_create_product_materials;
mod m20260001_000027_create_product_media;
mod m20260001_000028_add_product_to_threads;
mod m20260001_000029_create_review_ratings;
mod m20260001_000030_create_product_rating_stats;
mod m20260001_000031_create_product_search;
mod m20260001_000032_index_users_username_lower;
/// Re-exported so `ferum-infrastructure` names the same role it drops privileges
/// to, without a second copy of the literal that could drift out of step with
/// the migration that creates it.
pub use m20260001_000015_create_plugins::PLUGIN_DB_ROLE;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260001_000001_bootstrap_enums_and_functions::Migration),
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
            Box::new(m20260001_000020_create_plugin_storage::Migration),
            Box::new(m20260001_000021_create_furniture_enums::Migration),
            Box::new(m20260001_000022_create_brands::Migration),
            Box::new(m20260001_000023_create_materials::Migration),
            Box::new(m20260001_000024_create_product_categories::Migration),
            Box::new(m20260001_000025_create_products::Migration),
            Box::new(m20260001_000026_create_product_materials::Migration),
            Box::new(m20260001_000027_create_product_media::Migration),
            Box::new(m20260001_000028_add_product_to_threads::Migration),
            Box::new(m20260001_000029_create_review_ratings::Migration),
            Box::new(m20260001_000030_create_product_rating_stats::Migration),
            Box::new(m20260001_000031_create_product_search::Migration),
            Box::new(m20260001_000032_index_users_username_lower::Migration),
        ]
    }
}
