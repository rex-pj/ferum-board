//! Finds stored objects that nothing in the database points at any more.
//!
//! Reference counting is a write-path mechanism, so it only holds while every
//! write path plays along. Three did not — the site logo, the favicon and theme
//! previews each dropped a reference without collecting the object — and
//! `InlineJobRunner` keeps its queue in memory, so a restart between the
//! decrement and the collection loses the job.
//!
//! No write-path fix reaches any of that retroactively. Two checks here do, and
//! they search in opposite directions:
//!
//! * **`sweep`** enumerates the store and looks for objects with no row. Needs a
//!   backend that can list itself.
//! * **`audit`** walks the rows and looks for ones nothing points at. Pure
//!   database work, so it runs everywhere — and it is the only one that can see
//!   a row stranded at `ref_count = 1`.
//!
//! **Every run is a dry run unless told otherwise**, and the two verbs are
//! separate HTTP methods rather than a flag. The cost of a false positive is an
//! image nobody can restore, so the list is meant to be read by a person first.
//!
//! **Every run is a dry run unless told otherwise.** The whole point is to find
//! files nobody is tracking, and the cost of a false positive is a deleted image
//! nobody can restore — so the reporting and the deleting are separate
//! decisions, made by a person who has seen the list.

use std::collections::HashMap;
use std::sync::Arc;

use crate::permission::PermissionChecker;
use crate::ports::{ForumJob, JobQueue, StorageService};
use crate::shared::AppError;
use ferum_domain::repositories::plugin_repository::PluginRepository;
use ferum_domain::repositories::post_repository::PostRepository;
use ferum_domain::repositories::stored_file_repository::{StoredFileRef, StoredFileRepository};
use ferum_domain::AuthUser;

/// Keys examined per call.
///
/// Bounded because this runs inside an HTTP request: a bucket holding a hundred
/// thousand objects must not become a ten-minute response. The caller resumes
/// with `next_after`.
pub const DEFAULT_SWEEP_LIMIT: usize = 500;
const MAX_SWEEP_LIMIT: usize = 1000;

#[derive(Debug, Default, serde::Serialize)]
pub struct SweepReport {
    /// `false` means the configured backend cannot enumerate itself, so nothing
    /// below was actually looked at. Distinguished from "found nothing" on
    /// purpose — reporting a clean store for a store never read would be the
    /// single most misleading thing this could do.
    pub enumerable: bool,
    pub scanned: usize,
    /// In the store, with no row. Nothing can ever serve or find these again.
    pub orphaned_objects: Vec<String>,
    /// Row present at `ref_count <= 0` — collection was scheduled and never ran,
    /// or never got scheduled.
    pub uncollected: Vec<String>,
    /// Post attachments at `ref_count <= 0`, listed and **never acted on**.
    ///
    /// Reaching zero un-publishes an attachment; it does not make it garbage.
    /// Posts are soft-deleted, so `content_md` still names the file — and when
    /// the image *is* the violation, it is also the evidence for the removal.
    /// Collecting these would delete exactly what a moderator reopening the post
    /// needs to see.
    ///
    /// Reported anyway because "cannot be cleaned automatically" is not the same
    /// as "must stay invisible": an operator who knows a specific post is long
    /// settled can act on one by hand.
    ///
    /// Only ones past [`ABANDONED_ATTACHMENT_GRACE_HOURS`] — see
    /// `active_attachments`.
    pub retained_attachments: Vec<String>,
    /// Attachments at `ref_count <= 0` that are **too recent to judge**, counted
    /// rather than listed.
    ///
    /// A staged attachment is unreferenced for as long as its author keeps the
    /// composer open, so a brand new one is indistinguishable by count alone from
    /// one abandoned forever. Listing it under a heading that says "delete by hand
    /// only" invites deleting an image out of a live draft — the exact outcome
    /// `abandoned_attachments` already uses the same grace window to avoid, and
    /// the sweep had no equivalent.
    ///
    /// A count and not a list on purpose: the number keeps the report honest
    /// about what it saw, while giving nothing for an operator to act on.
    pub active_attachments: usize,
    /// Bytes freed, counted only when `apply` was set.
    pub deleted: usize,
    /// Resume point. `None` means the sweep reached the end of the store.
    pub next_after: Option<String>,
}

/// Post attachments. Held apart from the list below because their reference is
/// not a column — it is markdown inside `posts.content_md` — and because
/// reaching `ref_count = 0` here means "un-published", not "collectable".
/// See [`StorageAuditUseCase::abandoned_attachments`].
pub const ATTACHMENT_NAMESPACE: &str = "post-attachments/";

