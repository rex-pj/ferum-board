use std::collections::{HashMap, HashSet};

use chrono::Utc;

use uuid::Uuid;

use ferum_domain::AuthUser;
use ferum_domain::models::user::TrustLevel;

pub struct AuthUserBuilder {
    inner: AuthUser,
}

impl AuthUserBuilder {
    pub fn member() -> Self {
        Self {
            inner: AuthUser {
                id: Uuid::new_v4(),
                username: "testuser".to_string(),
                display_name: None,
                avatar_url: None,
                trust_level: TrustLevel::Member,
                is_banned: false,
                banned_until: None,
                permissions: HashSet::new(),
                category_permissions: HashMap::new(),
            },
        }
    }

    pub fn admin() -> Self {
        let mut builder = Self::member();
        builder.inner.trust_level = TrustLevel::Leader;
        builder.inner.permissions = [
            "admin.users", "admin.categories", "admin.roles",
            "admin.config", "admin.webhooks", "admin.plugins",
            "admin.ban_permanent", "thread.create", "post.create",
            "thread.pin", "thread.lock", "thread.move",
            "post.delete_any", "thread.delete_any", "thread.edit_any",
            "moderation.warn", "moderation.ban_temp",
            "moderation.view_reports", "moderation.resolve",
            "report.create", "reaction.add", "tag.create",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        builder
    }

    pub fn with_id(mut self, id: Uuid) -> Self {
        self.inner.id = id;
        self
    }

    pub fn with_username(mut self, username: &str) -> Self {
        self.inner.username = username.to_string();
        self
    }

    pub fn with_perm(mut self, perm: &str) -> Self {
        self.inner.permissions.insert(perm.to_string());
        self
    }

    pub fn with_perms(mut self, perms: &[&str]) -> Self {
        for p in perms {
            self.inner.permissions.insert(p.to_string());
        }
        self
    }

    pub fn with_category_perm(mut self, category_id: Uuid, perm: &str) -> Self {
        self.inner
            .category_permissions
            .entry(category_id)
            .or_default()
            .insert(perm.to_string());
        self
    }

    pub fn banned(mut self) -> Self {
        self.inner.is_banned = true;
        self.inner.banned_until = None;
        self
    }

    pub fn with_trust(mut self, level: TrustLevel) -> Self {
        self.inner.trust_level = level;
        self
    }

    pub fn build(self) -> AuthUser {
        self.inner
    }
}

// ─── Domain object builders ───────────────────────────────────────────────────

use ferum_domain::models::bookmark::Bookmark;
use ferum_domain::models::category::{Category, PostPolicy, ViewPolicy};
use ferum_domain::models::post::{Post, PostStatus};
use ferum_domain::models::thread::{Thread, ThreadStatus};

pub fn make_category(id: Uuid) -> Category {
    Category {
        id,
        parent_id: None,
        slug: "general".to_string(),
        name: "General".to_string(),
        description: None,
        position: 0,
        view_policy: ViewPolicy::Public,
        post_policy: PostPolicy::Members,
        color: None,
        created_at: chrono::Utc::now(),
        updated_at: None,
        created_by_id: None,
        updated_by_id: None,
    }
}

pub fn make_thread(id: Uuid, category_id: Uuid, author_id: Uuid) -> Thread {
    Thread {
        id,
        category_id,
        category_slug: "general".to_string(),
        category_name: None,
        author_id,
        author_username: None,
        author_display_name: None,
        author_avatar_url: None,
        title: "Test Thread".to_string(),
        slug: "test-thread".to_string(),
        status: ThreadStatus::Open,
        is_pinned: false,
        is_solved: false,
        best_answer_id: None,
        view_count: 0,
        reply_count: 0,
        last_post_at: None,
        created_at: chrono::Utc::now(),
        updated_at: None,
        deleted_at: None,
        deleted_by_id: None,
        excerpt: None,
        thumbnail_url: None,
        tags: vec![],
    }
}

pub fn make_post(id: Uuid, thread_id: Uuid, author_id: Uuid) -> Post {
    Post {
        id,
        thread_id,
        author_id,
        parent_id: None,
        content_md: "Hello world".to_string(),
        content_html: "<p>Hello world</p>".to_string(),
        status: PostStatus::Published,
        is_deleted: false,
        deleted_at: None,
        deleted_by_id: None,
        edited_at: None,
        edited_by_id: None,
        edit_count: 0,
        created_at: chrono::Utc::now(),
        author_username: None,
        author_display_name: None,
        author_avatar_url: None,
        author_role: None,
        reactions: vec![],
        my_reactions: vec![],
        thread_slug: None,
        thread_title: None,
    }
}

pub fn make_bookmark(user_id: Uuid, thread_id: Uuid) -> Bookmark {
    Bookmark {
        id: Uuid::new_v4(),
        user_id,
        thread_id,
        created_at: chrono::Utc::now(),
    }
}

use ferum_domain::models::report::{Report, ReportStatus};
use ferum_domain::models::role::{Role, UserRoleAssignment};
use ferum_domain::models::user::User;

pub fn make_user(id: Uuid) -> User {
    User {
        id,
        username: "testuser".to_string(),
        email: "test@example.com".to_string(),
        is_email_verified: true,
        display_name: None,
        password_hash: Some("$2b$12$hashedpassword".to_string()),
        trust_level: TrustLevel::Basic,
        primary_role_slug: None,
        trust_score: 0,
        post_count: 0,
        days_visited: 0,
        avatar_url: None,
        cover_url: None,
        bio: None,
        website: None,
        is_banned: false,
        banned_until: None,
        ban_reason: None,
        warn_count: 0,
        failed_login_count: 0,
        locked_until: None,
        created_at: Utc::now(),
        updated_at: None,
        deleted_at: None,
        last_seen_at: None,
    }
}

pub fn make_role(id: Uuid, slug: &str) -> Role {
    Role {
        id,
        slug: slug.to_string(),
        name: slug.to_string(),
        description: None,
        color: None,
        is_system: false,
        is_default: false,
        position: 0,
        created_at: Utc::now(),
    }
}

pub fn make_assignment(user_id: Uuid, role_id: Uuid) -> UserRoleAssignment {
    UserRoleAssignment {
        id: Uuid::new_v4(),
        user_id,
        role_id,
        role_slug: "member".to_string(),
        role_name: "Member".to_string(),
        role_color: None,
        category_id: None,
        granted_by: None,
        expires_at: None,
        created_at: Utc::now(),
    }
}

pub fn make_report(reporter_id: Uuid, post_id: Option<Uuid>, thread_id: Option<Uuid>) -> Report {
    Report {
        id: Uuid::new_v4(),
        reporter_id,
        post_id,
        thread_id,
        reason: "Test reason".to_string(),
        status: ReportStatus::Pending,
        moderator_notes: None,
        resolved_by_id: None,
        resolved_at: None,
        target_deleted_at: None,
        created_at: Utc::now(),
    }
}

/// Fixed UUIDs for use across tests — deterministic, easy to read in failure output.
pub mod ids {
    use uuid::Uuid;

    pub fn user_a() -> Uuid { Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap() }
    pub fn user_b() -> Uuid { Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap() }
    pub fn category_a() -> Uuid { Uuid::parse_str("00000000-0000-0000-0000-000000000010").unwrap() }
    pub fn thread_a() -> Uuid { Uuid::parse_str("00000000-0000-0000-0000-000000000020").unwrap() }
    pub fn post_a() -> Uuid { Uuid::parse_str("00000000-0000-0000-0000-000000000030").unwrap() }
}
