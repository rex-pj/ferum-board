//! The orphan sweep.
//!
//! Its whole job is deleting files, so the tests that matter are the ones about
//! *not* deleting: a referenced object must survive, a backend that cannot
//! enumerate must say so rather than report a clean store, and a report must
//! never delete anything.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use ferum_application::ports::{ForumJob, JobQueue, StorageService};
use ferum_application::usecases::storage_audit_usecase::{
    StorageAuditUseCase, ABANDONED_ATTACHMENT_GRACE_HOURS,
};
use ferum_domain::repositories::stored_file_repository::{
    StoredFileRef, StoredFileRepository, UploadUsage,
};
use ferum_domain::{AppError, AuthUser};
use ferum_test_support::fixtures::AuthUserBuilder;
use ferum_test_support::mocks::plugin_repository::MockPluginRepository;
use ferum_test_support::mocks::post_repository::MockPostRepository;
use uuid::Uuid;

// ─── Doubles ─────────────────────────────────────────────────────────────────

/// A store holding a fixed set of keys, recording what gets deleted.
///
/// `enumerable = false` models GCS, which has no `list_keys` implementation yet
/// and therefore falls through to the port's default.
struct FakeStore {
    keys: Vec<String>,
    enumerable: bool,
    deleted: Mutex<Vec<String>>,
}