/// How old an unreferenced attachment must be before the audit will name it.
///
/// A composer holds a staged attachment for as long as the author keeps writing,
/// and during that time nothing references it. 24 hours is far past any drafting
/// session and far short of how long abandoned uploads accumulate.
pub const ABANDONED_ATTACHMENT_GRACE_HOURS: i64 = 24;

/// Post bodies read per query while collecting embedded keys.
const POST_SCAN_PAGE: usize = 200;

/// `plugin_usecase::upload_media` namespaces its CAS keys `plugin_{slug}/…`.
pub const PLUGIN_NAMESPACE_PREFIX: &str = "plugin_";

/// The plugin slug embedded in a `plugin_{slug}/{hash}.ext` key.
///
/// `None` when the key does not have that shape — no `/` after the prefix. A
/// slug may itself contain dots (`com.ferum.chatbox`), so the split is on the
/// first `/`, never on a dot.
fn slug_of_plugin_key(key: &str) -> Option<&str> {
    key.strip_prefix(PLUGIN_NAMESPACE_PREFIX)?
        .split_once('/')
        .map(|(slug, _)| slug)
        .filter(|slug| !slug.is_empty())
}

/// Namespaces whose reference is a plain column, checkable with one anti-join.
///
/// `post-attachments/` and `plugin_*/` are handled separately rather than
/// omitted — their references live in markdown and in plugin-authored markup, so
/// each needs its own check.
pub const AUDITED_NAMESPACES: [&str; 7] = [
    "avatars/",
    "covers/",
    "thumbnails/",
    "products/",
    "theme-previews/",
    "logos/",
    "favicons/",
];

#[derive(Debug, serde::Serialize)]
pub struct AuditReport {
    /// Rows that exist, hold at least one reference, and that **nothing points
    /// at** — grouped by namespace so a surprising result is attributable.
    pub findings: Vec<AuditFinding>,
    pub total: usize,
    /// References released, counted only when `apply` was set.
    pub released: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct AuditFinding {
    pub namespace: &'static str,
    pub keys: Vec<String>,
}

pub struct StorageAuditUseCase {
    stored_files: Arc<dyn StoredFileRepository>,
    storage: Arc<dyn StorageService>,
    jobs: Arc<dyn JobQueue>,
    /// Only to learn which plugins are installed. Plugin media cannot be
    /// anti-joined against the markup that embeds it, but it *can* be checked
    /// against whether its plugin exists at all — which covers the case that
    /// actually happens: an uninstall whose dereference failed.
    plugins: Arc<dyn PluginRepository>,
    /// Only to read `content_md`. Attachment keys are embedded in post markdown,
    /// so this is the only place the reference can be observed.
    posts: Arc<dyn PostRepository>,
}

impl StorageAuditUseCase {
    pub fn new(
        stored_files: Arc<dyn StoredFileRepository>,
        storage: Arc<dyn StorageService>,
        jobs: Arc<dyn JobQueue>,
        plugins: Arc<dyn PluginRepository>,
        posts: Arc<dyn PostRepository>,
    ) -> Self {
        Self {
            stored_files,
            storage,
            jobs,
            plugins,
            posts,
        }
    }

    /// Examines one page of the store.
    ///
    /// # Errors
    /// `permission_denied` without `admin.config`; backend failures propagate
    /// rather than being reported as an empty page.
    pub async fn sweep(
        &self,
        actor: &AuthUser,
        after: Option<&str>,
        limit: usize,
        apply: bool,
    ) -> Result<SweepReport, AppError> {
        // Deleting stored files is as destructive as anything in the admin
        // panel, and this is the only operation that can do it in bulk.
        PermissionChecker::can_manage_config(actor)?;

        let limit = limit.clamp(1, MAX_SWEEP_LIMIT);
        let Some(keys) = self.storage.list_keys(after, limit).await? else {
            return Ok(SweepReport {
                enumerable: false,
                ..Default::default()
            });
        };

        let mut report = SweepReport {
            enumerable: true,
            scanned: keys.len(),
            // A short page is the end of the store; a full one may not be, so
            // the cursor is only cleared when we are certain.
            next_after: (keys.len() == limit).then(|| keys.last().cloned()).flatten(),
            ..Default::default()
        };
        if keys.is_empty() {
            return Ok(report);
        }

        let rows: HashMap<String, StoredFileRef> = self
            .stored_files
            .refs_for(&keys)
            .await?
            .into_iter()
            .map(|row| (row.key.clone(), row))
            .collect();

        // Same window `abandoned_attachments` uses, and deliberately the same
        // constant: two different definitions of "old enough to act on" is how
        // one tool ends up naming a file the other is still protecting.
        let attachment_cutoff =
            chrono::Utc::now() - chrono::Duration::hours(ABANDONED_ATTACHMENT_GRACE_HOURS);

        for key in &keys {
            match rows.get(key) {
                None => report.orphaned_objects.push(key.clone()),
                // A post attachment at zero is un-published, not garbage — see
                // `retained_attachments`. Routing it into `uncollected` would
                // have this tool delete moderation evidence on `?apply=1`, which
                // is what it did before this branch existed.
                Some(row) if row.ref_count <= 0 && key.starts_with(ATTACHMENT_NAMESPACE) => {
                    if row.created_at < attachment_cutoff {
                        report.retained_attachments.push(key.clone());
                    } else {
                        report.active_attachments += 1;
                    }
                }
                Some(row) if row.ref_count <= 0 => report.uncollected.push(key.clone()),
                Some(_) => {}
            }
        }

        if apply {
            report.deleted = self.collect(&report).await;
        }
        Ok(report)
    }

