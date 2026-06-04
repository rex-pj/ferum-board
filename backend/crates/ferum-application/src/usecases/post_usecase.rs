use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use regex::Regex;
use uuid::Uuid;

use crate::constants::{MAX_POSTS_PER_PAGE, MAX_POST_CONTENT_BYTES};
use crate::event_bus::EventBus;
use crate::permission::PermissionChecker;
use crate::shared::AppError;
use ferum_domain::events::ForumEvent;
use ferum_domain::models::post::Post;
use ferum_domain::models::thread::ThreadStatus;
use ferum_domain::repositories::category_repository::CategoryRepository;
use ferum_domain::repositories::post_repository::{NewPost, PostRepository};
use ferum_domain::repositories::reaction_repository::ReactionRepository;
use ferum_domain::repositories::thread_repository::ThreadRepository;
use ferum_domain::repositories::user_repository::UserRepository;
use ferum_domain::AuthUser;

pub struct PostUseCase {
    pub posts: Arc<dyn PostRepository>,
    pub threads: Arc<dyn ThreadRepository>,
    pub categories: Arc<dyn CategoryRepository>,
    pub users: Arc<dyn UserRepository>,
    pub reactions: Arc<dyn ReactionRepository>,
    pub event_bus: Arc<EventBus>,
}

impl PostUseCase {
    pub fn new(
        posts: Arc<dyn PostRepository>,
        threads: Arc<dyn ThreadRepository>,
        categories: Arc<dyn CategoryRepository>,
        users: Arc<dyn UserRepository>,
        reactions: Arc<dyn ReactionRepository>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self {
            posts,
            threads,
            categories,
            users,
            reactions,
            event_bus,
        }
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

        let post = self
            .posts
            .create(NewPost {
                thread_id: cmd.thread_id,
                author_id: actor.id,
                parent_id: cmd.parent_id,
                content_md: cmd.content_md,
                content_html,
            })
            .await?;

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

        self.threads
            .update_reply_stats(post.thread_id, -1, chrono::Utc::now())
            .await
            .ok();

        Ok(())
    }
}

#[derive(Debug)]
pub struct CreatePostCmd {
    pub thread_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub content_md: String,
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
