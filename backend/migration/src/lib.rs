pub use sea_orm_migration::prelude::*;

pub mod enums;

mod m20260001_000001_create_enums;
mod m20260001_000002_create_users;
mod m20260001_000003_create_user_preferences;
mod m20260001_000004_create_categories;
mod m20260001_000005_create_category_moderators;
mod m20260001_000006_create_threads;
mod m20260001_000007_create_posts;
mod m20260001_000008_create_tags;
mod m20260001_000009_create_reactions;
mod m20260001_000010_create_notifications;
mod m20260001_000011_create_reports;
mod m20260001_000012_create_audit_logs;
mod m20260001_000013_create_site_config;
mod m20260001_000014_create_webhooks;
mod m20260001_000015_create_indexes_and_fts;
mod m20260001_000016_create_bookmarks;
mod m20260001_000017_create_stored_files;
mod m20260001_000018_create_file_links;
mod m20260001_000019_perf_indexes;
mod m20260001_000020_create_rbac;
mod m20260001_000021_drop_legacy_role;
mod m20260001_000022_fix_user_pref_categories;
mod m20260001_000023_fix_reports_integrity;
mod m20260001_000024_counter_triggers;
mod m20260001_000025_fix_permissions_min_trust;
mod m20260001_000026_roles_updated_at;
mod m20260001_000027_slug_length_constraints;
mod m20260001_000028_fts_post_content;
mod m20260001_000029_create_user_covers;
mod m20260002_000030_create_plugins;
mod m20260002_000031_create_plugin_hooks;
mod m20260002_000032_create_plugin_ui_slots;
mod m20260002_000033_create_plugin_logs;
mod m20260002_000034_seed_plugin_permission;
mod m20260002_000035_post_approval;
mod m20260003_000036_create_user_follows;
mod m20260003_000037_user_post_count_trigger;
mod m20260003_000038_add_plugin_id_to_webhooks;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260001_000001_create_enums::Migration),
            Box::new(m20260001_000002_create_users::Migration),
            Box::new(m20260001_000003_create_user_preferences::Migration),
            Box::new(m20260001_000004_create_categories::Migration),
            Box::new(m20260001_000005_create_category_moderators::Migration),
            Box::new(m20260001_000006_create_threads::Migration),
            Box::new(m20260001_000007_create_posts::Migration),
            Box::new(m20260001_000008_create_tags::Migration),
            Box::new(m20260001_000009_create_reactions::Migration),
            Box::new(m20260001_000010_create_notifications::Migration),
            Box::new(m20260001_000011_create_reports::Migration),
            Box::new(m20260001_000012_create_audit_logs::Migration),
            Box::new(m20260001_000013_create_site_config::Migration),
            Box::new(m20260001_000014_create_webhooks::Migration),
            Box::new(m20260001_000015_create_indexes_and_fts::Migration),
            Box::new(m20260001_000016_create_bookmarks::Migration),
            Box::new(m20260001_000017_create_stored_files::Migration),
            Box::new(m20260001_000018_create_file_links::Migration),
            Box::new(m20260001_000019_perf_indexes::Migration),
            Box::new(m20260001_000020_create_rbac::Migration),
            Box::new(m20260001_000021_drop_legacy_role::Migration),
            Box::new(m20260001_000022_fix_user_pref_categories::Migration),
            Box::new(m20260001_000023_fix_reports_integrity::Migration),
            Box::new(m20260001_000024_counter_triggers::Migration),
            Box::new(m20260001_000025_fix_permissions_min_trust::Migration),
            Box::new(m20260001_000026_roles_updated_at::Migration),
            Box::new(m20260001_000027_slug_length_constraints::Migration),
            Box::new(m20260001_000028_fts_post_content::Migration),
            Box::new(m20260001_000029_create_user_covers::Migration),
            Box::new(m20260002_000030_create_plugins::Migration),
            Box::new(m20260002_000031_create_plugin_hooks::Migration),
            Box::new(m20260002_000032_create_plugin_ui_slots::Migration),
            Box::new(m20260002_000033_create_plugin_logs::Migration),
            Box::new(m20260002_000034_seed_plugin_permission::Migration),
            Box::new(m20260002_000035_post_approval::Migration),
            Box::new(m20260003_000036_create_user_follows::Migration),
            Box::new(m20260003_000037_user_post_count_trigger::Migration),
            Box::new(m20260003_000038_add_plugin_id_to_webhooks::Migration),
        ]
    }
}