    /// Finds rows that hold a reference nothing has claimed.
    ///
    /// **This is the half the sweep cannot see.** The sweep compares the store
    /// against the row table, so it only finds objects with *no* row. A row
    /// sitting at `ref_count = 1` that nothing points at looks alive to every
    /// count-based check — and that was the exact shape of the theme-preview
    /// leak. Finding it needs the other direction: rows, anti-joined against
    /// what actually references them.
    ///
    /// Pure SQL plus in-memory comparison: no store enumeration, so this works
    /// on every backend including the ones that cannot list themselves, and
    /// costs nothing between runs.
    ///
    /// # Errors
    /// `permission_denied` without `admin.config`.
    pub async fn audit(&self, actor: &AuthUser, apply: bool) -> Result<AuditReport, AppError> {
        PermissionChecker::can_manage_config(actor)?;

        // Resolved through `key_from_url`, never by string surgery here: the URL
        // shape depends on the backend, and a row predating `ports::file_url`
        // can hold an absolute one. A pointer this fails to resolve would look
        // like an unreferenced file, and the next step deletes those.
        let live: std::collections::HashSet<String> = self
            .stored_files
            .referencing_pointers()
            .await?
            .into_iter()
            .map(|pointer| {
                // `product_media.storage_key` stores the bare key, which
                // `key_from_url` correctly declines — fall back to the raw
                // value rather than dropping it, or every product image reads
                // as unreferenced.
                self.storage
                    .key_from_url(&pointer)
                    .unwrap_or(pointer)
            })
            .collect();

        let mut report = AuditReport {
            findings: Vec::new(),
            total: 0,
            released: 0,
        };

        for namespace in AUDITED_NAMESPACES {
            let keys = self.stored_files.list_keys_with_prefix(namespace).await?;
            let unreferenced: Vec<String> =
                keys.into_iter().filter(|k| !live.contains(k)).collect();
            if unreferenced.is_empty() {
                continue;
            }
            report.total += unreferenced.len();
            if apply {
                for key in &unreferenced {
                    // Releases **one** reference, not all of them. A pointer
                    // takes exactly one, so one is the right number — and if a
                    // row somehow holds more, this leaves it above zero and the
                    // next run reports it again. Idempotent, and it avoids
                    // introducing a "force to zero" primitive that could erase a
                    // file somebody is still using.
                    crate::storage_utils::release_cas_ref(&self.stored_files, &self.jobs, key)
                        .await;
                    report.released += 1;
                }
            }
            report.findings.push(AuditFinding {
                namespace,
                keys: unreferenced,
            });
        }

        let abandoned = self.abandoned_attachments().await?;
        if !abandoned.is_empty() {
            report.total += abandoned.len();
            if apply {
                for key in &abandoned {
                    crate::storage_utils::release_cas_ref(&self.stored_files, &self.jobs, key)
                        .await;
                    report.released += 1;
                }
            }
            report.findings.push(AuditFinding {
                namespace: ATTACHMENT_NAMESPACE,
                keys: abandoned,
            });
        }

        let stranded = self.orphaned_plugin_media().await?;
        if !stranded.is_empty() {
            report.total += stranded.len();
            if apply {
                for key in &stranded {
                    crate::storage_utils::release_cas_ref(&self.stored_files, &self.jobs, key)
                        .await;
                    report.released += 1;
                }
            }
            report.findings.push(AuditFinding {
                namespace: PLUGIN_NAMESPACE_PREFIX,
                keys: stranded,
            });
        }

        Ok(report)
    }

