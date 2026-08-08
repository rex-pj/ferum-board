//! Server-computed capability flags for the thread detail page.
//!
//! Extracted as pure functions (rather than inlined closures in the handler)
//! so they can be unit-tested without a DB, use cases, or Tera — the template
//! must only ever branch on these flags, never on `username == ...` /
//! `is_admin` / `is_moderator` comparisons.

use ferum_domain::models::role::perm;
use ferum_domain::AuthUser;
use uuid::Uuid;

/// Mirrors `ThreadUseCase::update_title`: mods with `thread.edit_any` in this
/// category can always edit; the author can only edit while the thread is
/// unlocked (the use case additionally enforces the 24h window — that check
/// is server-side only and deliberately not mirrored in the visible flag, so
/// the button stays visible and the user gets a clear "window expired" error
/// on submit rather than the button silently vanishing at hour 24).
pub fn thread_can_edit(
    user: Option<&AuthUser>,
    author_id: Uuid,
    category_id: Uuid,
    is_locked: bool,
) -> bool {
    let has_edit_any = user.is_some_and(|u| u.has_perm_in(perm::THREAD_EDIT_ANY, category_id));
    let is_author = user.is_some_and(|u| u.id == author_id);
    has_edit_any || (is_author && !is_locked)
}

/// Mirrors `ThreadUseCase::require_author_or_mod`.
pub fn thread_can_delete(user: Option<&AuthUser>, author_id: Uuid, category_id: Uuid) -> bool {
    let has_delete_any = user.is_some_and(|u| u.has_perm_in(perm::THREAD_DELETE_ANY, category_id));
    let is_author = user.is_some_and(|u| u.id == author_id);
    has_delete_any || is_author
}

/// Mirrors `ThreadUseCase::mark_solved`: author, or a mod with `thread.lock`
/// in this category (the same permission `mark_solved` checks server-side).
pub fn thread_can_mark_best_answer(
    user: Option<&AuthUser>,
    author_id: Uuid,
    category_id: Uuid,
) -> bool {
    let is_author = user.is_some_and(|u| u.id == author_id);
    let has_lock = user.is_some_and(|u| u.has_perm_in(perm::THREAD_LOCK, category_id));
    is_author || has_lock
}

/// Mirrors `PermissionChecker::can_edit_post` (minus the 24h window, same
/// rationale as `thread_can_edit`).
///
/// `is_locked` is here for the same reason it is on `thread_can_edit`: a locked
/// thread refuses author edits and grants them to `thread.edit_any`. Unlike the
/// edit window, this one *is* mirrored — the window expires silently mid-session
/// so the button has to stay and explain itself on submit, whereas a lock is a
/// visible state change the reader can see for themselves.
pub fn post_can_edit(
    user: Option<&AuthUser>,
    post_author_id: Uuid,
    category_id: Uuid,
    is_locked: bool,
) -> bool {
    let has_edit_any = user.is_some_and(|u| u.has_perm_in(perm::THREAD_EDIT_ANY, category_id));
    let is_own = user.is_some_and(|u| u.id == post_author_id);
    let has_edit_own = user.is_some_and(|u| u.has_perm(perm::POST_EDIT_OWN));
    has_edit_any || (is_own && has_edit_own && !is_locked)
}

/// Mirrors `PermissionChecker::can_delete_post`.
pub fn post_can_delete(user: Option<&AuthUser>, post_author_id: Uuid, category_id: Uuid) -> bool {
    let has_delete_any = user.is_some_and(|u| u.has_perm_in(perm::POST_DELETE_ANY, category_id));
    let is_own = user.is_some_and(|u| u.id == post_author_id);
    let has_delete_own = user.is_some_and(|u| u.has_perm(perm::POST_DELETE_OWN));
    has_delete_any || (is_own && has_delete_own)
}