impl FakeStore {
    fn holding(keys: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            keys: keys.iter().map(|k| k.to_string()).collect(),
            enumerable: true,
            deleted: Mutex::new(Vec::new()),
        })
    }
    fn blind() -> Arc<Self> {
        Arc::new(Self {
            keys: Vec::new(),
            enumerable: false,
            deleted: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl StorageService for FakeStore {
    async fn put(&self, _k: &str, _d: Bytes, _c: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn delete(&self, key: &str) -> Result<(), AppError> {
        self.deleted.lock().unwrap().push(key.to_string());
        Ok(())
    }
    fn public_url(&self, key: &str) -> String {
        format!("/files/{key}")
    }
    /// Resolves the absolute `{cdn}/files/{key}` form as well as the bare path,
    /// because a real adapter does and the audit's correctness depends on it: a
    /// pointer that fails to resolve reads as an unreferenced file, and those
    /// get released. A double that only stripped a leading `/files/` would let
    /// the use case hardcode that prefix and still pass.
    fn key_from_url(&self, url: &str) -> Option<String> {
        url.find("/files/")
            .map(|i| url[i + "/files/".len()..].to_string())
            .filter(|k| !k.is_empty())
    }
    async fn list_keys(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Option<Vec<String>>, AppError> {
        if !self.enumerable {
            return Ok(None);
        }
        let mut keys: Vec<String> = self.keys.clone();
        keys.sort();
        Ok(Some(
            keys.into_iter()
                .filter(|k| after.is_none_or(|a| k.as_str() > a))
                .take(limit)
                .collect(),
        ))
    }
}

/// Rows keyed by CAS key with their reference count. Anything absent has no row.
///
/// `pointers` is the other half, for the audit: the values that columns like
/// `users.avatar_url` actually hold. Stored as written — `/files/{key}` for the
/// URL columns, a bare key for `product_media.storage_key` — because resolving
/// the difference is the behaviour under test.
struct FakeRows {
    counts: HashMap<String, i32>,
    /// Per-key `created_at`. Absent means [`FakeRows::AGED`] — every fixture that
    /// is not specifically about age wants a row old enough to be actionable, and
    /// defaulting to "new" would silently move most of these tests into the grace
    /// window instead.
    ages: HashMap<String, chrono::DateTime<chrono::Utc>>,
    pointers: Vec<String>,
    released: Mutex<Vec<String>>,
}

impl FakeRows {
    /// Comfortably past `ABANDONED_ATTACHMENT_GRACE_HOURS`.
    const AGED: chrono::TimeDelta = chrono::TimeDelta::days(30);

    fn with(entries: &[(&str, i32)]) -> Arc<Self> {
        Arc::new(Self {
            counts: entries.iter().map(|(k, c)| (k.to_string(), *c)).collect(),
            ages: HashMap::new(),
            pointers: Vec::new(),
            released: Mutex::new(Vec::new()),
        })
    }

    /// Rows plus the pointers that reference them, for the audit tests.
    fn with_pointers(entries: &[(&str, i32)], pointers: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            counts: entries.iter().map(|(k, c)| (k.to_string(), *c)).collect(),
            ages: HashMap::new(),
            pointers: pointers.iter().map(|p| p.to_string()).collect(),
            released: Mutex::new(Vec::new()),
        })
    }

    /// Rows where some keys carry an explicit age, for the grace-window tests.
    fn with_ages(entries: &[(&str, i32, chrono::TimeDelta)]) -> Arc<Self> {
        let now = chrono::Utc::now();
        Arc::new(Self {
            counts: entries.iter().map(|(k, c, _)| (k.to_string(), *c)).collect(),
            ages: entries
                .iter()
                .map(|(k, _, age)| (k.to_string(), now - *age))
                .collect(),
            pointers: Vec::new(),
            released: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl StoredFileRepository for FakeRows {
    async fn refs_for(&self, keys: &[String]) -> Result<Vec<StoredFileRef>, AppError> {
        let aged = chrono::Utc::now() - Self::AGED;
        Ok(keys
            .iter()
            .filter_map(|k| {
                self.counts.get(k).map(|c| StoredFileRef {
                    key: k.clone(),
                    ref_count: *c,
                    created_at: self.ages.get(k).copied().unwrap_or(aged),
                })
            })
            .collect())
    }
    async fn referencing_pointers(&self) -> Result<Vec<String>, AppError> {
        Ok(self.pointers.clone())
    }
    /// Ignores the cutoff, so nothing here can vouch for the age floor — the
    /// fixtures are about *whether a post mentions the key*. That floor is a SQL
    /// predicate and is covered where it can actually be exercised, in
    /// `tests/infrastructure`'s `the_age_floor_excludes_a_recently_staged_row`.
    /// The sweep's own grace window is a different mechanism, covered by
    /// `a_freshly_staged_attachment_is_counted_but_not_listed` below.
    async fn attachment_keys_before(
        &self,
        _cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<String>, AppError> {
        let mut keys: Vec<String> = self
            .counts
            .keys()
            .filter(|k| k.starts_with("post-attachments/"))
            .cloned()
            .collect();
        keys.sort();
        Ok(keys)
    }
    async fn list_keys_with_prefix(&self, prefix: &str) -> Result<Vec<String>, AppError> {
        let mut keys: Vec<String> = self
            .counts
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        keys.sort();
        Ok(keys)
    }
    async fn usage_since(
        &self,
        _b: Uuid,
        _s: chrono::DateTime<chrono::Utc>,
    ) -> Result<UploadUsage, AppError> {
        Ok(UploadUsage { file_count: 0, total_bytes: 0 })
    }
    async fn upsert_and_ref(&self, _k: &str, _c: &str, _s: i64, _b: Option<Uuid>) -> Result<(), AppError> {
        Ok(())
    }
    async fn upsert_staged(&self, _k: &str, _c: &str, _s: i64, _b: Option<Uuid>) -> Result<(), AppError> {
        Ok(())
    }
    async fn increment_ref(&self, _k: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn read_data(&self, _k: &str) -> Result<Option<(Vec<u8>, String)>, AppError> {
        Ok(None)
    }
    async fn clear_data(&self, _k: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn decrement_ref(&self, key: &str) -> Result<i32, AppError> {
        self.released.lock().unwrap().push(key.to_string());
        Ok(0)
    }
    async fn delete_if_unreferenced(&self, _k: &str) -> Result<bool, AppError> {
        Ok(true)
    }
}

struct SpyJobs(Mutex<Vec<String>>);

impl SpyJobs {
    fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(Vec::new())))
    }
}

#[async_trait]
impl JobQueue for SpyJobs {
    async fn enqueue(&self, job: ForumJob) -> Result<(), AppError> {
        if let ForumJob::GcStorageKey { key } = job {
            self.0.lock().unwrap().push(key);
        }
        Ok(())
    }
}

fn admin() -> AuthUser {
    AuthUserBuilder::admin().build()
}

/// A plugin row with nothing set but the slug — the audit reads no other field.
fn plugin_named(slug: &str) -> ferum_domain::models::plugin::Plugin {
    ferum_domain::models::plugin::Plugin {
        id: Uuid::new_v4(),
        slug: slug.to_string(),
        name: slug.to_string(),
        version: "1.0.0".into(),
        tier: ferum_domain::models::plugin::PluginTier::Script,
        status: ferum_domain::models::plugin::PluginStatus::Active,
        manifest: serde_json::json!({}),
        config: serde_json::json!({}),
        granted_capabilities: serde_json::json!({}),
        install_path: String::new(),
        db_schema_name: None,
        db_schema_version: 0,
        installed_by: None,
        installed_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        activated_at: None,
        error_message: None,
        last_seen_at: None,
        restart_count: 0,
        circuit_open: false,
    }
}

/// Post bodies the audit will scan for embedded attachment keys.
fn posts_containing(bodies: &[&str]) -> Arc<MockPostRepository> {
    let rows: Vec<(Uuid, String)> = bodies
        .iter()
        .map(|b| (Uuid::new_v4(), b.to_string()))
        .collect();
    let mut mock = MockPostRepository::new();
    // Answers the first page and then nothing, which ends the caller's loop.
    mock.expect_bodies_with_attachments()
        .returning(move |after, _| Ok(if after.is_none() { rows.clone() } else { vec![] }));
    Arc::new(mock)
}

fn plugins_installed(slugs: &[&str]) -> Arc<MockPluginRepository> {
    let rows: Vec<_> = slugs.iter().map(|s| plugin_named(s)).collect();
    let mut mock = MockPluginRepository::new();
    mock.expect_list().returning(move || Ok(rows.clone()));
    Arc::new(mock)
}

/// The sweep never consults the plugin table, so its tests pass an empty one.
fn sweep_uc(
    rows: Arc<FakeRows>,
    store: Arc<FakeStore>,
    jobs: Arc<SpyJobs>,
) -> StorageAuditUseCase {
    StorageAuditUseCase::new(rows, store, jobs, plugins_installed(&[]), posts_containing(&[]))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_object_with_no_row_is_reported_as_an_orphan() {
    let store = FakeStore::holding(&["avatars/live.jpg", "logos/stranded.png"]);
    let rows = FakeRows::with(&[("avatars/live.jpg", 1)]);

    let report = sweep_uc(rows, store.clone(), SpyJobs::new())
        .sweep(&admin(), None, 100, false)
        .await
        .expect("sweep runs");

    assert!(report.enumerable);
    assert_eq!(report.scanned, 2);
    assert_eq!(report.orphaned_objects, ["logos/stranded.png"]);
    assert!(
        store.deleted.lock().unwrap().is_empty(),
        "a report must never delete — that is the whole reason it is separate"
    );
    assert_eq!(report.deleted, 0);
}

#[tokio::test]
async fn a_row_left_at_zero_is_reported_as_uncollected() {
    // A GC job lost to a restart, or one that was never enqueued. The row is
    // there, so this is not an orphan — it needs collecting, not deleting.
    let store = FakeStore::holding(&["avatars/a.jpg"]);
    let rows = FakeRows::with(&[("avatars/a.jpg", 0)]);

    let report = sweep_uc(rows, store, SpyJobs::new())
        .sweep(&admin(), None, 100, false)
        .await
        .expect("sweep runs");

    assert!(report.orphaned_objects.is_empty());
    assert_eq!(report.uncollected, ["avatars/a.jpg"]);
}

#[tokio::test]
async fn a_post_attachment_at_zero_is_reported_but_never_collected() {
    // The most consequential bug this tool could have, and it had it: any row at
    // `ref_count <= 0` went into `uncollected`, and `?apply=1` collected them —
    // attachments included. A post is soft-deleted, so `content_md` still names
    // the file, and when the image IS the violation it is also the evidence for
    // the removal. Deleting it is exactly wrong.
    let store = FakeStore::holding(&["post-attachments/evidence.jpg"]);
    let rows = FakeRows::with(&[("post-attachments/evidence.jpg", 0)]);
    let jobs = SpyJobs::new();

    let report = sweep_uc(rows, store.clone(), jobs.clone())
        .sweep(&admin(), None, 100, true)
        .await
        .expect("sweep runs");

    assert_eq!(
        report.retained_attachments,
        ["post-attachments/evidence.jpg"],
        "it must be reported — invisible is not the same as protected"
    );
    assert!(report.uncollected.is_empty());
    assert!(
        store.deleted.lock().unwrap().is_empty() && jobs.0.lock().unwrap().is_empty(),
        "and nothing may act on it, even with apply set"
    );
    assert_eq!(report.active_attachments, 0, "30 days old is not active");
}

#[tokio::test]
async fn a_freshly_staged_attachment_is_counted_but_not_listed() {
    // An attachment is unreferenced for as long as its author keeps the composer
    // open, so by count alone a draft in progress is indistinguishable from one
    // abandoned forever. `abandoned_attachments` already refuses to name one
    // inside the grace window; the sweep had no equivalent, so it listed a live
    // draft's image under a heading reading "delete by hand only".
    let store = FakeStore::holding(&[
        "post-attachments/being-typed.jpg",
        "post-attachments/long-gone.jpg",
    ]);
    let rows = FakeRows::with_ages(&[
        ("post-attachments/being-typed.jpg", 0, chrono::TimeDelta::minutes(5)),
        ("post-attachments/long-gone.jpg", 0, chrono::TimeDelta::days(9)),
    ]);

    let report = sweep_uc(rows, store, SpyJobs::new())
        .sweep(&admin(), None, 100, false)
        .await
        .expect("sweep runs");

    assert_eq!(
        report.retained_attachments,
        ["post-attachments/long-gone.jpg"],
        "only the one past the grace window may be offered for manual deletion"
    );
    assert_eq!(
        report.active_attachments, 1,
        "the recent one is still counted — the report must not under-report what it saw"
    );
    // Neither is collectable either way; the split is about what gets shown.
    assert!(report.uncollected.is_empty());
}

#[tokio::test]
async fn the_two_tools_share_one_definition_of_old_enough() {
    // Right at the boundary from both sides. Two different cutoffs is how one tool
    // ends up naming a file the other is still protecting, which is why both read
    // `ABANDONED_ATTACHMENT_GRACE_HOURS` rather than each carrying a literal.
    let grace = chrono::TimeDelta::hours(ABANDONED_ATTACHMENT_GRACE_HOURS);
    let store = FakeStore::holding(&["post-attachments/just-inside.jpg", "post-attachments/just-outside.jpg"]);
    let rows = FakeRows::with_ages(&[
        // A minute short of the window: still protected.
        ("post-attachments/just-inside.jpg", 0, grace - chrono::TimeDelta::minutes(1)),
        ("post-attachments/just-outside.jpg", 0, grace + chrono::TimeDelta::minutes(1)),
    ]);

    let report = sweep_uc(rows, store, SpyJobs::new())
        .sweep(&admin(), None, 100, false)
        .await
        .expect("sweep runs");

    assert_eq!(
        report.retained_attachments,
        ["post-attachments/just-outside.jpg"]
    );
    assert_eq!(report.active_attachments, 1);
}

#[tokio::test]
async fn a_non_attachment_at_zero_is_still_collected() {
    // The other side: the retention rule must not leak into every namespace, or
    // the tool stops doing its job.
    let store = FakeStore::holding(&["avatars/zero.jpg"]);
    let rows = FakeRows::with(&[("avatars/zero.jpg", 0)]);
    let jobs = SpyJobs::new();

    sweep_uc(rows, store, jobs.clone())
        .sweep(&admin(), None, 100, true)
        .await
        .expect("sweep runs");

    assert_eq!(jobs.0.lock().unwrap().as_slice(), ["avatars/zero.jpg"]);
}

#[tokio::test]
async fn applying_deletes_orphans_directly_and_collects_rows_through_the_job() {
    // The split is load-bearing. An orphan has no row, so `GcStorageKey` would
    // find nothing to delete and skip the object entirely — it must go direct.
    // An uncollected key does have a row, so it must go through the job, whose
    // `ref_count = 0` re-test inside the DELETE is the only guard against
    // deleting something re-referenced mid-sweep.
    let store = FakeStore::holding(&["logos/orphan.png", "avatars/zero.jpg"]);
    let rows = FakeRows::with(&[("avatars/zero.jpg", 0)]);
    let jobs = SpyJobs::new();

    let report = sweep_uc(rows, store.clone(), jobs.clone())
        .sweep(&admin(), None, 100, true)
        .await
        .expect("sweep runs");

    assert_eq!(store.deleted.lock().unwrap().as_slice(), ["logos/orphan.png"]);
    assert_eq!(jobs.0.lock().unwrap().as_slice(), ["avatars/zero.jpg"]);
    assert_eq!(report.deleted, 1, "only direct deletes are counted");
}

#[tokio::test]
async fn a_referenced_object_is_never_touched() {
    let store = FakeStore::holding(&["avatars/keep.jpg"]);
    let rows = FakeRows::with(&[("avatars/keep.jpg", 3)]);
    let jobs = SpyJobs::new();

    let report = sweep_uc(rows, store.clone(), jobs.clone())
        .sweep(&admin(), None, 100, true)
        .await
        .expect("sweep runs");

    assert!(report.orphaned_objects.is_empty());
    assert!(report.uncollected.is_empty());
    assert!(store.deleted.lock().unwrap().is_empty());
    assert!(jobs.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_backend_that_cannot_enumerate_says_so_rather_than_reporting_a_clean_store() {
    // The most dangerous possible output of this tool is "found nothing" for a
    // store it never read. GCS has no `list_keys` yet and falls through to the
    // port default, so this is a live case, not a hypothetical.
    let report = sweep_uc(FakeRows::with(&[]), FakeStore::blind(), SpyJobs::new())
        .sweep(&admin(), None, 100, true)
        .await
        .expect("sweep runs");

    assert!(!report.enumerable);
    assert_eq!(report.scanned, 0);
    assert_eq!(report.deleted, 0);
}

#[tokio::test]
async fn paging_stops_when_the_store_runs_out() {
    let store = FakeStore::holding(&["a", "b", "c"]);
    let rows = FakeRows::with(&[("a", 1), ("b", 1), ("c", 1)]);
    let uc = sweep_uc(rows, store, SpyJobs::new());

    let first = uc.sweep(&admin(), None, 2, false).await.expect("page 1");
    assert_eq!(first.scanned, 2);
    assert_eq!(
        first.next_after.as_deref(),
        Some("b"),
        "a full page may not be the end, so the cursor must carry on"
    );

    let second = uc
        .sweep(&admin(), first.next_after.as_deref(), 2, false)
        .await
        .expect("page 2");
    assert_eq!(second.scanned, 1);
    assert!(
        second.next_after.is_none(),
        "a short page is the end of the store"
    );
}

// ─── The namespace audit ─────────────────────────────────────────────────────

#[tokio::test]
async fn a_row_nothing_points_at_is_found_even_though_it_looks_referenced() {
    // The theme-preview leak, exactly: `ref_count = 1`, so every count-based
    // check reads it as alive, and the sweep cannot see it either because the
    // row exists. Only the anti-join finds it.
    let rows = FakeRows::with_pointers(
        &[("theme-previews/live.jpg", 1), ("theme-previews/stranded.jpg", 1)],
        &["/files/theme-previews/live.jpg"],
    );

    let report = sweep_uc(rows, FakeStore::holding(&[]), SpyJobs::new())
        .audit(&admin(), false)
        .await
        .expect("audit runs");

    assert_eq!(report.total, 1);
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].namespace, "theme-previews/");
    assert_eq!(report.findings[0].keys, ["theme-previews/stranded.jpg"]);
    assert_eq!(report.released, 0, "reporting must not release anything");
}

#[tokio::test]
async fn a_bare_storage_key_pointer_counts_as_a_reference() {
    // `product_media.storage_key` holds the key itself, not a URL, so
    // `key_from_url` declines it. Dropping such a pointer instead of falling
    // back to the raw value would make **every product image** read as
    // unreferenced — the single most destructive way this could be wrong.
    let rows = FakeRows::with_pointers(
        &[("products/photo.jpg", 1)],
        &["products/photo.jpg"],
    );

    let report = sweep_uc(rows, FakeStore::holding(&[]), SpyJobs::new())
        .audit(&admin(), false)
        .await
        .expect("audit runs");

    assert_eq!(report.total, 0, "a bare-key pointer is still a reference");
}

#[tokio::test]
async fn an_absolute_cdn_url_still_counts_as_a_reference() {
    // Rows written before `ports::file_url` existed can hold an absolute URL.
    // `key_from_url` is what recognises those; matching with SQL `LIKE` would
    // not, and the next step deletes whatever went unmatched.
    let rows = FakeRows::with_pointers(
        &[("avatars/old.jpg", 1)],
        &["https://cdn.example.com/files/avatars/old.jpg"],
    );

    let report = sweep_uc(rows, FakeStore::holding(&[]), SpyJobs::new())
        .audit(&admin(), false)
        .await
        .expect("audit runs");

    assert_eq!(report.total, 0);
}

#[tokio::test]
async fn applying_releases_one_reference_per_finding() {
    let rows = FakeRows::with_pointers(&[("avatars/gone.jpg", 1)], &[]);
    let jobs = SpyJobs::new();

    let report = sweep_uc(rows.clone(), FakeStore::holding(&[]), jobs.clone())
        .audit(&admin(), true)
        .await
        .expect("audit runs");

    assert_eq!(report.released, 1);
    assert_eq!(rows.released.lock().unwrap().as_slice(), ["avatars/gone.jpg"]);
    assert_eq!(
        jobs.0.lock().unwrap().as_slice(),
        ["avatars/gone.jpg"],
        "reaching zero must schedule collection of the object too"
    );
}

#[tokio::test]
async fn plugin_media_is_reported_when_its_plugin_is_gone() {
    // The case that actually happens: `uninstall` dereferences a plugin's whole
    // namespace, so a leftover means that dereference failed.
    let rows = FakeRows::with_pointers(
        &[("plugin_chatbox/a.jpg", 1), ("plugin_polls/b.jpg", 1)],
        &[],
    );
    let jobs = SpyJobs::new();

    let report = StorageAuditUseCase::new(rows.clone(), FakeStore::holding(&[]), jobs.clone(), plugins_installed(&["polls"]), posts_containing(&[]))
    .audit(&admin(), true)
    .await
    .expect("audit runs");

    let plugin_finding = report
        .findings
        .iter()
        .find(|f| f.namespace == "plugin_")
        .expect("plugin media must be reported");
    assert_eq!(
        plugin_finding.keys,
        ["plugin_chatbox/a.jpg"],
        "only the uninstalled plugin's media is stranded"
    );
    assert_eq!(jobs.0.lock().unwrap().as_slice(), ["plugin_chatbox/a.jpg"]);
}

#[tokio::test]
async fn media_of_an_installed_plugin_is_left_alone_however_unused_it_looks() {
    // Nothing can tell whether an installed plugin still embeds an image — that
    // lives in markup it authored. Guessing would delete a live asset.
    let rows = FakeRows::with_pointers(&[("plugin_chatbox/a.jpg", 1)], &[]);
    let jobs = SpyJobs::new();

    let report = StorageAuditUseCase::new(rows, FakeStore::holding(&[]), jobs.clone(), plugins_installed(&["chatbox"]), posts_containing(&[]))
    .audit(&admin(), true)
    .await
    .expect("audit runs");

    assert_eq!(report.total, 0);
    assert!(jobs.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_plugin_slug_containing_dots_is_matched_whole() {
    // Slugs like `com.ferum.chatbox` are legal, so the slug must be taken up to
    // the first `/` and never split on a dot.
    let rows = FakeRows::with_pointers(&[("plugin_com.ferum.chatbox/a.jpg", 1)], &[]);

    let report = StorageAuditUseCase::new(rows, FakeStore::holding(&[]), SpyJobs::new(), plugins_installed(&["com.ferum.chatbox"]), posts_containing(&[]))
    .audit(&admin(), false)
    .await
    .expect("audit runs");

    assert_eq!(report.total, 0, "the dotted slug must match its own media");
}

#[tokio::test]
async fn an_attachment_no_post_mentions_is_reported() {
    // Uploaded in a composer that was never submitted. No post to reopen, no
    // evidence to preserve — genuinely garbage, and the last automatable leak.
    let rows = FakeRows::with_pointers(&[("post-attachments/abandoned.jpg", 0)], &[]);
    let jobs = SpyJobs::new();

    let report = StorageAuditUseCase::new(
        rows.clone(),
        FakeStore::holding(&[]),
        jobs.clone(),
        plugins_installed(&[]),
        posts_containing(&["a post with no images at all"]),
    )
    .audit(&admin(), true)
    .await
    .expect("audit runs");

    assert_eq!(report.total, 1);
    assert_eq!(jobs.0.lock().unwrap().as_slice(), ["post-attachments/abandoned.jpg"]);
}

#[tokio::test]
async fn an_attachment_a_deleted_post_still_mentions_is_left_alone() {
    // `bodies_with_attachments` includes soft-deleted posts, so a removed post
    // still counts as a reference. This is the evidence case: the image may be
    // the reason the post was removed.
    let key = "post-attachments/0123456789abcdef0123456789abcdef.jpg";
    let rows = FakeRows::with_pointers(&[(key, 0)], &[]);
    let jobs = SpyJobs::new();

    let report = StorageAuditUseCase::new(
        rows.clone(),
        FakeStore::holding(&[]),
        jobs.clone(),
        plugins_installed(&[]),
        posts_containing(&[&format!("look at this ![](/{key})")]),
    )
    .audit(&admin(), true)
    .await
    .expect("audit runs");

    assert_eq!(report.total, 0);
    assert!(jobs.0.lock().unwrap().is_empty());
    assert!(rows.released.lock().unwrap().is_empty());
}

// `attachments_are_not_audited` was here. It asserted the old behaviour — that
// the audit skipped `post-attachments/` entirely — and became wrong once the
// abandoned-upload check landed. Deleted rather than adjusted: the two tests
// above cover both directions, and a third asserting "no finding" for some third
// fixture would only be another way to get the same answer.

#[tokio::test]
async fn a_non_admin_cannot_audit() {
    let rows = FakeRows::with_pointers(&[("avatars/gone.jpg", 1)], &[]);

    let err = sweep_uc(rows.clone(), FakeStore::holding(&[]), SpyJobs::new())
        .audit(&AuthUserBuilder::member().build(), true)
        .await
        .expect_err("releasing references in bulk is admin-only");

    assert!(matches!(err, AppError::Forbidden(_)));
    assert!(rows.released.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_non_admin_cannot_sweep() {
    let store = FakeStore::holding(&["logos/orphan.png"]);
    let member = AuthUserBuilder::member().build();

    let err = sweep_uc(FakeRows::with(&[]), store.clone(), SpyJobs::new())
        .sweep(&member, None, 100, true)
        .await
        .expect_err("bulk deletion is admin-only");

    assert!(matches!(err, AppError::Forbidden(_)));
    assert!(store.deleted.lock().unwrap().is_empty());
}
