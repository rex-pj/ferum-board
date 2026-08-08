use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use regex::Regex;
use uuid::Uuid;

use chrono::{Duration as ChronoDuration, Utc};

use crate::constants::{
    DEFAULT_MAX_POSTS_PER_PAGE, DEFAULT_POST_EDIT_WINDOW_HOURS, MAX_POST_ATTACHMENT_BYTES,
    MAX_POST_CONTENT_BYTES, MAX_UPLOADS_PER_WINDOW, MAX_UPLOAD_BYTES_PER_WINDOW,
    UPLOAD_QUOTA_WINDOW_HOURS,
};
use crate::event_bus::EventPublisher;
use crate::permission::PermissionChecker;
use crate::ports::{HookContext, HookDecision, PluginHookRuntime, StorageService};
use crate::shared::{AppError, OptionExt};
use crate::storage_utils::{cas_key, validate_image_content_type};
use ferum_domain::events::ForumEvent;
use ferum_domain::models::category::PostPolicy;
use ferum_domain::models::post::{Post, PostStatus};
use ferum_domain::models::thread::ThreadStatus;
use ferum_domain::models::user::TrustLevel;
use ferum_domain::repositories::category_repository::CategoryRepository;
use ferum_domain::repositories::post_repository::{NewPost, PostRepository};
use ferum_domain::repositories::reaction_repository::ReactionRepository;
use ferum_domain::repositories::site_config_repository::{
    get_config_i64, get_config_u64, SiteConfigRepository,
};
use ferum_domain::repositories::stored_file_repository::StoredFileRepository;
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
    pub event_bus: Arc<dyn EventPublisher>,
    pub plugin_runtime: Arc<dyn PluginHookRuntime>,
    /// Only needed by `upload_attachment` (F-CTT-03) — optional so existing
    /// test builders that construct `PostUseCase` without CAS storage still compile.
    ///
    /// Paired with `storage` by the builder: metadata and bytes are two halves of
    /// one upload, and having either alone can only produce a row pointing at
    /// nothing or bytes nobody can find.
    pub stored_files: Option<Arc<dyn StoredFileRepository>>,
    pub storage: Option<Arc<dyn StorageService>>,
    /// Where a *staged* attachment's bytes go, when that differs from where
    /// published ones are served from.
    ///
    /// `None` means no object store is configured, so staging and serving are
    /// the same place: uploads go straight to `storage` and nothing is ever
    /// promoted. **This is the only signal for that.** Deriving it from the
    /// shape of a `public_url` looks equivalent and is not — see
    /// [`PostUseCase::promote_attachment`].
    ///
    /// A staged attachment (`ref_count == 0`) is authorized per viewer — only
    /// its uploader may fetch it, which is what stops "upload, never post, share
    /// the link" from working as free file hosting. That guarantee is
    /// unenforceable once the bytes sit in a world-readable bucket: `/files/`
    /// can refuse to reveal the location, but the object answers anyone who has
    /// it.
    ///
    /// So staging is database-backed regardless of the configured backend, and
    /// the bytes move outward only when a post publishes them — see
    /// [`PostUseCase::promote_attachment`].
    ///
    /// # Scope: this protects a file up to its first publish, and no further
    ///
    /// Promotion is one-way. Nothing demotes an object back into the database
    /// when `ref_count` returns to zero, and nothing deletes it from the bucket,
    /// so **after a post has published an attachment even once, the object stays
    /// world-readable permanently.** Deleting the post drops `ref_count` and
    /// makes `/files/` answer 404 again, but that only closes the route through
    /// this application; anyone already holding the object URL keeps it.
    ///
    /// That is a deliberate position, not an oversight:
    ///
    /// * The exposure is narrow. The object name is a SHA-256 of its own bytes,
    ///   so it cannot be guessed, and the bucket is bound to
    ///   `roles/storage.legacyObjectReader`, which grants `objects.get` without
    ///   `objects.list` — so it cannot be enumerated either. Only someone who
    ///   was already handed the URL retains access.
    /// * Deleting the object instead would be actively worse. Posts are
    ///   soft-deleted: `content_md` survives and still references the key, so a
    ///   moderator reopening a removed post would find the image gone — and when
    ///   the image *is* the violation, that is the evidence for the removal.
    ///
    /// Reclaiming bucket space therefore needs a reconciliation pass that can
    /// prove no post, deleted or otherwise, still references the key. There
    /// isn't one, and the per-account upload quota in `upload_attachment` is
    /// what bounds the residue in the meantime.
    pub staging_storage: Option<Arc<dyn StorageService>>,
}

