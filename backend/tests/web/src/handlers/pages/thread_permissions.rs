use std::collections::{HashMap, HashSet};

use ferum_domain::models::role::perm;
use ferum_domain::models::user::TrustLevel;
use ferum_domain::AuthUser;
use uuid::Uuid;

use ferum_web::handlers::pages::thread_permissions::{
    post_can_delete, post_can_edit, thread_can_delete, thread_can_edit,
    thread_can_mark_best_answer,
};

fn user(id: Uuid, global_perms: &[&str], category_perms: &[(Uuid, &str)]) -> AuthUser {
    let mut category_permissions: HashMap<Uuid, HashSet<String>> = HashMap::new();
    for (cat_id, perm) in category_perms {
        category_permissions
            .entry(*cat_id)
            .or_default()
            .insert(perm.to_string());
    }
    AuthUser {
        id,
        username: "testuser".to_string(),
        display_name: None,
        avatar_url: None,
        trust_level: TrustLevel::Member,
        is_banned: false,
        banned_until: None,
        permissions: global_perms.iter().map(|s| s.to_string()).collect(),
        category_permissions,
    }
}

fn ids() -> (Uuid, Uuid, Uuid, Uuid) {
    // (author_id, other_user_id, category_id, other_category_id)
    (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
}

// ─── thread_can_edit ──────────────────────────────────────────────────────────

#[test]
fn thread_can_edit_guest_is_false() {
    let (author, _, cat, _) = ids();
    assert!(!thread_can_edit(None, author, cat, false));
}

#[test]
fn thread_can_edit_author_unlocked_is_true() {
    let (author, _, cat, _) = ids();
    let u = user(author, &[], &[]);
    assert!(thread_can_edit(Some(&u), author, cat, false));
}

#[test]
fn thread_can_edit_author_locked_is_false() {
    // Locking removes the author's own edit right — only a mod with
    // thread.edit_any can still edit a locked thread.
    let (author, _, cat, _) = ids();
    let u = user(author, &[], &[]);
    assert!(!thread_can_edit(Some(&u), author, cat, true));
}

#[test]
fn thread_can_edit_non_author_without_perm_is_false() {
    let (author, other, cat, _) = ids();
    let u = user(other, &[], &[]);
    assert!(!thread_can_edit(Some(&u), author, cat, false));
}

#[test]
fn thread_can_edit_mod_with_edit_any_in_category_is_true_even_when_locked() {
    let (author, other, cat, _) = ids();
    let u = user(other, &[], &[(cat, perm::THREAD_EDIT_ANY)]);
    assert!(thread_can_edit(Some(&u), author, cat, true));
}

#[test]
fn thread_can_edit_mod_with_edit_any_in_different_category_is_false() {
    // Category-scoped moderator assignments must not leak into other categories.
    let (author, other, cat, other_cat) = ids();
    let u = user(other, &[], &[(other_cat, perm::THREAD_EDIT_ANY)]);
    assert!(!thread_can_edit(Some(&u), author, cat, false));
}

// ─── thread_can_delete ────────────────────────────────────────────────────────

#[test]
fn thread_can_delete_guest_is_false() {
    let (author, _, cat, _) = ids();
    assert!(!thread_can_delete(None, author, cat));
}

#[test]
fn thread_can_delete_author_is_true() {
    let (author, _, cat, _) = ids();
    let u = user(author, &[], &[]);
    assert!(thread_can_delete(Some(&u), author, cat));
}

#[test]
fn thread_can_delete_non_author_without_perm_is_false() {
    let (author, other, cat, _) = ids();
    let u = user(other, &[], &[]);
    assert!(!thread_can_delete(Some(&u), author, cat));
}

#[test]
fn thread_can_delete_mod_with_delete_any_in_category_is_true() {
    let (author, other, cat, _) = ids();
    let u = user(other, &[], &[(cat, perm::THREAD_DELETE_ANY)]);
    assert!(thread_can_delete(Some(&u), author, cat));
}

#[test]
fn thread_can_delete_mod_with_delete_any_in_different_category_is_false() {
    let (author, other, cat, other_cat) = ids();
    let u = user(other, &[], &[(other_cat, perm::THREAD_DELETE_ANY)]);
    assert!(!thread_can_delete(Some(&u), author, cat));
}

// ─── thread_can_mark_best_answer ──────────────────────────────────────────────

#[test]
fn mark_best_answer_guest_is_false() {
    let (author, _, cat, _) = ids();
    assert!(!thread_can_mark_best_answer(None, author, cat));
}

#[test]
fn mark_best_answer_author_is_true() {
    let (author, _, cat, _) = ids();
    let u = user(author, &[], &[]);
    assert!(thread_can_mark_best_answer(Some(&u), author, cat));
}

#[test]
fn mark_best_answer_mod_with_lock_perm_in_category_is_true() {
    // mirrors ThreadUseCase::mark_solved, which accepts thread.lock as the
    // moderator escape hatch instead of a dedicated best-answer permission.
    let (author, other, cat, _) = ids();
    let u = user(other, &[], &[(cat, perm::THREAD_LOCK)]);
    assert!(thread_can_mark_best_answer(Some(&u), author, cat));
}

#[test]
fn mark_best_answer_unrelated_perm_is_false() {
    let (author, other, cat, _) = ids();
    let u = user(other, &["admin.users"], &[]);
    assert!(!thread_can_mark_best_answer(Some(&u), author, cat));
}

// ─── post_can_edit ────────────────────────────────────────────────────────────

#[test]
fn post_can_edit_guest_is_false() {
    let (author, _, cat, _) = ids();
    assert!(!post_can_edit(None, author, cat));
}

#[test]
fn post_can_edit_own_post_with_edit_own_perm_is_true() {
    let (author, _, cat, _) = ids();
    let u = user(author, &[perm::POST_EDIT_OWN], &[]);
    assert!(post_can_edit(Some(&u), author, cat));
}

#[test]
fn post_can_edit_own_post_without_edit_own_perm_is_false() {
    // Authorship alone is not enough — trust-level gating happens via the
    // post.edit_own permission, same as PermissionChecker::can_edit_post.
    let (author, _, cat, _) = ids();
    let u = user(author, &[], &[]);
    assert!(!post_can_edit(Some(&u), author, cat));
}

#[test]
fn post_can_edit_mod_with_thread_edit_any_in_category_is_true() {
    let (author, other, cat, _) = ids();
    let u = user(other, &[], &[(cat, perm::THREAD_EDIT_ANY)]);
    assert!(post_can_edit(Some(&u), author, cat));
}

#[test]
fn post_can_edit_mod_with_thread_edit_any_in_different_category_is_false() {
    let (author, other, cat, other_cat) = ids();
    let u = user(other, &[], &[(other_cat, perm::THREAD_EDIT_ANY)]);
    assert!(!post_can_edit(Some(&u), author, cat));
}

// ─── post_can_delete ──────────────────────────────────────────────────────────

#[test]
fn post_can_delete_guest_is_false() {
    let (author, _, cat, _) = ids();
    assert!(!post_can_delete(None, author, cat));
}

#[test]
fn post_can_delete_own_post_with_delete_own_perm_is_true() {
    let (author, _, cat, _) = ids();
    let u = user(author, &[perm::POST_DELETE_OWN], &[]);
    assert!(post_can_delete(Some(&u), author, cat));
}

#[test]
fn post_can_delete_own_post_without_delete_own_perm_is_false() {
    let (author, _, cat, _) = ids();
    let u = user(author, &[], &[]);
    assert!(!post_can_delete(Some(&u), author, cat));
}

#[test]
fn post_can_delete_mod_with_post_delete_any_in_category_is_true() {
    let (author, other, cat, _) = ids();
    let u = user(other, &[], &[(cat, perm::POST_DELETE_ANY)]);
    assert!(post_can_delete(Some(&u), author, cat));
}

#[test]
fn post_can_delete_mod_with_post_delete_any_in_different_category_is_false() {
    let (author, other, cat, other_cat) = ids();
    let u = user(other, &[], &[(other_cat, perm::POST_DELETE_ANY)]);
    assert!(!post_can_delete(Some(&u), author, cat));
}