    /// Attachments that **no post mentions at all**.
    ///
    /// Distinct from the sweep's `retained_attachments`, and the difference is
    /// the whole reason this is safe. That list is attachments at `ref_count = 0`
    /// — un-published, but still named by a soft-deleted post's `content_md`,
    /// which is why nothing may touch them. This list is attachments that appear
    /// in **no** post body, deleted or otherwise: uploads abandoned in a composer
    /// that was never submitted. There is no post to reopen and no evidence to
    /// preserve.
    ///
    /// Two things keep it from deleting live drafts. `attachment_keys_before`
    /// ignores anything younger than [`ABANDONED_ATTACHMENT_GRACE_HOURS`], and
    /// the referenced set comes from `extract_attachment_keys` — the same
    /// function the write path uses — so "what counts as a reference" has one
    /// definition rather than two.
    async fn abandoned_attachments(&self) -> Result<Vec<String>, AppError> {
        let cutoff = chrono::Utc::now()
            - chrono::Duration::hours(ABANDONED_ATTACHMENT_GRACE_HOURS);
        let candidates = self.stored_files.attachment_keys_before(cutoff).await?;
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        // Paged: a body is arbitrarily long, so reading every post that embeds an
        // image in one query is unbounded memory. Only the extracted keys are
        // kept, and those are fixed-size.
        let mut referenced = std::collections::HashSet::new();
        let mut after = None;
        loop {
            let page = self
                .posts
                .bodies_with_attachments(after, POST_SCAN_PAGE as u64)
                .await?;
            if page.is_empty() {
                break;
            }
            after = page.last().map(|(id, _)| *id);
            let full = page.len() == POST_SCAN_PAGE;
            for (_, body) in page {
                referenced.extend(crate::usecases::post_usecase::extract_attachment_keys(&body));
            }
            if !full {
                break;
            }
        }

        Ok(candidates
            .into_iter()
            .filter(|key| !referenced.contains(key))
            .collect())
    }

    /// Plugin media whose plugin is not installed.
    ///
    /// A weaker check than the other namespaces and deliberately so. Nothing can
    /// tell whether an *installed* plugin still embeds a given image — that
    /// lives in markup the plugin authored. What can be answered is whether the
    /// plugin exists at all, and that covers the case that actually occurs:
    /// `uninstall` dereferences a plugin's whole namespace, so a leftover means
    /// that dereference failed.
    ///
    /// Media belonging to an installed plugin is never reported, however unused
    /// it looks.
    async fn orphaned_plugin_media(&self) -> Result<Vec<String>, AppError> {
        let installed: std::collections::HashSet<String> = self
            .plugins
            .list()
            .await?
            .into_iter()
            .map(|p| p.slug)
            .collect();

        // `list_keys_with_prefix` escapes LIKE metacharacters, so the `_` in
        // `plugin_` matches literally rather than as a wildcard.
        Ok(self
            .stored_files
            .list_keys_with_prefix(PLUGIN_NAMESPACE_PREFIX)
            .await?
            .into_iter()
            .filter(|key| match slug_of_plugin_key(key) {
                Some(slug) => !installed.contains(slug),
                // No `/` after the prefix means this is not a plugin media key
                // in the shape `cas_key` produces. Left alone rather than
                // guessed at.
                None => false,
            })
            .collect())
    }

    /// Acts on what the sweep found, and returns how many objects went.
    ///
    /// The two categories are deleted through different routes, and that is not
    /// incidental. An orphan has no row, so `GcStorageKey` — which re-tests the
    /// row before touching the object — would find nothing and skip it; it has
    /// to be deleted directly. An uncollected key *does* have a row, so it must
    /// go through the job, whose `ref_count = 0` re-test inside the DELETE is
    /// the only thing standing between a sweep and a file someone re-referenced
    /// while it was running.
    async fn collect(&self, report: &SweepReport) -> usize {
        let mut deleted = 0;
        for key in &report.orphaned_objects {
            match self.storage.delete(key).await {
                Ok(()) => deleted += 1,
                Err(e) => tracing::warn!(cas_key = %key, error = ?e, "sweep: object delete failed"),
            }
        }
        for key in &report.uncollected {
            if let Err(e) = self
                .jobs
                .enqueue(ForumJob::GcStorageKey { key: key.clone() })
                .await
            {
                tracing::warn!(cas_key = %key, error = ?e, "sweep: could not enqueue collection");
            }
        }
        deleted
    }
}