impl PostUseCase {
    pub fn new(
        posts: Arc<dyn PostRepository>,
        threads: Arc<dyn ThreadRepository>,
        categories: Arc<dyn CategoryRepository>,
        users: Arc<dyn UserRepository>,
        reactions: Arc<dyn ReactionRepository>,
        site_config: Arc<dyn SiteConfigRepository>,
        event_bus: Arc<dyn EventPublisher>,
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
            stored_files: None,
            storage: None,
            staging_storage: None,
        }
    }

    pub fn with_plugin_runtime(mut self, runtime: Arc<dyn PluginHookRuntime>) -> Self {
        self.plugin_runtime = runtime;
        self
    }

    /// Takes all three at once so an upload can never be half-configured.
    ///
    /// `staging_storage` is `Some` **only when an object store is configured**,
    /// and must then be database-backed — see the field docs. Pass `None` when
    /// `storage` is already the database: staging into it would be the same
    /// write, and promoting out of it would delete the bytes.
    pub fn with_stored_files(
        mut self,
        stored_files: Arc<dyn StoredFileRepository>,
        storage: Arc<dyn StorageService>,
        staging_storage: Option<Arc<dyn StorageService>>,
    ) -> Self {
        self.stored_files = Some(stored_files);
        self.storage = Some(storage);
        self.staging_storage = staging_storage;
        self
    }

    /// F-CTT-03: upload an image to embed in post/reply content (e.g. via
    /// `![](url)` markdown), gated the same way as thread thumbnails —
    /// trust_level >= Member (file.upload permission) — plus a rolling
    /// per-account storage quota. Returns the public URL; the caller inserts it
    /// into the post's Markdown themselves.
    ///
    /// The returned file is *staged*: stored, but not publicly servable until
    /// some post embeds its URL (see `sync_attachment_refs`). That is what stops
    /// "upload, never post, share the link" from working as free file hosting.
    ///
    /// Blobs whose ref_count falls back to zero are un-published but not
    /// deleted; the per-account quota is what bounds that residue. Under an
    /// object store, un-publishing closes the `/files/` route but does **not**
    /// retract the object — see [`PostUseCase::staging_storage`] for the scope
    /// of what staging actually guarantees.
    #[tracing::instrument(skip(self, actor, data), fields(user_id = %actor.id))]
    pub async fn upload_attachment(
        &self,
        actor: &AuthUser,
        data: bytes::Bytes,
        content_type: String,
    ) -> Result<String, AppError> {
        PermissionChecker::can_upload(actor)?;

        if !validate_image_content_type(&content_type) {
            return Err(AppError::invalid("attachment_invalid_type"));
        }
        if data.len() > MAX_POST_ATTACHMENT_BYTES {
            return Err(AppError::invalid_with("attachment_too_large", [("limit_mb", (MAX_POST_ATTACHMENT_BYTES / (1024 * 1024)).into())]));
        }
        if !crate::validators::validate_image_magic(&data) {
            return Err(AppError::invalid("image_content_mismatch"));
        }

        let stored_files = self
            .stored_files
            .as_ref()
            .ok_or_else(|| AppError::internal("post attachment storage not configured"))?;
        // Staging when there is one — the bytes must not enter a world-readable
        // bucket until a post publishes them. Falls back to `self.storage`,
        // which is the database anyway whenever staging is absent.
        let storage = self
            .staging_storage
            .as_ref()
            .or(self.storage.as_ref())
            .ok_or_else(|| AppError::internal("post attachment storage not configured"))?;

        // Per-account quota. The write rate limiter upstream keys on IP, so it
        // caps burst rate but not total storage per account — without this an
        // account can upload indefinitely (see upload_quota_exceeded).
        // Staff bypass: they already hold destructive permissions, and a quota
        // stall on a moderator posting evidence is worse than the abuse it prevents.
        if !actor.has_perm(ferum_domain::models::role::perm::ADMIN_USERS)
            && !actor.has_perm(ferum_domain::models::role::perm::MOD_WARN)
        {
            let since = Utc::now() - ChronoDuration::hours(UPLOAD_QUOTA_WINDOW_HOURS);
            let usage = stored_files.usage_since(actor.id, since).await?;
            let would_be_bytes = usage.total_bytes.saturating_add(data.len() as i64);
            if usage.file_count >= MAX_UPLOADS_PER_WINDOW
                || would_be_bytes > MAX_UPLOAD_BYTES_PER_WINDOW
            {
                return Err(AppError::forbidden("upload_quota_exceeded"));
            }
        }

        // Staged (ref_count = 0): stored, but not publicly servable until a post
        // actually embeds this URL. Until then only the uploader can fetch it,
        // which is what the composer's preview needs and nothing more.
        let size = data.len() as i64;
        let key = cas_key("post-attachments", &data, &content_type);
        storage.put(&key, data, &content_type).await?;
        stored_files
            .upsert_staged(&key, &content_type, size, Some(actor.id))
            .await?;

        // `file_url`, NOT `public_url`, and this is the single most consequential
        // instance of that choice in the codebase. What this returns is embedded
        // by the composer into the post's markdown, sanitised into
        // `posts.content_html`, and then never rewritten by anything. A
        // `public_url` here would bake today's bucket and CDN into every post
        // ever written, and from that point the deployment could not change
        // backend, CDN or bucket without breaking its own archive.
        //
        // It is also what lets the staged check in `/files/` run at all: the
        // composer previews through this application instead of fetching the
        // object directly.
        Ok(crate::ports::file_url(&key))
    }

    /// Applies attachment ref-count changes for a post whose content just went
    /// from `old` to `new` (either side may be empty for create/delete).
    ///
    /// Bookkeeping failures are logged, never propagated: refusing to create or
    /// delete a post because a ref_count UPDATE failed would be a worse outcome
    /// than a temporarily mis-counted attachment. A missed increment degrades to
    /// "image visible only to its uploader"; a missed decrement leaks one ref.
    ///
    /// Deliberately does NOT delete blobs when a count reaches zero. Dropping to
    /// zero only un-publishes the file; reclaiming it needs a reconciliation
    /// pass that can prove no other post references it, and getting that wrong
    /// deletes an image that is still on screen somewhere.
    async fn sync_attachment_refs(&self, old: &str, new: &str) {
        let Some(stored_files) = self.stored_files.as_ref() else {
            return;
        };
        let old_keys = extract_attachment_keys(old);
        let new_keys = extract_attachment_keys(new);

        for key in new_keys.difference(&old_keys) {
            if let Err(e) = stored_files.increment_ref(key).await {
                tracing::warn!(attachment_key = %key, "attachment increment_ref failed: {e:?}");
                // No promotion on a failed increment: the file is not published,
                // so it must stay staged and stay private.
                continue;
            }
            self.promote_attachment(key).await;
        }
        for key in old_keys.difference(&new_keys) {
            if let Err(e) = stored_files.decrement_ref(key).await {
                tracing::warn!(attachment_key = %key, "attachment decrement_ref failed: {e:?}");
            }
        }
    }

    /// Moves a just-published attachment out of database staging into the
    /// configured object store.
    ///
    /// **Order is the whole safety argument.** Bytes are written to the object
    /// store *first* and only then dropped from the row, because until that
    /// write lands the row holds the sole copy. Clearing first would turn a
    /// transient upload failure into permanent data loss — the same rule as
    /// "bytes before row" on the way in, applied in reverse.
    ///
    /// Best-effort by design. If either half fails the file simply stays in the
    /// database: `/files/` serves it from there, the post renders, and the only
    /// cost is that one image is served by this process instead of the bucket.
    /// Failing the post edit over it would be a far worse trade.
    ///
    /// A no-op when no object store is configured, which is the common case:
    /// under database storage the bytes are already where they belong. That is
    /// decided by `staging_storage` being `Some`, i.e. by what `startup.rs`
    /// actually selected — **never** by inspecting a `public_url`.
    ///
    /// This function once tested `public_url(key).contains("://")` for that, on
    /// the reasoning that a relative URL means same-origin. It is wrong, and
    /// destructively so: database storage behind `CDN_BASE_URL` mints
    /// `{cdn}/files/{key}`, which is absolute. Promotion would then `put` the
    /// bytes back into the very row it had just read them from and `clear_data`
    /// immediately after — deleting the only copy, and leaving `/files/` to
    /// redirect to a CDN that fetches `/files/` right back.
    async fn promote_attachment(&self, key: &str) {
        let (Some(stored_files), Some(storage), Some(_)) = (
            self.stored_files.as_ref(),
            self.storage.as_ref(),
            self.staging_storage.as_ref(),
        ) else {
            return;
        };

        let bytes = match stored_files.read_data(key).await {
            // Already promoted, or never staged here. Both make this idempotent,
            // which matters because an edit that re-adds the same image runs
            // this path again.
            Ok(None) => return,
            Ok(Some(found)) => found,
            Err(e) => {
                tracing::warn!(attachment_key = %key, "attachment promote read failed: {e:?}");
                return;
            }
        };
        let (data, content_type) = bytes;

        if let Err(e) = storage
            .put(key, bytes::Bytes::from(data), &content_type)
            .await
        {
            tracing::warn!(attachment_key = %key, "attachment promote upload failed: {e:?}");
            return;
        }
        if let Err(e) = stored_files.clear_data(key).await {
            // The object store now has the bytes too. Leaving `data` in place is
            // harmless duplication, not corruption — `/files/` will keep serving
            // the database copy until some later promotion clears it.
            tracing::warn!(attachment_key = %key, "attachment promote cleanup failed: {e:?}");
        }
    }

    #[tracing::instrument(skip_all, fields(thread_id = %thread_id, page = page))]
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
            .or_not_found()?;
        if thread.deleted_at.is_some() {
            return Err(AppError::NotFound);
        }

        let category = self
            .categories
            .find_by_id(thread.category_id)
            .await?
            .or_not_found()?;
        PermissionChecker::can_view_category(actor, &category)?;

        self.list_posts_enriched(actor, thread_id, page, per_page).await
    }

    /// Resolve a post to `(thread_slug, page)` so a caller (a notification link,
    /// the best-answer badge) can deep-link straight to the page the post
    /// actually renders on, instead of always landing on page 1.
    #[tracing::instrument(skip(self, actor), fields(post_id = %post_id))]
    pub async fn locate_post(
        &self,
        actor: Option<&AuthUser>,
        post_id: Uuid,
    ) -> Result<(String, u64), AppError> {
        let post = self.posts.find_by_id(post_id).await?.or_not_found()?;
        // Deleted posts, and Pending posts belonging to someone else, are not
        // independently reachable — same visibility rule as list_by_thread.
        if post.is_deleted {
            return Err(AppError::NotFound);
        }
        if post.status.is_pending() && actor.map(|a| a.id) != Some(post.author_id) {
            return Err(AppError::NotFound);
        }

        let thread = self.threads.find_by_id(post.thread_id).await?.or_not_found()?;
        let category = self.categories.find_by_id(thread.category_id).await?.or_not_found()?;
        PermissionChecker::can_view_category(actor, &category)?;

        let viewer_id = actor.map(|a| a.id);
        let position = self
            .posts
            .position_in_thread(thread.id, post_id, viewer_id)
            .await?
            .or_not_found()?;

        let max_per_page = get_config_u64(self.site_config.as_ref(), "max_posts_per_page", DEFAULT_MAX_POSTS_PER_PAGE).await;
        let per_page = DEFAULT_MAX_POSTS_PER_PAGE.min(max_per_page).max(1);
        let page = position / per_page + 1;

        Ok((thread.slug, page))
    }

    /// Fetch a page of posts for a thread and hydrate authors + reaction counts + the
    /// viewer's own reactions in one batched round of queries. Assumes the caller has
    /// already enforced thread visibility.
    async fn list_posts_enriched(
        &self,
        actor: Option<&AuthUser>,
        thread_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Post>, u64), AppError> {
        let max_per_page = get_config_u64(self.site_config.as_ref(), "max_posts_per_page", DEFAULT_MAX_POSTS_PER_PAGE).await;
        let per_page = per_page.min(max_per_page);
        let viewer_id = actor.map(|a| a.id);
        let (mut posts, total) = self.posts.list_by_thread(thread_id, viewer_id, page, per_page).await?;

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

    #[tracing::instrument(skip(self, actor, cmd), fields(user_id = %actor.id, thread_id = %cmd.thread_id))]
    pub async fn create(&self, actor: &AuthUser, cmd: CreatePostCmd) -> Result<Post, AppError> {
        let thread = self
            .threads
            .find_by_id(cmd.thread_id)
            .await?
            .or_not_found()?;

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
            .or_not_found()?;
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

        if cmd.content_md.trim().is_empty() {
            return Err(AppError::invalid("post_content_empty"));
        }
        if cmd.content_md.len() > MAX_POST_CONTENT_BYTES {
            return Err(AppError::invalid_with("post_content_too_long", [("limit_kb", (MAX_POST_CONTENT_BYTES / 1024).into())]));
        }

        if let Some(parent_id) = cmd.parent_id {
            let parent = self
                .posts
                .find_by_id(parent_id)
                .await?
                .or_not_found()?;
            if parent.thread_id != cmd.thread_id {
                return Err(AppError::invalid("parent_post_wrong_thread"));
            }
        }

        let content_html = render_content(&cmd.content_md).await?;
        let mentions = extract_mentions(&cmd.content_md);

        // Determine whether this post needs moderation approval
        let status =
            resolve_post_status(actor, &category, &self.site_config, &cmd.content_md).await?;
        let needs_approval = status.is_pending();

        // Captured before content_md moves into NewPost.
        let content_for_refs = cmd.content_md.clone();

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

        // Publishes any attachment this post embeds. Runs for Pending posts too:
        // a moderator has to be able to see the image in order to judge it.
        self.sync_attachment_refs("", &content_for_refs).await;

        if needs_approval {
            // Pending posts do not count toward reply stats or fire events yet
            return Ok(post);
        }

        self.threads
            .update_reply_stats(cmd.thread_id, 1, Some(chrono::Utc::now()))
            .await
            .ok();
        self.users.increment_post_count(actor.id, 1).await.ok();

        self.event_bus
            .publish(ForumEvent::PostCreated {
                post_id: post.id,
                thread_id: cmd.thread_id,
                thread_slug: thread.slug.clone(),
                thread_title: thread.title.clone(),
                author_id: actor.id,
                author_username: actor.username.clone(),
                thread_author_id: thread.author_id,
                category_id: thread.category_id,
            })
            .await;

        let mention_list: Vec<String> = mentions.into_iter().take(5).collect();
        let mention_results = futures::future::join_all(
            mention_list.iter().map(|u| self.users.find_by_username(u.as_str())),
        )
        .await;
        for result in mention_results {
            if let Ok(Some(mentioned)) = result {
                if mentioned.id != actor.id {
                    self.event_bus
                        .publish(ForumEvent::MentionAdded {
                            post_id: post.id,
                            thread_id: cmd.thread_id,
                            thread_slug: thread.slug.clone(),
                            thread_title: thread.title.clone(),
                            mentioned_user_id: mentioned.id,
                            author_id: actor.id,
                            author_username: actor.username.clone(),
                        })
                        .await;
                }
            }
        }

        Ok(post)
    }

    #[tracing::instrument(skip(self, actor, content_md), fields(user_id = %actor.id, post_id = %id))]
    pub async fn edit(
        &self,
        actor: &AuthUser,
        id: Uuid,
        content_md: String,
    ) -> Result<Post, AppError> {
        if content_md.trim().is_empty() {
            return Err(AppError::invalid("post_content_empty"));
        }
        if content_md.len() > MAX_POST_CONTENT_BYTES {
            return Err(AppError::invalid_with("post_content_too_long", [("limit_kb", (MAX_POST_CONTENT_BYTES / 1024).into())]));
        }
        let post = self.posts.find_by_id(id).await?.or_not_found()?;
        let thread = self
            .threads
            .find_by_id(post.thread_id)
            .await?
            .or_not_found()?;
        let edit_window_hours = get_config_i64(
            self.site_config.as_ref(),
            "post_edit_window_hours",
            DEFAULT_POST_EDIT_WINDOW_HOURS,
        )
        .await;
        PermissionChecker::can_edit_post(actor, &post, thread.category_id, edit_window_hours)?;

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
        let old_content = post.content_md.clone();
        let new_content = content_md.clone();
        let updated = self
            .posts
            .update_content(id, content_md, content_html, actor.id)
            .await?;

        // Publish attachments the edit added; un-publish the ones it removed.
        self.sync_attachment_refs(&old_content, &new_content).await;

        Ok(updated)
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, post_id = %id))]
    pub async fn delete(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        let post = self.posts.find_by_id(id).await?.or_not_found()?;
        let thread = self
            .threads
            .find_by_id(post.thread_id)
            .await?
            .or_not_found()?;
        PermissionChecker::can_delete_post(actor, &post, thread.category_id)?;

        let hook_ctx = crate::ports::HookContext {
            hook_name: "before_post_delete".to_string(),
            actor_id: Some(actor.id),
            actor_trust_level: format!("{:?}", actor.trust_level).to_lowercase(),
            payload: serde_json::json!({
                "post_id": id,
                "thread_id": post.thread_id,
                "category_id": thread.category_id,
                "author_id": post.author_id,
            }),
        };
        match self
            .plugin_runtime
            .dispatch_before_hook("before_post_delete", &hook_ctx)
            .await?
        {
            HookDecision::Deny { reason, error_code } => {
                return Err(AppError::PluginBlocked { reason, error_code });
            }
            HookDecision::Allow => {}
        }

        let was_already_deleted = post.is_deleted;
        self.posts.soft_delete(id, actor.id).await?;

        // Un-publish this post's attachments. Guarded on the prior state so a
        // repeated delete cannot decrement the same refs twice and un-publish an
        // image another post still embeds.
        if !was_already_deleted {
            self.sync_attachment_refs(&post.content_md, "").await;
        }

        self.event_bus
            .publish(ForumEvent::PostDeleted {
                post_id: id,
                deleted_by_id: actor.id,
            })
            .await;

        // Pending posts were never counted in reply_count (create() returns early
        // before calling update_reply_stats when needs_approval is true).
        // last_post_at is intentionally left untouched — deleting a post (which may
        // not have been the thread's most recent) must not bump the thread to the
        // top of "latest activity" sorts.
        if !post.status.is_pending() {
            self.threads
                .update_reply_stats(post.thread_id, -1, None)
                .await
                .ok();
            self.users.increment_post_count(post.author_id, -1).await.ok();
        }

        Ok(())
    }

    // ── Approval queue ────────────────────────────────────────────────────────

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, page = page))]
    pub async fn list_pending(
        &self,
        actor: &AuthUser,
        category_id: Option<Uuid>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Post>, u64), AppError> {
        use ferum_domain::models::role::perm;

        // Must moderate somewhere at all…
        PermissionChecker::can_view_reports(actor, None)?;
        // …and if a category is named, they must moderate *that* one. A plain
        // 403 is clearer than silently handing back an empty page.
        if let Some(cat_id) = category_id {
            PermissionChecker::can_view_reports(actor, Some(cat_id))?;
        }
        // With no filter supplied, still clamp the result set to the categories
        // the actor moderates. `None` = holds the perm globally, unrestricted.
        let allowed = actor.permitted_category_ids(perm::MOD_VIEW_REPORTS);

        let max_per_page = get_config_u64(self.site_config.as_ref(), "max_posts_per_page", DEFAULT_MAX_POSTS_PER_PAGE).await;
        let per_page = per_page.min(max_per_page);
        self.posts
            .list_pending(category_id, allowed.as_deref(), page, per_page)
            .await
    }

    /// A user's published posts, as seen by `actor`.
    ///
    /// The viewer is not optional context here, it is the access control. This
    /// backs `GET /api/users/:username/posts`, which is public and
    /// unauthenticated, and it used to take no viewer at all — so every post
    /// body written in a `staff_only` category was readable by anyone who knew
    /// a username. The intersection below is the same one
    /// `ThreadUseCase::list_by_author` has always applied to the thread half of
    /// the very same profile page.
    #[tracing::instrument(skip(self, actor), fields(author_id = %author_id, page = page))]
    pub async fn list_by_author(
        &self,
        actor: Option<&AuthUser>,
        author_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Post>, u64), AppError> {
        let visible: Vec<Uuid> = self
            .categories
            .list_all()
            .await?
            .iter()
            .filter(|c| PermissionChecker::can_view_category(actor, c).is_ok())
            .map(|c| c.id)
            .collect();

        let (mut posts, total) = self
            .posts
            .list_by_author(author_id, &visible, page, per_page.min(50))
            .await?;
        if let Ok(Some(user)) = self.users.find_by_id(author_id).await {
            for post in posts.iter_mut() {
                post.author_username = Some(user.username.clone());
                post.author_display_name = user.display_name.clone();
                post.author_avatar_url = user.avatar_url.clone();
                post.author_role = user.primary_role_slug.clone();
            }
        }
        Ok((posts, total))
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, post_id = %id))]
    pub async fn approve_post(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        let post = self.posts.find_by_id(id).await?.or_not_found()?;
        if !post.status.is_pending() {
            return Err(AppError::invalid("post_not_pending_approval"));
        }
        let thread = self
            .threads
            .find_by_id(post.thread_id)
            .await?
            .or_not_found()?;
        if !actor.has_perm("moderation.view_reports")
            && !actor.has_perm_in("moderation.view_reports", thread.category_id)
        {
            return Err(AppError::forbidden("permission_denied"));
        }
        self.posts.set_status(id, PostStatus::Published).await?;
        self.threads
            .update_reply_stats(post.thread_id, 1, Some(chrono::Utc::now()))
            .await
            .ok();
        self.users.increment_post_count(post.author_id, 1).await.ok();
        let author_username = self
            .users
            .find_by_id(post.author_id)
            .await
            .ok()
            .flatten()
            .map(|u| u.username)
            .unwrap_or_default();
        self.event_bus
            .publish(ForumEvent::PostCreated {
                post_id: post.id,
                thread_id: post.thread_id,
                thread_slug: thread.slug,
                thread_title: thread.title,
                author_id: post.author_id,
                author_username,
                thread_author_id: thread.author_id,
                category_id: thread.category_id,
            })
            .await;
        Ok(())
    }

    #[tracing::instrument(skip(self, actor), fields(user_id = %actor.id, post_id = %id))]
    pub async fn reject_post(&self, actor: &AuthUser, id: Uuid) -> Result<(), AppError> {
        let post = self.posts.find_by_id(id).await?.or_not_found()?;
        if !post.status.is_pending() {
            return Err(AppError::invalid("post_not_pending_approval"));
        }
        let thread = self
            .threads
            .find_by_id(post.thread_id)
            .await?
            .or_not_found()?;
        if !actor.has_perm("moderation.view_reports")
            && !actor.has_perm_in("moderation.view_reports", thread.category_id)
        {
            return Err(AppError::forbidden("permission_denied"));
        }
        let was_already_deleted = post.is_deleted;
        self.posts.soft_delete(id, actor.id).await?;

        // A rejected post never becomes visible, so its attachments must not stay
        // published. `is_pending` above stays true after a soft delete, so this
        // path is re-enterable — guard the deref exactly as `delete` does.
        if !was_already_deleted {
            self.sync_attachment_refs(&post.content_md, "").await;
        }
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
    content_md: &str,
) -> Result<PostStatus, AppError> {
    use ferum_domain::models::role::perm;

    // Staff (admin or category moderator) always bypass the approval queue
    if actor.has_perm(perm::ADMIN_USERS) || actor.has_perm_in(perm::MOD_WARN, category.id) {
        return Ok(PostStatus::Published);
    }

    // F-MOD-07: content matching an admin-configured keyword is queued for review
    // rather than rejected outright, so the author isn't tipped off that they were flagged.
    if content_matches_blacklist(site_config, content_md).await? {
        return Ok(PostStatus::Pending);
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

/// F-MOD-07 spam filter: `keyword_blacklist` is admin-edited as one keyword per
/// line (`settings.html`'s textarea); a case-insensitive substring match on
/// any non-blank line flags the post for the moderation queue.
async fn content_matches_blacklist(
    site_config: &Arc<dyn SiteConfigRepository>,
    content_md: &str,
) -> Result<bool, AppError> {
    let raw = site_config.get("keyword_blacklist").await?.unwrap_or_default();
    if raw.trim().is_empty() {
        return Ok(false);
    }
    let content_lower = content_md.to_lowercase();
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|kw| !kw.is_empty())
        .any(|kw| content_lower.contains(&kw.to_lowercase())))
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

/// CAS keys of post attachments embedded in `content_md`, e.g. the
/// `post-attachments/<32 hex>.png` inside `![alt](/files/post-attachments/….png)`.
///
/// The shape is pinned to exactly what `cas_key("post-attachments", …)` emits
/// (16 bytes of SHA-256 as hex, then a whitelisted extension) so a crafted URL
/// in post content can never widen this into a lookup for some other namespace.
/// Deduplicated: one post embedding the same image twice holds a single ref.
///
/// Shared with `ThreadUseCase`, which releases these same references when a
/// whole thread is deleted — one definition, so the two can never disagree
/// about what counts as an attachment reference.
///
/// Matches on the key rather than on a particular URL prefix, because the prefix
/// is not stable and the content is. Post HTML is written once and never
/// rewritten, so a forum that has changed storage backend or added a CDN holds
/// several generations of URL for the same file. Anchoring to `/files/` would
/// have made this silently stop matching newer ones — and since this drives
/// reference counting, "silently stop matching" means attachments never get
/// referenced, stay staged, and are eventually collected out from under posts
/// that still display them.
pub fn extract_attachment_keys(content: &str) -> HashSet<String> {
    ATTACHMENT_KEY_RE
        .captures_iter(content)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

/// The literal `post-attachments/` pins the namespace, and the extensions are
/// the exact set `content_type_to_ext` can return for the content types
/// `upload_attachment` accepts (jpeg/png/webp/gif) — not a loose `[a-z0-9]+`,
/// which would happily match `.exe`. The leading `/` keeps the key at a path
/// boundary, so it cannot be reached by gluing text onto the end of some
/// unrelated word.
///
/// Compiled once. Regex construction is not free — it parses, builds an NFA and
/// may build a DFA — and this ran on every post create and every post edit.
static ATTACHMENT_KEY_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"/(post-attachments/[0-9a-f]{32}\.(?:jpg|png|webp|gif))").expect("valid regex")
});

fn extract_mentions(content: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    MENTION_RE
        .captures_iter(content)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_lowercase()))
        .filter(|u| seen.insert(u.clone()))
        .collect()
}

/// Compiled once — see [`ATTACHMENT_KEY_RE`].
static MENTION_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"@([a-zA-Z0-9_]{3,32})").expect("valid regex"));

async fn render_content(md: &str) -> Result<String, crate::shared::AppError> {
    let md = md.to_owned();
    tokio::task::spawn_blocking(move || {
        crate::validators::markdown::render_and_sanitize(&md).ok_or_else(|| {
            crate::shared::AppError::invalid_with("post_content_too_long", [("limit_kb", (MAX_POST_CONTENT_BYTES / 1024).into())])
        })
    })
    .await
    .map_err(|_| crate::shared::AppError::internal("render task panicked"))?
}
