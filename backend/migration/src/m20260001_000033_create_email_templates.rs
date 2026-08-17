use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000033_create_email_templates"
    }
}

// Transactional email copy, replacing `locales/*/emails.ftl`.
//
// The primary key is (key, locale), not key: one row per language is what keeps
// the feature multilingual at all. Lookup walks `Locale::fallback_chain`, so a
// locale with no row of its own inherits from its base and finally from the
// default — the same resolution Fluent used to provide.
//
// Subject and body live in one row on purpose. Split across rows they would fall
// back independently, and an admin who translated only the subject would send a
// Vietnamese subject with an English body.
//
// Seeded by `PgSystemSeedService` from `ferum_domain::models::email_template`,
// insert-only so a boot never overwrites edited copy. No seed data here —
// migrations are DDL.
#[derive(Iden)]
enum EmailTemplates {
    Table,
    Key,
    Locale,
    Subject,
    BodyHtml,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(EmailTemplates::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(EmailTemplates::Key).text().not_null())
                    // Canonical form as produced by `Locale::parse` (`vi`, not
                    // `vi-VN` typed by hand) — a non-canonical value here matches
                    // no recipient and the row is silently never used.
                    .col(ColumnDef::new(EmailTemplates::Locale).text().not_null())
                    .col(ColumnDef::new(EmailTemplates::Subject).text().not_null())
                    .col(ColumnDef::new(EmailTemplates::BodyHtml).text().not_null())
                    .col(
                        ColumnDef::new(EmailTemplates::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .primary_key(
                        Index::create()
                            .col(EmailTemplates::Key)
                            .col(EmailTemplates::Locale),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(EmailTemplates::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
