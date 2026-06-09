use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::reaction::ReactionKind;
use crate::models::user::TrustLevel;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ForumEvent {
    PostCreated {
        post_id: Uuid,
        thread_id: Uuid,
        thread_slug: String,
        author_id: Uuid,
        thread_author_id: Uuid,
        category_id: Uuid,
    },
    PostDeleted {
        post_id: Uuid,
        deleted_by_id: Uuid,
    },
    ThreadLocked {
        thread_id: Uuid,
        by_user_id: Uuid,
    },
    ThreadMoved {
        thread_id: Uuid,
        from_category: Uuid,
        to_category: Uuid,
        by_user_id: Uuid,
    },
    UserBanned {
        user_id: Uuid,
        by_user_id: Uuid,
        reason: String,
        until: Option<DateTime<Utc>>,
    },
    UserWarned {
        user_id: Uuid,
        by_user_id: Uuid,
        reason: String,
    },
    ReactionAdded {
        post_id: Uuid,
        thread_id: Uuid,
        thread_slug: String,
        post_author_id: Uuid,
        reactor_id: Uuid,
        kind: ReactionKind,
    },
    ReactionRemoved {
        post_id: Uuid,
        post_author_id: Uuid,
        reactor_id: Uuid,
        kind: ReactionKind,
    },
    TrustLevelChanged {
        user_id: Uuid,
        from: TrustLevel,
        to: TrustLevel,
    },
    BestAnswerMarked {
        post_id: Uuid,
        thread_id: Uuid,
        thread_slug: String,
        post_author_id: Uuid,
        by_user_id: Uuid,
    },
    MentionAdded {
        post_id: Uuid,
        thread_id: Uuid,
        thread_slug: String,
        mentioned_user_id: Uuid,
        author_id: Uuid,
    },
    ThreadCreated {
        thread_id: Uuid,
        thread_slug: String,
        author_id: Uuid,
        category_id: Uuid,
    },
    ThreadDeleted {
        thread_id: Uuid,
        deleted_by_id: Uuid,
    },
    UserFollowed {
        follower_id: Uuid,
        follower_username: String,
        followed_id: Uuid,
    },
}

impl ForumEvent {
    /// Returns the string event type used for webhook subscriptions and plugin dispatch.
    pub fn event_type_str(&self) -> &'static str {
        match self {
            ForumEvent::PostCreated { .. } => "post.created",
            ForumEvent::PostDeleted { .. } => "post.deleted",
            ForumEvent::ThreadLocked { .. } => "thread.locked",
            ForumEvent::ThreadMoved { .. } => "thread.moved",
            ForumEvent::UserBanned { .. } => "user.banned",
            ForumEvent::UserWarned { .. } => "user.warned",
            ForumEvent::ReactionAdded { .. } => "reaction.added",
            ForumEvent::ReactionRemoved { .. } => "reaction.removed",
            ForumEvent::TrustLevelChanged { .. } => "trust_level.changed",
            ForumEvent::BestAnswerMarked { .. } => "best_answer.marked",
            ForumEvent::MentionAdded { .. } => "mention.added",
            ForumEvent::ThreadCreated { .. } => "thread.created",
            ForumEvent::ThreadDeleted { .. } => "thread.deleted",
            ForumEvent::UserFollowed { .. } => "user.followed",
        }
    }
}
