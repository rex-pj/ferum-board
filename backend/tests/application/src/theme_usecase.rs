//! Theme preview images and the CAS reference they hold.
//!
//! Both operations here used to abandon that reference: replacing a preview
//! took a fresh one and walked away from the old, and deleting a theme walked
//! away from it outright. The stranded row sits at `ref_count = 1` — which is
//! worse than 0, because it *looks* referenced, so any sweep hunting orphans by
//! `ref_count = 0` passes straight over it and the object stays in the bucket
//! for good.
//!
//! Under database storage none of this is visible: the row is the bytes, so
//! losing the row loses them together. It only bites on S3/GCS/R2 — which is
//! the production configuration.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use ferum_application::ports::{ForumJob, JobQueue};
use ferum_application::usecases::theme_usecase::ThemeUseCase;
use ferum_domain::models::theme::{NewTheme, Theme};
use ferum_domain::repositories::stored_file_repository::{StoredFileRepository, UploadUsage};
use ferum_domain::repositories::theme_repository::ThemeRepository;
use ferum_domain::AppError;
use ferum_test_support::mocks::storage_service::NoopStorageService;
use uuid::Uuid;

// ─── Doubles ─────────────────────────────────────────────────────────────────

/// Holds one theme and records what was done to it.
struct FakeThemes {
    theme: Mutex<Option<Theme>>,
    deleted: Mutex<bool>,
    preview_writes: Mutex<Vec<Option<String>>>,
}

