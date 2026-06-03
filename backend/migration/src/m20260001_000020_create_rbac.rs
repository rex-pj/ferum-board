use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260001_000020_create_rbac"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // ── 1. Create tables ─────────────────────────────────────────────────
        conn.execute_unprepared(r#"
            CREATE TABLE roles (
                id          UUID        NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
                slug        TEXT        NOT NULL UNIQUE,
                name        TEXT        NOT NULL,
                description TEXT,
                color       TEXT,
                is_system   BOOLEAN     NOT NULL DEFAULT false,
                is_default  BOOLEAN     NOT NULL DEFAULT false,
                position    INT         NOT NULL DEFAULT 0,
                created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
            );

            CREATE TABLE permissions (
                id          UUID    NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
                key         TEXT    NOT NULL UNIQUE,
                description TEXT    NOT NULL,
                group_name  TEXT    NOT NULL,
                min_trust   TEXT    NOT NULL DEFAULT 'new'
            );

            CREATE TABLE role_permissions (
                role_id       UUID NOT NULL REFERENCES roles(id)       ON DELETE CASCADE,
                permission_id UUID NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
                PRIMARY KEY (role_id, permission_id)
            );

            CREATE TABLE user_roles (
                id          UUID        NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
                user_id     UUID        NOT NULL REFERENCES users(id)      ON DELETE CASCADE,
                role_id     UUID        NOT NULL REFERENCES roles(id)      ON DELETE CASCADE,
                category_id UUID                 REFERENCES categories(id) ON DELETE CASCADE,
                granted_by  UUID                 REFERENCES users(id)      ON DELETE SET NULL,
                expires_at  TIMESTAMPTZ,
                created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
            );

            -- NULL-safe unique: one global role per user, one scoped role per user+category
            CREATE UNIQUE INDEX uq_user_roles_global
                ON user_roles(user_id, role_id)
                WHERE category_id IS NULL;

            CREATE UNIQUE INDEX uq_user_roles_scoped
                ON user_roles(user_id, role_id, category_id)
                WHERE category_id IS NOT NULL;

            CREATE INDEX idx_user_roles_user   ON user_roles(user_id);
            CREATE INDEX idx_user_roles_role   ON user_roles(role_id);
            CREATE INDEX idx_user_roles_cat    ON user_roles(category_id) WHERE category_id IS NOT NULL;
        "#).await?;

        // ── 2. Seed system roles ─────────────────────────────────────────────
        conn.execute_unprepared(r#"
            INSERT INTO roles (slug, name, description, color, is_system, is_default, position)
            VALUES
                ('admin',     'Administrator', 'Full system access',                     '#dc3545', true,  false, 0),
                ('moderator', 'Moderator',     'Category-scoped moderation permissions', '#fd7e14', true,  false, 1),
                ('member',    'Member',        'Default member role',                    '#0d6efd', true,  true,  2);
        "#).await?;

        // ── 3. Seed permissions ──────────────────────────────────────────────
        conn.execute_unprepared(r#"
            INSERT INTO permissions (key, description, group_name, min_trust) VALUES
                -- Content
                ('thread.create',      'Create new threads',                     'content',    'basic'),
                ('thread.edit_own',    'Edit own threads (within 24h)',          'content',    'new'),
                ('thread.delete_own',  'Delete own threads',                     'content',    'new'),
                ('thread.edit_any',    'Edit any thread title',                  'content',    'new'),
                ('thread.delete_any',  'Delete any thread',                      'content',    'new'),
                ('thread.pin',         'Pin / unpin threads',                    'content',    'new'),
                ('thread.lock',        'Lock / unlock threads',                  'content',    'new'),
                ('thread.move',        'Move threads to another category',       'content',    'new'),
                ('post.create',        'Post replies',                           'content',    'basic'),
                ('post.edit_own',      'Edit own posts (within 24h)',            'content',    'new'),
                ('post.delete_own',    'Delete own posts',                       'content',    'new'),
                ('post.delete_any',    'Delete any post',                        'content',    'new'),
                ('reaction.add',       'Add reactions to posts',                 'content',    'basic'),
                ('file.upload',        'Upload files and images',                'content',    'member'),
                ('link.embed',         'Embed links in posts',                   'content',    'member'),
                ('tag.create',         'Create new tags',                        'content',    'member'),
                -- Moderation
                ('report.create',            'Report posts / threads',           'moderation', 'new'),
                ('moderation.view_reports',  'View report queue',                'moderation', 'new'),
                ('moderation.resolve',       'Resolve or dismiss reports',       'moderation', 'new'),
                ('moderation.warn',          'Warn users',                       'moderation', 'new'),
                ('moderation.ban_temp',      'Temporarily ban users',            'moderation', 'new'),
                -- Admin
                ('admin.users',          'Manage users and role assignments',    'admin',      'new'),
                ('admin.ban_permanent',  'Permanently ban users',                'admin',      'new'),
                ('admin.categories',     'Create / edit / delete categories',    'admin',      'new'),
                ('admin.roles',          'Manage roles and permissions',         'admin',      'new'),
                ('admin.config',         'Manage site configuration',            'admin',      'new'),
                ('admin.webhooks',       'Manage webhooks',                      'admin',      'new');
        "#).await?;

        // ── 4. Seed default role_permissions ────────────────────────────────
        conn.execute_unprepared(r#"
            -- admin gets everything
            INSERT INTO role_permissions (role_id, permission_id)
            SELECT r.id, p.id
            FROM roles r, permissions p
            WHERE r.slug = 'admin';

            -- moderator permissions
            INSERT INTO role_permissions (role_id, permission_id)
            SELECT r.id, p.id
            FROM roles r
            JOIN permissions p ON p.key IN (
                'thread.create', 'thread.edit_own', 'thread.delete_own',
                'thread.edit_any', 'thread.delete_any',
                'thread.pin', 'thread.lock', 'thread.move',
                'post.create', 'post.edit_own', 'post.delete_own', 'post.delete_any',
                'reaction.add', 'file.upload', 'link.embed', 'tag.create',
                'report.create',
                'moderation.view_reports', 'moderation.resolve',
                'moderation.warn', 'moderation.ban_temp'
            )
            WHERE r.slug = 'moderator';

            -- member permissions
            INSERT INTO role_permissions (role_id, permission_id)
            SELECT r.id, p.id
            FROM roles r
            JOIN permissions p ON p.key IN (
                'thread.create', 'thread.edit_own', 'thread.delete_own',
                'post.create', 'post.edit_own', 'post.delete_own',
                'reaction.add', 'file.upload', 'link.embed', 'tag.create',
                'report.create'
            )
            WHERE r.slug = 'member';
        "#).await?;

        // ── 5. Migrate existing users → user_roles ───────────────────────────
        // Map users.role ENUM values to the new roles table slugs
        conn.execute_unprepared(r#"
            INSERT INTO user_roles (user_id, role_id, category_id, granted_by, created_at)
            SELECT
                u.id,
                r.id,
                NULL,
                u.id,
                u.created_at
            FROM users u
            JOIN roles r ON r.slug = u.role::TEXT;
        "#).await?;

        // ── 6. Migrate category_moderators → user_roles (category-scoped) ───
        conn.execute_unprepared(r#"
            INSERT INTO user_roles (user_id, role_id, category_id, granted_by, created_at)
            SELECT
                cm.user_id,
                r.id,
                cm.category_id,
                COALESCE(cm.assigned_by_id, cm.user_id),
                cm.assigned_at
            FROM category_moderators cm
            JOIN roles r ON r.slug = 'moderator'
            ON CONFLICT DO NOTHING;
        "#).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(r#"
                DROP TABLE IF EXISTS user_roles;
                DROP TABLE IF EXISTS role_permissions;
                DROP TABLE IF EXISTS permissions;
                DROP TABLE IF EXISTS roles;
            "#)
            .await?;
        Ok(())
    }
}
