use sea_orm::Statement;
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

        // ── 2. Seed system roles ──────────────────────────────────────────────
        manager
            .exec_stmt(
                Query::insert()
                    .into_table(Roles::Table)
                    .columns([
                        Roles::Slug,
                        Roles::Name,
                        Roles::Description,
                        Roles::Color,
                        Roles::IsSystem,
                        Roles::IsDefault,
                        Roles::Position,
                    ])
                    .values_panic([
                        "admin".into(),
                        "Administrator".into(),
                        "Full system access".into(),
                        "#dc3545".into(),
                        true.into(),
                        false.into(),
                        0i32.into(),
                    ])
                    .values_panic([
                        "moderator".into(),
                        "Moderator".into(),
                        "Category-scoped moderation permissions".into(),
                        "#fd7e14".into(),
                        true.into(),
                        false.into(),
                        1i32.into(),
                    ])
                    .values_panic([
                        "member".into(),
                        "Member".into(),
                        "Default member role".into(),
                        "#0d6efd".into(),
                        true.into(),
                        true.into(),
                        2i32.into(),
                    ])
                    .to_owned(),
            )
            .await?;

        // ── 3. Seed permissions ───────────────────────────────────────────────
        let perm_rows: &[(&str, &str, &str, &str)] = &[
            // Content
            ("thread.create",     "Create new threads",                   "content",    "basic"),
            ("thread.edit_own",   "Edit own threads (within 24h)",        "content",    "new"),
            ("thread.delete_own", "Delete own threads",                   "content",    "new"),
            ("thread.edit_any",   "Edit any thread title",                "content",    "new"),
            ("thread.delete_any", "Delete any thread",                    "content",    "new"),
            ("thread.pin",        "Pin / unpin threads",                  "content",    "new"),
            ("thread.lock",       "Lock / unlock threads",                "content",    "new"),
            ("thread.move",       "Move threads to another category",     "content",    "new"),
            ("post.create",       "Post replies",                         "content",    "basic"),
            ("post.edit_own",     "Edit own posts (within 24h)",          "content",    "new"),
            ("post.delete_own",   "Delete own posts",                     "content",    "new"),
            ("post.delete_any",   "Delete any post",                      "content",    "new"),
            ("reaction.add",      "Add reactions to posts",               "content",    "basic"),
            ("file.upload",       "Upload files and images",              "content",    "member"),
            ("link.embed",        "Embed links in posts",                 "content",    "member"),
            ("tag.create",        "Create new tags",                      "content",    "member"),
            // Moderation
            ("report.create",           "Report posts / threads",         "moderation", "new"),
            ("moderation.view_reports", "View report queue",              "moderation", "new"),
            ("moderation.resolve",      "Resolve or dismiss reports",     "moderation", "new"),
            ("moderation.warn",         "Warn users",                     "moderation", "new"),
            ("moderation.ban_temp",     "Temporarily ban users",          "moderation", "new"),
            // Admin
            ("admin.users",         "Manage users and role assignments",  "admin", "new"),
            ("admin.ban_permanent", "Permanently ban users",              "admin", "new"),
            ("admin.categories",    "Create / edit / delete categories",  "admin", "new"),
            ("admin.roles",         "Manage roles and permissions",       "admin", "new"),
            ("admin.config",        "Manage site configuration",          "admin", "new"),
            ("admin.webhooks",      "Manage webhooks",                    "admin", "new"),
            ("admin.plugins",       "Install and manage plugins",         "admin", "new"),
        ];

        // min_trust is a PostgreSQL enum; parameterized bind values arrive as
        // text and PostgreSQL rejects implicit text→enum casts.
        let value_clauses: Vec<String> = perm_rows
            .iter()
            .map(|(key, desc, group, trust)| {
                format!(
                    "('{}', '{}', '{}', '{}'::trust_level)",
                    key.replace('\'', "''"),
                    desc.replace('\'', "''"),
                    group.replace('\'', "''"),
                    trust,
                )
            })
            .collect();
        let insert_sql = format!(
            "INSERT INTO permissions (key, description, group_name, min_trust) VALUES {} ON CONFLICT (key) DO NOTHING",
            value_clauses.join(", ")
        );
        conn.execute(Statement::from_string(
            manager.get_database_backend(),
            insert_sql,
        ))
        .await?;

        // ── 4. Seed default role_permissions via INSERT...SELECT ──────────────
        // Admin gets all permissions
        let mut admin_rp = Query::insert()
            .into_table(RolePermissions::Table)
            .columns([RolePermissions::RoleId, RolePermissions::PermissionId])
            .to_owned();
        admin_rp
            .select_from(
                Query::select()
                    .column((Roles::Table, Roles::Id))
                    .column((Permissions::Table, Permissions::Id))
                    .from(Roles::Table)
                    .from(Permissions::Table)
                    .and_where(Expr::col((Roles::Table, Roles::Slug)).eq("admin"))
                    .to_owned(),
            )
            .map_err(|e| DbErr::Custom(e.to_string()))?;
        manager.exec_stmt(admin_rp).await?;

        let mod_perms = [
            "thread.create", "thread.edit_own", "thread.delete_own",
            "thread.edit_any", "thread.delete_any",
            "thread.pin", "thread.lock", "thread.move",
            "post.create", "post.edit_own", "post.delete_own", "post.delete_any",
            "reaction.add", "file.upload", "link.embed", "tag.create",
            "report.create",
            "moderation.view_reports", "moderation.resolve",
            "moderation.warn", "moderation.ban_temp",
        ];
        let mut mod_rp = Query::insert()
            .into_table(RolePermissions::Table)
            .columns([RolePermissions::RoleId, RolePermissions::PermissionId])
            .to_owned();
        mod_rp
            .select_from(
                Query::select()
                    .column((Roles::Table, Roles::Id))
                    .column((Permissions::Table, Permissions::Id))
                    .from(Roles::Table)
                    .join(
                        JoinType::InnerJoin,
                        Permissions::Table,
                        Expr::col((Permissions::Table, Permissions::Key)).is_in(mod_perms),
                    )
                    .and_where(Expr::col((Roles::Table, Roles::Slug)).eq("moderator"))
                    .to_owned(),
            )
            .map_err(|e| DbErr::Custom(e.to_string()))?;
        manager.exec_stmt(mod_rp).await?;

        let member_perms = [
            "thread.create", "thread.edit_own", "thread.delete_own",
            "post.create", "post.edit_own", "post.delete_own",
            "reaction.add", "file.upload", "link.embed", "tag.create",
            "report.create",
        ];
        let mut member_rp = Query::insert()
            .into_table(RolePermissions::Table)
            .columns([RolePermissions::RoleId, RolePermissions::PermissionId])
            .to_owned();
        member_rp
            .select_from(
                Query::select()
                    .column((Roles::Table, Roles::Id))
                    .column((Permissions::Table, Permissions::Id))
                    .from(Roles::Table)
                    .join(
                        JoinType::InnerJoin,
                        Permissions::Table,
                        Expr::col((Permissions::Table, Permissions::Key)).is_in(member_perms),
                    )
                    .and_where(Expr::col((Roles::Table, Roles::Slug)).eq("member"))
                    .to_owned(),
            )
            .map_err(|e| DbErr::Custom(e.to_string()))?;
        manager.exec_stmt(member_rp).await?;

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
