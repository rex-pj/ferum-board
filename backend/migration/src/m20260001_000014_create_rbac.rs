use sea_orm_migration::prelude::*;

use crate::enums::trust_level::TrustLevelEnum;
use crate::m20260001_000002_create_users::Users;
use crate::m20260001_000004_create_categories::Categories;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000014_create_rbac"
    }
}

#[derive(Iden)]
pub enum Roles {
    Table,
    Id,
    Slug,
    Name,
    Description,
    Color,
    IsSystem,
    IsDefault,
    Position,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
pub enum Permissions {
    Table,
    Id,
    Key,
    Description,
    GroupName,
    MinTrust,
}

#[derive(Iden)]
pub enum RolePermissions {
    Table,
    RoleId,
    PermissionId,
}

#[derive(Iden)]
pub enum UserRoles {
    Table,
    Id,
    UserId,
    RoleId,
    CategoryId,
    GrantedBy,
    ExpiresAt,
    CreatedAt,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── 1. Create tables ──────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Roles::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Roles::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(Roles::Slug).text().not_null().unique_key())
                    .col(ColumnDef::new(Roles::Name).text().not_null())
                    .col(ColumnDef::new(Roles::Description).text().null())
                    .col(ColumnDef::new(Roles::Color).text().null())
                    .col(
                        ColumnDef::new(Roles::IsSystem)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Roles::IsDefault)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Roles::Position)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Roles::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(
                        ColumnDef::new(Roles::UpdatedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Permissions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Permissions::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(Permissions::Key).text().not_null().unique_key())
                    .col(ColumnDef::new(Permissions::Description).text().not_null())
                    .col(ColumnDef::new(Permissions::GroupName).text().not_null())
                    .col(
                        ColumnDef::new(Permissions::MinTrust)
                            .custom(TrustLevelEnum::Type)
                            .not_null()
                            .extra("DEFAULT 'new'"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(RolePermissions::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(RolePermissions::RoleId).uuid().not_null())
                    .col(ColumnDef::new(RolePermissions::PermissionId).uuid().not_null())
                    .primary_key(
                        Index::create()
                            .col(RolePermissions::RoleId)
                            .col(RolePermissions::PermissionId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_role_permissions_role_id")
                            .from(RolePermissions::Table, RolePermissions::RoleId)
                            .to(Roles::Table, Roles::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_role_permissions_permission_id")
                            .from(RolePermissions::Table, RolePermissions::PermissionId)
                            .to(Permissions::Table, Permissions::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(UserRoles::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserRoles::Id)
                            .uuid()
                            .not_null()
                            .primary_key()
                            .extra("DEFAULT gen_random_uuid()"),
                    )
                    .col(ColumnDef::new(UserRoles::UserId).uuid().not_null())
                    .col(ColumnDef::new(UserRoles::RoleId).uuid().not_null())
                    .col(ColumnDef::new(UserRoles::CategoryId).uuid().null())
                    .col(ColumnDef::new(UserRoles::GrantedBy).uuid().null())
                    .col(
                        ColumnDef::new(UserRoles::ExpiresAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(UserRoles::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .extra("DEFAULT now()"),
                    )
                    .col(
                        ColumnDef::new(UserRoles::UpdatedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_roles_user_id")
                            .from(UserRoles::Table, UserRoles::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_roles_role_id")
                            .from(UserRoles::Table, UserRoles::RoleId)
                            .to(Roles::Table, Roles::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_roles_category_id")
                            .from(UserRoles::Table, UserRoles::CategoryId)
                            .to(Categories::Table, Categories::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_roles_granted_by")
                            .from(UserRoles::Table, UserRoles::GrantedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        let conn = manager.get_connection();

        conn.execute_unprepared(
            "CREATE UNIQUE INDEX uq_user_roles_global ON user_roles(user_id, role_id) WHERE category_id IS NULL; \
             CREATE UNIQUE INDEX uq_user_roles_scoped ON user_roles(user_id, role_id, category_id) WHERE category_id IS NOT NULL; \
             CREATE INDEX idx_user_roles_cat ON user_roles(category_id) WHERE category_id IS NOT NULL;",
        )
        .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_user_roles_user")
                    .table(UserRoles::Table)
                    .col(UserRoles::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_user_roles_role")
                    .table(UserRoles::Table)
                    .col(UserRoles::RoleId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserRoles::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(RolePermissions::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(Permissions::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(Roles::Table).if_exists().to_owned())
            .await?;
        Ok(())
    }
}
