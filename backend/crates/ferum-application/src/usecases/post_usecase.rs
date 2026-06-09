use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use regex::Regex;
use uuid::Uuid;

use crate::constants::{MAX_POSTS_PER_PAGE, MAX_POST_CONTENT_BYTES};
use crate::event_bus::EventBus;
use crate::permission::PermissionChecker;
use crate::ports::{HookContext, HookDecision, PluginRuntime};
use crate::shared::AppError;
use ferum_domain::events::ForumEvent;
use ferum_domain::models::category::PostPolicy;
use ferum_domain::models::post::{Post, PostStatus};
use ferum_domain::models::thread::ThreadStatus;
use ferum_domain::models::user::TrustLevel;
use ferum_domain::repositories::category_repository::CategoryRepository;
use ferum_domain::repositories::post_repository::{NewPost, PostRepository};
use ferum_domain::repositories::reaction_repository::ReactionRepository;
use ferum_domain::repositories::site_config_repository::SiteConfigRepository;
use ferum_domain::repositories::thread_repository::ThreadRepository;
use ferum_domain::repositories::user_repository::UserRepository;
use ferum_domain::AuthUser;

pub struct PostUseCase {
    pub posts: Arc<dyn PostRepository>,
    pub threads: Arc<dyn ThreadRepository>,
    pub categories: Arc<dyn CategoryRepository>,
    pub users: Arc<dyn UserRepository>,
    pub reactions: Arc<dyn ReactionRepository>,
    pub site_config: Arc<dyn SiteConfigRepository>,
    pub event_bus: Arc<EventBus>,
    pub plugin_runtime: Arc<dyn PluginRuntime>,
}

impl PostUseCase {
    pub fn new(
        posts: Arc<dyn PostRepository>,
        threads: Arc<dyn ThreadRepository>,
        categories: Arc<dyn CategoryRepository>,
        users: Arc<dyn UserRepository>,
        reactions: Arc<dyn ReactionRepository>,
        site_config: Arc<dyn SiteConfigRepository>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self {
            posts,
            threads,
            categories,
            users,
            reactions,
            site_config,
            event_bus,
            plugin_runtime: Arc::new(crate::ports::NullPluginRuntime),
        }
    }

    pub fn with_plugin_runtime(mut self, runtime: Arc<dyn PluginRuntime>) -> Self {
        self.plugin_runtime = runtime;
        self
    }