impl FakeThemes {
    fn with_preview(preview_url: Option<&str>) -> Arc<Self> {
        Arc::new(Self {
            theme: Mutex::new(Some(Theme {
                id: Uuid::new_v4(),
                slug: "sumi".into(),
                name: "Sumi".into(),
                author: None,
                version: "1.0.0".into(),
                description: None,
                parent_slug: "default".into(),
                is_system: false,
                is_active: false,
                preview_url: preview_url.map(str::to_string),
                created_at: Utc::now(),
            })),
            deleted: Mutex::new(false),
            preview_writes: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl ThemeRepository for FakeThemes {
    async fn list(&self) -> Result<Vec<Theme>, AppError> {
        Ok(self.theme.lock().unwrap().clone().into_iter().collect())
    }
    async fn get_active(&self) -> Result<Theme, AppError> {
        Err(AppError::NotFound)
    }
    async fn find_by_slug(&self, _slug: &str) -> Result<Option<Theme>, AppError> {
        Ok(self.theme.lock().unwrap().clone())
    }
    async fn upsert(&self, _theme: NewTheme) -> Result<Theme, AppError> {
        Err(AppError::NotFound)
    }
    async fn set_active(&self, _slug: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn update_preview_url(&self, _slug: &str, url: Option<String>) -> Result<(), AppError> {
        self.preview_writes.lock().unwrap().push(url);
        Ok(())
    }
    async fn delete(&self, _slug: &str) -> Result<(), AppError> {
        *self.deleted.lock().unwrap() = true;
        Ok(())
    }
}

/// Records every `decrement_ref` and reports the key as reaching zero, so the
/// GC enqueue that follows is the thing under observation.
struct RecordingFiles {
    decremented: Mutex<Vec<String>>,
}

impl RecordingFiles {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            decremented: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl StoredFileRepository for RecordingFiles {
    async fn attachment_keys_before(&self, _: chrono::DateTime<chrono::Utc>) -> Result<Vec<String>, AppError> { Ok(vec![]) }
    async fn referencing_pointers(&self) -> Result<Vec<String>, AppError> { Ok(vec![]) }
    async fn ref_counts_for(&self, _: &[String]) -> Result<Vec<(String, i32)>, AppError> { Ok(vec![]) }
    async fn usage_since(
        &self,
        _by: Uuid,
        _since: chrono::DateTime<Utc>,
    ) -> Result<UploadUsage, AppError> {
        Ok(UploadUsage { file_count: 0, total_bytes: 0 })
    }
    async fn upsert_and_ref(&self, _k: &str, _c: &str, _s: i64, _b: Option<Uuid>) -> Result<(), AppError> {
        Ok(())
    }
    async fn upsert_staged(&self, _k: &str, _c: &str, _s: i64, _b: Option<Uuid>) -> Result<(), AppError> {
        Ok(())
    }
    async fn increment_ref(&self, _key: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn read_data(&self, _key: &str) -> Result<Option<(Vec<u8>, String)>, AppError> {
        Ok(None)
    }
    async fn clear_data(&self, _key: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn decrement_ref(&self, key: &str) -> Result<i32, AppError> {
        self.decremented.lock().unwrap().push(key.to_string());
        Ok(0)
    }
    async fn delete_if_unreferenced(&self, _key: &str) -> Result<bool, AppError> {
        Ok(true)
    }
    async fn list_keys_with_prefix(&self, _prefix: &str) -> Result<Vec<String>, AppError> {
        Ok(vec![])
    }
}

/// Captures the CAS keys handed to `GcStorageKey`. This is the assertion that
/// matters: only the GC job deletes the *object*, so an enqueue that never
/// happens is an object that never goes.
struct RecordingJobs {
    collected: Mutex<Vec<String>>,
}

impl RecordingJobs {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            collected: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl JobQueue for RecordingJobs {
    async fn enqueue(&self, job: ForumJob) -> Result<(), AppError> {
        if let ForumJob::GcStorageKey { key } = job {
            self.collected.lock().unwrap().push(key);
        }
        Ok(())
    }
}

fn admin() -> ferum_domain::AuthUser {
    ferum_test_support::fixtures::AuthUserBuilder::admin().build()
}

fn use_case(
    themes: Arc<FakeThemes>,
    files: Arc<RecordingFiles>,
    jobs: Arc<RecordingJobs>,
) -> ThemeUseCase {
    ThemeUseCase::new(themes).with_cas(files, jobs, Arc::new(NoopStorageService))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn replacing_a_preview_releases_the_old_image() {
    let themes = FakeThemes::with_preview(Some("/files/theme-previews/old.jpg"));
    let files = RecordingFiles::new();
    let jobs = RecordingJobs::new();

    use_case(themes.clone(), files.clone(), jobs.clone())
        .set_preview(&admin(), "sumi", Some("/files/theme-previews/new.jpg".into()))
        .await
        .expect("set_preview must succeed");

    assert_eq!(
        files.decremented.lock().unwrap().as_slice(),
        ["theme-previews/old.jpg"],
        "the outgoing preview must give its reference back"
    );
    assert_eq!(
        jobs.collected.lock().unwrap().as_slice(),
        ["theme-previews/old.jpg"],
        "reaching zero must enqueue collection — nothing else deletes the object"
    );
    assert_eq!(
        themes.preview_writes.lock().unwrap().as_slice(),
        [Some("/files/theme-previews/new.jpg".to_string())]
    );
}

#[tokio::test]
async fn deleting_a_theme_releases_its_preview() {
    let themes = FakeThemes::with_preview(Some("/files/theme-previews/gone.jpg"));
    let files = RecordingFiles::new();
    let jobs = RecordingJobs::new();

    use_case(themes.clone(), files.clone(), jobs.clone())
        .delete(&admin(), "sumi")
        .await
        .expect("delete must succeed");

    assert_eq!(
        jobs.collected.lock().unwrap().as_slice(),
        ["theme-previews/gone.jpg"]
    );
    assert!(*themes.deleted.lock().unwrap(), "the theme must still be deleted");
}

#[tokio::test]
async fn a_theme_without_a_preview_releases_nothing() {
    // The common case — most themes never get a preview uploaded. Decrementing
    // a key derived from `None` would corrupt an unrelated file's count.
    let themes = FakeThemes::with_preview(None);
    let files = RecordingFiles::new();
    let jobs = RecordingJobs::new();

    use_case(themes.clone(), files.clone(), jobs.clone())
        .delete(&admin(), "sumi")
        .await
        .expect("delete must succeed");

    assert!(files.decremented.lock().unwrap().is_empty());
    assert!(jobs.collected.lock().unwrap().is_empty());
    assert!(*themes.deleted.lock().unwrap());
}

#[tokio::test]
async fn an_empty_preview_url_is_treated_as_absent() {
    // `update_preview_url(None)` writes NULL, but a row that went through an
    // older code path can hold "". `key_from_url("")` yields nothing, and
    // passing that on would decrement the empty key.
    let themes = FakeThemes::with_preview(Some(""));
    let files = RecordingFiles::new();
    let jobs = RecordingJobs::new();

    use_case(themes, files.clone(), jobs.clone())
        .delete(&admin(), "sumi")
        .await
        .expect("delete must succeed");

    assert!(files.decremented.lock().unwrap().is_empty());
    assert!(jobs.collected.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_use_case_built_without_cas_still_works() {
    // `with_cas` is optional, and the builders in the rest of this suite do not
    // call it. Deleting a theme must not depend on reference counting being
    // wired up.
    let themes = FakeThemes::with_preview(Some("/files/theme-previews/x.jpg"));

    ThemeUseCase::new(themes.clone())
        .delete(&admin(), "sumi")
        .await
        .expect("delete must succeed without CAS wiring");

    assert!(*themes.deleted.lock().unwrap());
}