    pub async fn list_by_thread(
        &self,
        actor: Option<&AuthUser>,
        thread_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Post>, u64), AppError> {
        let thread = self
            .threads
            .find_by_id(thread_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if thread.deleted_at.is_some() {
            return Err(AppError::NotFound);
        }

        let category = self
            .categories
            .find_by_id(thread.category_id)
            .await?
            .ok_or(AppError::NotFound)?;
        PermissionChecker::can_view_category(actor, &category)?;

        let per_page = per_page.min(MAX_POSTS_PER_PAGE);
        let (mut posts, total) = self.posts.list_by_thread(thread_id, page, per_page).await?;

        if posts.is_empty() {
            return Ok((posts, total));
        }

        let post_ids: Vec<Uuid> = posts.iter().map(|p| p.id).collect();
        let author_ids: Vec<Uuid> = posts
            .iter()
            .map(|p| p.author_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        let (users, reaction_counts, my_reactions) = tokio::try_join!(
            self.users.find_many_by_ids(&author_ids),
            self.reactions.counts_by_posts(&post_ids),
            async {
                match actor {
                    Some(a) => {
                        self.reactions
                            .user_reactions_for_posts(a.id, &post_ids)
                            .await
                    }
                    None => Ok(HashMap::new()),
                }
            },
        )?;

        let user_map: HashMap<Uuid, _> = users.into_iter().map(|u| (u.id, u)).collect();

        for post in posts.iter_mut() {
            if let Some(user) = user_map.get(&post.author_id) {
                post.author_username = Some(user.username.clone());
                post.author_display_name = user.display_name.clone();
                post.author_avatar_url = user.avatar_url.clone();
                post.author_role = user.primary_role_slug.clone();
            }
            post.reactions = reaction_counts.get(&post.id).cloned().unwrap_or_default();
            post.my_reactions = my_reactions.get(&post.id).cloned().unwrap_or_default();
        }

        Ok((posts, total))
    }

    pub async fn create(&self, actor: &AuthUser, cmd: CreatePostCmd) -> Result<Post, AppError> {
        let thread = self
            .threads
            .find_by_id(cmd.thread_id)
            .await?
            .ok_or(AppError::NotFound)?;

        if thread.deleted_at.is_some() {
            return Err(AppError::NotFound);
        }
        if thread.status == ThreadStatus::Locked {
            return Err(AppError::forbidden("thread_locked"));
        }

        let category = self
            .categories
            .find_by_id(thread.category_id)
            .await?
            .ok_or(AppError::NotFound)?;
        PermissionChecker::can_create_post(actor, &category)?;

        // Before-hook: allow plugins to inspect or block post creation
        let hook_ctx = HookContext {
            hook_name: "before_post_create".to_string(),
            actor_id: Some(actor.id),
            actor_trust_level: format!("{:?}", actor.trust_level).to_lowercase(),
            payload: serde_json::json!({
                "content_md": cmd.content_md,
                "thread_id": cmd.thread_id,
                "parent_id": cmd.parent_id,
                "category_id": category.id,
            }),
        };
        match self
            .plugin_runtime
            .dispatch_before_hook("before_post_create", &hook_ctx)
            .await?
        {
            HookDecision::Deny { reason, error_code } => {
                return Err(AppError::PluginBlocked { reason, error_code });
            }
            HookDecision::Allow => {}
        }

        if cmd.content_md.len() > MAX_POST_CONTENT_BYTES {
            return Err(AppError::unprocessable("Post content exceeds 100 KB limit"));
        }

        if let Some(parent_id) = cmd.parent_id {
            let parent = self
                .posts
                .find_by_id(parent_id)
                .await?
                .ok_or(AppError::NotFound)?;
            if parent.thread_id != cmd.thread_id {
                return Err(AppError::unprocessable(
                    "parent_id does not belong to this thread",
                ));
            }
        }

        let content_html = render_content(&cmd.content_md).await?;
        let mentions = extract_mentions(&cmd.content_md);

        // Determine whether this post needs moderation approval
        let status = resolve_post_status(actor, &category, &self.site_config).await?;
        let needs_approval = status.is_pending();

        let post = self
            .posts
            .create(NewPost {
                thread_id: cmd.thread_id,
                author_id: actor.id,
                parent_id: cmd.parent_id,
                content_md: cmd.content_md,
                content_html,
                status,
            })
            .await?;

        if needs_approval {
            // Pending posts do not count toward reply stats or fire events yet
            return Ok(post);
        }

        self.threads
            .update_reply_stats(cmd.thread_id, 1, chrono::Utc::now())
            .await
            .ok();

        self.event_bus
            .publish(ForumEvent::PostCreated {
                post_id: post.id,
                thread_id: cmd.thread_id,
                thread_slug: thread.slug.clone(),
                author_id: actor.id,
                thread_author_id: thread.author_id,
                category_id: thread.category_id,
            })
            .await;

        for username in mentions.into_iter().take(5) {
            if let Ok(Some(mentioned)) = self.users.find_by_username(&username).await {
                if mentioned.id != actor.id {
                    self.event_bus
                        .publish(ForumEvent::MentionAdded {
                            post_id: post.id,
                            thread_id: cmd.thread_id,
                            thread_slug: thread.slug.clone(),
                            mentioned_user_id: mentioned.id,
                            author_id: actor.id,
                        })
                        .await;
                }
            }
        }

        Ok(post)
    }

    pub async fn edit(
        &self,
        actor: &AuthUser,
        id: Uuid,
        content_md: String,
    ) -> Result<Post, AppError> {
        if content_md.len() > MAX_POST_CONTENT_BYTES {
            return Err(AppError::unprocessable("Post content exceeds 100 KB limit"));
        }
        let post = self.posts.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        let thread = self
            .threads
            .find_by_id(post.thread_id)
            .await?
            .ok_or(AppError::NotFound)?;
        PermissionChecker::can_edit_post(actor, &post, thread.category_id)?;

        let hook_ctx = crate::ports::HookContext {
            hook_name: "before_post_edit".to_string(),
            actor_id: Some(actor.id),
            actor_trust_level: format!("{:?}", actor.trust_level).to_lowercase(),
            payload: serde_json::json!({
                "post_id": id,
                "content_md": content_md,
                "thread_id": post.thread_id,
                "category_id": thread.category_id,
            }),
        };
        match self
            .plugin_runtime
            .dispatch_before_hook("before_post_edit", &hook_ctx)
            .await?
        {
            HookDecision::Deny { reason, error_code } => {
                return Err(AppError::PluginBlocked { reason, error_code });
            }
            HookDecision::Allow => {}
        }

        let content_html = render_content(&content_md).await?;
        self.posts
            .update_content(id, content_md, content_html, actor.id)
            .await
    }

    pub async fn delete(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        let post = self.posts.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        let thread = self
            .threads
            .find_by_id(post.thread_id)
            .await?
            .ok_or(AppError::NotFound)?;
        PermissionChecker::can_delete_post(actor, &post, thread.category_id)?;
        self.posts.soft_delete(id, actor.id).await?;

        // Pending posts were never counted in reply_count (create() returns early
        // before calling update_reply_stats when needs_approval is true).
        if !post.status.is_pending() {
            self.threads
                .update_reply_stats(post.thread_id, -1, chrono::Utc::now())
                .await
                .ok();
        }

        Ok(())
    }

    // ── Approval queue ────────────────────────────────────────────────────────

    pub async fn list_pending(
        &self,
        actor: &AuthUser,
        category_id: Option<Uuid>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Post>, u64), AppError> {
        if !actor.has_perm("moderation.view_reports") {
            return Err(AppError::forbidden("permission_denied"));
        }
        let per_page = per_page.min(MAX_POSTS_PER_PAGE);
        self.posts.list_pending(category_id, page, per_page).await
    }

    pub async fn list_by_author(
        &self,
        author_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Post>, u64), AppError> {
        self.posts.list_by_author(author_id, page, per_page).await
    }

    pub async fn approve_post(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        let post = self.posts.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        if !post.status.is_pending() {
            return Err(AppError::unprocessable("Post is not pending approval"));
        }
        let thread = self
            .threads
            .find_by_id(post.thread_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if !actor.has_perm("moderation.view_reports")
            && !actor.has_perm_in("moderation.view_reports", thread.category_id)
        {
            return Err(AppError::forbidden("permission_denied"));
        }
        self.posts.set_status(id, PostStatus::Published).await?;
        self.threads
            .update_reply_stats(post.thread_id, 1, chrono::Utc::now())
            .await
            .ok();
        self.event_bus
            .publish(ForumEvent::PostCreated {
                post_id: post.id,
                thread_id: post.thread_id,
                thread_slug: thread.slug,
                author_id: post.author_id,
                thread_author_id: thread.author_id,
                category_id: thread.category_id,
            })
            .await;
        Ok(())
    }

    pub async fn reject_post(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        let post = self.posts.find_by_id(id).await?.ok_or(AppError::NotFound)?;
        if !post.status.is_pending() {
            return Err(AppError::unprocessable("Post is not pending approval"));
        }
        let thread = self
            .threads
            .find_by_id(post.thread_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if !actor.has_perm("moderation.view_reports")
            && !actor.has_perm_in("moderation.view_reports", thread.category_id)
        {
            return Err(AppError::forbidden("permission_denied"));
        }
        self.posts.soft_delete(id, actor.id).await?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct CreatePostCmd {
    pub thread_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub content_md: String,
}

async fn resolve_post_status(
    actor: &AuthUser,
    category: &ferum_domain::models::category::Category,
    site_config: &Arc<dyn SiteConfigRepository>,
) -> Result<PostStatus, AppError> {
    use ferum_domain::models::role::perm;

    // Staff (admin or category moderator) always bypass the approval queue
    if actor.has_perm(perm::ADMIN_USERS) || actor.has_perm_in(perm::MOD_WARN, category.id) {
        return Ok(PostStatus::Published);
    }

    // Category-level rule
    if category.post_policy == PostPolicy::Moderated {
        return Ok(PostStatus::Pending);
    }

    // Global rule
    let enabled = site_config
        .get("post_approval_enabled")
        .await?
        .map(|v| v == "true")
        .unwrap_or(false);

    if enabled {
        let min_trust_str = site_config
            .get("post_approval_min_trust")
            .await?
            .unwrap_or_else(|| "new".to_string());
        let min_trust = parse_trust_level(&min_trust_str);
        if actor.trust_level < min_trust {
            return Ok(PostStatus::Pending);
        }
    }

    Ok(PostStatus::Published)
}

fn parse_trust_level(s: &str) -> TrustLevel {
    match s {
        "basic" => TrustLevel::Basic,
        "member" => TrustLevel::Member,
        "regular" => TrustLevel::Regular,
        "leader" => TrustLevel::Leader,
        _ => TrustLevel::New,
    }
}

fn extract_mentions(content: &str) -> Vec<String> {
    let re = Regex::new(r"@([a-zA-Z0-9_]{3,32})").expect("valid regex");
    let mut seen = HashSet::new();
    re.captures_iter(content)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_lowercase()))
        .filter(|u| seen.insert(u.clone()))
        .collect()
}

async fn render_content(md: &str) -> Result<String, crate::shared::AppError> {
    let md = md.to_owned();
    tokio::task::spawn_blocking(move || {
        crate::validators::markdown::render_and_sanitize(&md).ok_or_else(|| {
            crate::shared::AppError::unprocessable("Post content exceeds the maximum allowed size")
        })
    })
    .await
    .map_err(|_| crate::shared::AppError::internal("render task panicked"))?
}
