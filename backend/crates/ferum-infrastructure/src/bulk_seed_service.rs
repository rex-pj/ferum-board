//! Optional example data for the setup wizard checkbox — demo users, threads,
//! posts, catalogue. Behind the `bulk_seed` feature.
//!
//! Distinct from `system_seed_service`, which seeds what the app CANNOT run
//! without and runs on every startup. Nothing here is required.

use async_trait::async_trait;
use chrono::{Duration, Utc};
use sea_orm::{
    sea_query::{Expr, OnConflict},
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder,
};
use uuid::{uuid, Uuid};

use crate::entities::{
    categories, notifications, posts, reactions,
    // Codegen puts every PostgreSQL enum type in one module rather than beside
    // the table that happens to use it, since several are shared.
    sea_orm_active_enums::{
        NotificationKind, PostPolicy, ReactionKind, ThreadStatus, TrustLevel, ViewPolicy,
    },
    tags, thread_tags, threads, user_preferences, user_roles, users,
};
use ferum_application::ports::BulkSeedService;
use ferum_application::shared::AppError;

/// `table[n % table.len()]` — the cycling lookup every generator below uses.
///
/// `None` only for an empty table, which is also the `% 0` that would panic on
/// the *division* before any bounds check could speak. One call reports both
/// hazards as the same absence, and callers turn it into "generate one fewer
/// demo row" rather than into a panicked setup wizard.
fn cycle<T>(table: &[T], n: usize) -> Option<&T> {
    table.get(n.checked_rem(table.len())?)
}

// Fixed UUIDs for example seed users (moderator, alice, bob).
// The admin user's real ID is passed in at runtime.
const MOD_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000002");
const ALICE_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000003");
const BOB_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000004");

const CAT_GENERAL: Uuid = uuid!("10000000-0000-0000-0000-000000000001");
const CAT_INTRODUCTIONS: Uuid = uuid!("10000000-0000-0000-0000-000000000002");
const CAT_OFF_TOPIC: Uuid = uuid!("10000000-0000-0000-0000-000000000003");
const CAT_TECHNOLOGY: Uuid = uuid!("10000000-0000-0000-0000-000000000004");
const CAT_PROGRAMMING: Uuid = uuid!("10000000-0000-0000-0000-000000000005");
const CAT_HARDWARE: Uuid = uuid!("10000000-0000-0000-0000-000000000006");
const CAT_FEEDBACK: Uuid = uuid!("10000000-0000-0000-0000-000000000007");
const CAT_STAFF: Uuid = uuid!("10000000-0000-0000-0000-000000000008");

const TAG_RUST: Uuid      = uuid!("40000000-0000-0000-0000-000000000001");
const TAG_WEBDEV: Uuid    = uuid!("40000000-0000-0000-0000-000000000002");
const TAG_COMMUNITY: Uuid = uuid!("40000000-0000-0000-0000-000000000003");
const TAG_QUESTION: Uuid  = uuid!("40000000-0000-0000-0000-000000000004");

const TH_WELCOME: Uuid = uuid!("20000000-0000-0000-0000-000000000001");
const TH_INTRO: Uuid = uuid!("20000000-0000-0000-0000-000000000002");
const TH_FAV_LANG: Uuid = uuid!("20000000-0000-0000-0000-000000000003");
const TH_RUST_VS_GO: Uuid = uuid!("20000000-0000-0000-0000-000000000004");
const TH_DARK_MODE: Uuid = uuid!("20000000-0000-0000-0000-000000000005");

const POST_WELCOME_1: Uuid = uuid!("30000000-0000-0000-0000-000000000001");
const POST_WELCOME_2: Uuid = uuid!("30000000-0000-0000-0000-000000000002");
const POST_WELCOME_3: Uuid = uuid!("30000000-0000-0000-0000-000000000003");
const POST_INTRO_1: Uuid = uuid!("30000000-0000-0000-0000-000000000004");
const POST_INTRO_2: Uuid = uuid!("30000000-0000-0000-0000-000000000005");
const POST_FAV_LANG_1: Uuid = uuid!("30000000-0000-0000-0000-000000000006");
const POST_FAV_LANG_2: Uuid = uuid!("30000000-0000-0000-0000-000000000007");
const POST_FAV_LANG_3: Uuid = uuid!("30000000-0000-0000-0000-000000000008");
const POST_RUST_GO_1: Uuid = uuid!("30000000-0000-0000-0000-000000000009");
const POST_RUST_GO_2: Uuid = uuid!("30000000-0000-0000-0000-000000000010");
const POST_RUST_GO_3: Uuid = uuid!("30000000-0000-0000-0000-000000000011");
const POST_DARK_1: Uuid = uuid!("30000000-0000-0000-0000-000000000012");
const POST_DARK_2: Uuid = uuid!("30000000-0000-0000-0000-000000000013");

// Furniture-review demo (products + one full review) — showcases the catalog.
const P_SOFA: Uuid = uuid!("60000000-0000-0000-0000-000000000001");
const P_TABLE: Uuid = uuid!("60000000-0000-0000-0000-000000000002");
const P_MDF: Uuid = uuid!("60000000-0000-0000-0000-000000000003");
const BRAND_NHA_XINH: Uuid = uuid!("61000000-0000-0000-0000-000000000001");
const BRAND_HOA_PHAT: Uuid = uuid!("61000000-0000-0000-0000-000000000002");
const TH_SOFA_REVIEW: Uuid = uuid!("21000000-0000-0000-0000-000000000001");
const POST_SOFA_REVIEW: Uuid = uuid!("31000000-0000-0000-0000-000000000001");

// Moderation demo — fixed ids so re-running the seed cannot pile up duplicate
// reports on the same post.
const REPORT_SPAM: Uuid = uuid!("50000000-0000-0000-0000-000000000001");
const REPORT_RUDE: Uuid = uuid!("50000000-0000-0000-0000-000000000002");
const REPORT_THREAD: Uuid = uuid!("50000000-0000-0000-0000-000000000003");
const REPORT_DISMISSED: Uuid = uuid!("50000000-0000-0000-0000-000000000004");

const BULK_THREAD_TITLE_PREFIXES: [&str; 10] = [
    "What do you think about",
    "How to approach",
    "Discussion on",
    "Tips for",
    "Questions about",
    "Best practices for",
    "Comparing options in",
    "Getting started with",
    "Advanced topics in",
    "Community guide to",
];

const BULK_POST_BODIES: [&str; 10] = [
    "Great discussion! Adding my thoughts here.",
    "Interesting perspective. I agree with most points.",
    "Have you considered the alternative approach?",
    "Thanks for sharing this. Very helpful.",
    "I had a similar experience. Here is what worked for me.",
    "Could you elaborate on that point?",
    "This is exactly what I was looking for.",
    "I disagree with the premise here.",
    "Bumping this thread with an update.",
    "Has anyone else run into this issue?",
];

pub struct PgBulkSeedService {
    db: DatabaseConnection,
}

impl PgBulkSeedService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

/// Insert `rows` with ON CONFLICT DO NOTHING, tolerating the case where every
/// row conflicts (Sea-ORM returns `RecordNotInserted` in that case).
async fn insert_or_ignore<E, A, I>(db: &DatabaseConnection, rows: I) -> Result<(), AppError>
where
    E: EntityTrait,
    A: ActiveModelTrait<Entity = E> + Send,
    I: IntoIterator<Item = A>,
{
    match E::insert_many(rows)
        .on_conflict(OnConflict::new().do_nothing().to_owned())
        .exec(db)
        .await
    {
        Ok(_) | Err(DbErr::RecordNotInserted) => Ok(()),
        Err(e) => Err(AppError::internal(e.to_string())),
    }
}

async fn insert_in_chunks<E, A>(
    db: &DatabaseConnection,
    rows: Vec<A>,
    chunk_size: usize,
) -> Result<(), AppError>
where
    E: EntityTrait,
    A: ActiveModelTrait<Entity = E> + Send,
{
    for chunk in rows.chunks(chunk_size) {
        insert_or_ignore::<E, A, _>(db, chunk.to_vec()).await?;
    }
    Ok(())
}

#[async_trait]
impl BulkSeedService for PgBulkSeedService {
    async fn seed_bulk(&self, admin_id: Uuid) -> Result<(), AppError> {
        // cost 4 — fast enough for dev/test seeding
        let hash = bcrypt::hash("Ferum1234!", 4)
            .map_err(|e| AppError::internal(format!("bcrypt seed error: {e}")))?;

        self.seed_example_users(&hash).await?;
        self.seed_bulk_users(&hash).await?;
        self.seed_user_roles(admin_id).await?;
        self.seed_preferences(admin_id).await?;
        self.seed_categories().await?;
        self.seed_example_threads(admin_id).await?;
        self.seed_example_posts(admin_id).await?;
        self.seed_example_reactions(admin_id).await?;
        self.seed_example_tags(admin_id).await?;
        self.seed_bulk_threads().await?;
        self.seed_bulk_posts().await?;
        self.sync_bulk_thread_stats().await?;
        self.seed_bulk_reactions().await?;
        self.seed_bulk_notifications().await?;
        self.seed_materials().await?;
        self.seed_brands().await?;
        self.seed_furniture_demo(admin_id).await?;
        self.seed_moderation_demo(admin_id).await?;
        self.seed_social_demo(admin_id).await?;
        // Last: every post that counts towards a total has been written by now.
        self.sync_user_post_counts().await?;

        Ok(())
    }
}

impl PgBulkSeedService {
    // ── Example data (compile-time-typed via Sea-ORM ActiveModel) ─────────────

    async fn seed_example_users(&self, hash: &str) -> Result<(), AppError> {
        insert_or_ignore::<users::Entity, _, _>(
            &self.db,
            [
                users::ActiveModel {
                    id: Set(MOD_ID),
                    username: Set("moderator".into()),
                    email: Set("mod@ferum.local".into()),
                    is_email_verified: Set(true),
                    display_name: Set(Some("Moderator".into())),
                    password_hash: Set(Some(hash.into())),
                    trust_level: Set(TrustLevel::Member),
                    trust_score: Set(80),
                    // post_count is left at its default here and for every other
                    // seeded user: `sync_user_post_counts` derives it from the
                    // posts actually written, at the end of the run. A
                    // hand-maintained number goes stale the moment a seed step
                    // is added, and a profile that claims 5 posts above a list
                    // of 3 is a bug report waiting to happen.
                    ..Default::default()
                },
                users::ActiveModel {
                    id: Set(ALICE_ID),
                    username: Set("alice".into()),
                    email: Set("alice@ferum.local".into()),
                    is_email_verified: Set(true),
                    display_name: Set(Some("Alice".into())),
                    password_hash: Set(Some(hash.into())),
                    trust_level: Set(TrustLevel::Member),
                    trust_score: Set(40),
                    ..Default::default()
                },
                users::ActiveModel {
                    id: Set(BOB_ID),
                    username: Set("bob".into()),
                    email: Set("bob@ferum.local".into()),
                    is_email_verified: Set(true),
                    display_name: Set(Some("Bob".into())),
                    password_hash: Set(Some(hash.into())),
                    trust_level: Set(TrustLevel::Basic),
                    trust_score: Set(10),
                    ..Default::default()
                },
            ],
        )
        .await
    }

    async fn seed_categories(&self) -> Result<(), AppError> {
        insert_or_ignore::<categories::Entity, _, _>(
            &self.db,
            [
                categories::ActiveModel {
                    id: Set(CAT_GENERAL),
                    parent_id: Set(None),
                    slug: Set("general".into()),
                    name: Set("General Discussion".into()),
                    description: Set(Some(
                        "A place for general topics and community chat.".into(),
                    )),
                    position: Set(1),
                    view_policy: Set(ViewPolicy::Public),
                    post_policy: Set(PostPolicy::Members),
                    color: Set(Some("#0d6efd".into())),
                    ..Default::default()
                },
                categories::ActiveModel {
                    id: Set(CAT_INTRODUCTIONS),
                    parent_id: Set(Some(CAT_GENERAL)),
                    slug: Set("introductions".into()),
                    name: Set("Introductions".into()),
                    description: Set(Some("New here? Say hello!".into())),
                    position: Set(1),
                    view_policy: Set(ViewPolicy::Public),
                    post_policy: Set(PostPolicy::Members),
                    color: Set(Some("#198754".into())),
                    ..Default::default()
                },
                categories::ActiveModel {
                    id: Set(CAT_OFF_TOPIC),
                    parent_id: Set(Some(CAT_GENERAL)),
                    slug: Set("off-topic".into()),
                    name: Set("Off-Topic".into()),
                    description: Set(Some("Anything that does not fit elsewhere.".into())),
                    position: Set(2),
                    view_policy: Set(ViewPolicy::Public),
                    post_policy: Set(PostPolicy::Members),
                    color: Set(Some("#6c757d".into())),
                    ..Default::default()
                },
                categories::ActiveModel {
                    id: Set(CAT_TECHNOLOGY),
                    parent_id: Set(None),
                    slug: Set("technology".into()),
                    name: Set("Technology".into()),
                    description: Set(Some(
                        "Discussions about tech, software, and hardware.".into(),
                    )),
                    position: Set(2),
                    view_policy: Set(ViewPolicy::Public),
                    post_policy: Set(PostPolicy::Members),
                    color: Set(Some("#6f42c1".into())),
                    ..Default::default()
                },
                categories::ActiveModel {
                    id: Set(CAT_PROGRAMMING),
                    parent_id: Set(Some(CAT_TECHNOLOGY)),
                    slug: Set("programming".into()),
                    name: Set("Programming".into()),
                    description: Set(Some("Languages, frameworks, tools, and code.".into())),
                    position: Set(1),
                    view_policy: Set(ViewPolicy::Public),
                    post_policy: Set(PostPolicy::Members),
                    color: Set(Some("#0dcaf0".into())),
                    ..Default::default()
                },
                categories::ActiveModel {
                    id: Set(CAT_HARDWARE),
                    parent_id: Set(Some(CAT_TECHNOLOGY)),
                    slug: Set("hardware".into()),
                    name: Set("Hardware".into()),
                    description: Set(Some("CPUs, GPUs, peripherals, and builds.".into())),
                    position: Set(2),
                    view_policy: Set(ViewPolicy::Public),
                    post_policy: Set(PostPolicy::Members),
                    color: Set(Some("#fd7e14".into())),
                    ..Default::default()
                },
                categories::ActiveModel {
                    id: Set(CAT_FEEDBACK),
                    parent_id: Set(None),
                    slug: Set("site-feedback".into()),
                    name: Set("Site Feedback".into()),
                    description: Set(Some("Report bugs and suggest improvements.".into())),
                    position: Set(3),
                    view_policy: Set(ViewPolicy::Public),
                    post_policy: Set(PostPolicy::Members),
                    color: Set(Some("#ffc107".into())),
                    ..Default::default()
                },
                categories::ActiveModel {
                    id: Set(CAT_STAFF),
                    parent_id: Set(None),
                    slug: Set("staff-only".into()),
                    name: Set("Staff Only".into()),
                    description: Set(Some("Internal staff coordination.".into())),
                    position: Set(4),
                    view_policy: Set(ViewPolicy::StaffOnly),
                    post_policy: Set(PostPolicy::StaffOnly),
                    color: Set(Some("#dc3545".into())),
                    ..Default::default()
                },
            ],
        )
        .await
    }

    async fn seed_example_threads(&self, admin_id: Uuid) -> Result<(), AppError> {
        let now = Utc::now().fixed_offset();
        insert_or_ignore::<threads::Entity, _, _>(
            &self.db,
            [
                threads::ActiveModel {
                    id: Set(TH_WELCOME),
                    category_id: Set(CAT_GENERAL),
                    author_id: Set(admin_id),
                    title: Set("Welcome to Ferum Board!".into()),
                    slug: Set("welcome-to-ferum-board".into()),
                    status: Set(ThreadStatus::Open),
                    is_pinned: Set(true),
                    reply_count: Set(2),
                    view_count: Set(120),
                    last_post_at: Set(Some(now - Duration::hours(1))),
                    custom_fields: Set(serde_json::Value::Object(Default::default())),
                    ..Default::default()
                },
                threads::ActiveModel {
                    id: Set(TH_INTRO),
                    category_id: Set(CAT_INTRODUCTIONS),
                    author_id: Set(MOD_ID),
                    title: Set("Introduce yourself here!".into()),
                    slug: Set("introduce-yourself-here".into()),
                    status: Set(ThreadStatus::Open),
                    is_pinned: Set(true),
                    reply_count: Set(1),
                    view_count: Set(54),
                    last_post_at: Set(Some(now - Duration::hours(2))),
                    custom_fields: Set(serde_json::Value::Object(Default::default())),
                    ..Default::default()
                },
                threads::ActiveModel {
                    id: Set(TH_FAV_LANG),
                    category_id: Set(CAT_PROGRAMMING),
                    author_id: Set(ALICE_ID),
                    title: Set("What is your favorite programming language?".into()),
                    slug: Set("what-is-your-favorite-programming-language".into()),
                    status: Set(ThreadStatus::Open),
                    is_pinned: Set(false),
                    reply_count: Set(2),
                    view_count: Set(87),
                    last_post_at: Set(Some(now - Duration::hours(3))),
                    custom_fields: Set(serde_json::Value::Object(Default::default())),
                    ..Default::default()
                },
                threads::ActiveModel {
                    id: Set(TH_RUST_VS_GO),
                    category_id: Set(CAT_PROGRAMMING),
                    author_id: Set(BOB_ID),
                    title: Set("Rust vs Go for backend development".into()),
                    slug: Set("rust-vs-go-for-backend-development".into()),
                    status: Set(ThreadStatus::Open),
                    is_pinned: Set(false),
                    reply_count: Set(2),
                    view_count: Set(210),
                    last_post_at: Set(Some(now - Duration::hours(5))),
                    custom_fields: Set(serde_json::Value::Object(Default::default())),
                    ..Default::default()
                },
                threads::ActiveModel {
                    id: Set(TH_DARK_MODE),
                    category_id: Set(CAT_FEEDBACK),
                    author_id: Set(ALICE_ID),
                    title: Set("Feature request: dark mode".into()),
                    slug: Set("feature-request-dark-mode".into()),
                    status: Set(ThreadStatus::Open),
                    is_pinned: Set(false),
                    reply_count: Set(1),
                    view_count: Set(33),
                    last_post_at: Set(Some(now - Duration::hours(6))),
                    custom_fields: Set(serde_json::Value::Object(Default::default())),
                    ..Default::default()
                },
            ],
        )
        .await
    }

    async fn seed_example_posts(&self, admin_id: Uuid) -> Result<(), AppError> {
        let now = Utc::now().fixed_offset();
        insert_or_ignore::<posts::Entity, _, _>(&self.db, [
            posts::ActiveModel {
                id:           Set(POST_WELCOME_1),
                thread_id:    Set(TH_WELCOME),
                author_id:    Set(admin_id),
                parent_id:    Set(None),
                content_md:   Set("## Welcome to Ferum Board!\n\nThis forum is built with **Rust**, **Axum**, and **Tera** on the backend, with Svelte Web Components for interactive islands.".into()),
                content_html: Set("<h2>Welcome to Ferum Board!</h2><p>This forum is built with <strong>Rust</strong>, <strong>Axum</strong>, and <strong>Tera</strong> on the backend, with Svelte Web Components for interactive islands.</p>".into()),
                created_at:   Set(now - Duration::days(4)),
                ..Default::default()
            },
            posts::ActiveModel {
                id:           Set(POST_WELCOME_2),
                thread_id:    Set(TH_WELCOME),
                author_id:    Set(ALICE_ID),
                parent_id:    Set(None),
                content_md:   Set("Thanks for setting this up! The interface looks great.".into()),
                content_html: Set("<p>Thanks for setting this up! The interface looks great.</p>".into()),
                created_at:   Set(now - Duration::days(3)),
                ..Default::default()
            },
            posts::ActiveModel {
                id:           Set(POST_WELCOME_3),
                thread_id:    Set(TH_WELCOME),
                author_id:    Set(BOB_ID),
                parent_id:    Set(None),
                content_md:   Set("Really impressed with how fast it loads. Rust backend is doing its job.".into()),
                content_html: Set("<p>Really impressed with how fast it loads. Rust backend is doing its job.</p>".into()),
                created_at:   Set(now - Duration::days(2)),
                ..Default::default()
            },
            posts::ActiveModel {
                id:           Set(POST_INTRO_1),
                thread_id:    Set(TH_INTRO),
                author_id:    Set(MOD_ID),
                parent_id:    Set(None),
                content_md:   Set("Hi everyone! I am the community moderator. Drop a reply with your name.".into()),
                content_html: Set("<p>Hi everyone! I am the community moderator. Drop a reply with your name.</p>".into()),
                created_at:   Set(now - Duration::days(3)),
                ..Default::default()
            },
            posts::ActiveModel {
                id:           Set(POST_INTRO_2),
                thread_id:    Set(TH_INTRO),
                author_id:    Set(ALICE_ID),
                parent_id:    Set(None),
                content_md:   Set("Hey! I am Alice, a backend developer interested in Rust.".into()),
                content_html: Set("<p>Hey! I am Alice, a backend developer interested in Rust.</p>".into()),
                created_at:   Set(now - Duration::days(2)),
                ..Default::default()
            },
            posts::ActiveModel {
                id:           Set(POST_FAV_LANG_1),
                thread_id:    Set(TH_FAV_LANG),
                author_id:    Set(ALICE_ID),
                parent_id:    Set(None),
                content_md:   Set("I have been using **Rust** for about two years and I cannot imagine going back.".into()),
                content_html: Set("<p>I have been using <strong>Rust</strong> for about two years and I cannot imagine going back.</p>".into()),
                created_at:   Set(now - Duration::days(2)),
                ..Default::default()
            },
            posts::ActiveModel {
                id:           Set(POST_FAV_LANG_2),
                thread_id:    Set(TH_FAV_LANG),
                author_id:    Set(BOB_ID),
                parent_id:    Set(None),
                content_md:   Set("Python for data science tasks, Rust for systems work.".into()),
                content_html: Set("<p>Python for data science tasks, Rust for systems work.</p>".into()),
                created_at:   Set(now - Duration::days(1)),
                ..Default::default()
            },
            posts::ActiveModel {
                id:           Set(POST_FAV_LANG_3),
                thread_id:    Set(TH_FAV_LANG),
                author_id:    Set(MOD_ID),
                parent_id:    Set(None),
                content_md:   Set("TypeScript is my daily driver for web work.".into()),
                content_html: Set("<p>TypeScript is my daily driver for web work.</p>".into()),
                created_at:   Set(now - Duration::hours(12)),
                ..Default::default()
            },
            posts::ActiveModel {
                id:           Set(POST_RUST_GO_1),
                thread_id:    Set(TH_RUST_VS_GO),
                author_id:    Set(BOB_ID),
                parent_id:    Set(None),
                content_md:   Set("Both are great but I keep going back to **Go** for greenfield projects.".into()),
                content_html: Set("<p>Both are great but I keep going back to <strong>Go</strong> for greenfield projects.</p>".into()),
                created_at:   Set(now - Duration::hours(5)),
                ..Default::default()
            },
            posts::ActiveModel {
                id:           Set(POST_RUST_GO_2),
                thread_id:    Set(TH_RUST_VS_GO),
                author_id:    Set(ALICE_ID),
                parent_id:    Set(None),
                content_md:   Set("Rust is worth the learning curve. Memory safety without a GC matters at scale.".into()),
                content_html: Set("<p>Rust is worth the learning curve. Memory safety without a GC matters at scale.</p>".into()),
                created_at:   Set(now - Duration::hours(4)),
                ..Default::default()
            },
            posts::ActiveModel {
                id:           Set(POST_RUST_GO_3),
                thread_id:    Set(TH_RUST_VS_GO),
                author_id:    Set(admin_id),
                parent_id:    Set(None),
                content_md:   Set("This very forum backend is written in Rust with Axum.".into()),
                content_html: Set("<p>This very forum backend is written in Rust with Axum.</p>".into()),
                created_at:   Set(now - Duration::hours(2)),
                ..Default::default()
            },
            posts::ActiveModel {
                id:           Set(POST_DARK_1),
                thread_id:    Set(TH_DARK_MODE),
                author_id:    Set(ALICE_ID),
                parent_id:    Set(None),
                content_md:   Set("It would be great to have a dark mode option. Bootstrap 5 supports it natively.".into()),
                content_html: Set("<p>It would be great to have a dark mode option. Bootstrap 5 supports it natively.</p>".into()),
                created_at:   Set(now - Duration::hours(6)),
                ..Default::default()
            },
            posts::ActiveModel {
                id:           Set(POST_DARK_2),
                thread_id:    Set(TH_DARK_MODE),
                author_id:    Set(admin_id),
                parent_id:    Set(None),
                content_md:   Set("Great suggestion! Added to the roadmap.".into()),
                content_html: Set("<p>Great suggestion! Added to the roadmap.</p>".into()),
                created_at:   Set(now - Duration::hours(5)),
                ..Default::default()
            },
        ]).await
    }

    async fn seed_example_reactions(&self, admin_id: Uuid) -> Result<(), AppError> {
        insert_or_ignore::<reactions::Entity, _, _>(
            &self.db,
            [
                reactions::ActiveModel {
                    post_id: Set(POST_WELCOME_1),
                    user_id: Set(ALICE_ID),
                    kind: Set(ReactionKind::Like),
                    ..Default::default()
                },
                reactions::ActiveModel {
                    post_id: Set(POST_WELCOME_1),
                    user_id: Set(BOB_ID),
                    kind: Set(ReactionKind::Helpful),
                    ..Default::default()
                },
                reactions::ActiveModel {
                    post_id: Set(POST_WELCOME_2),
                    user_id: Set(admin_id),
                    kind: Set(ReactionKind::Like),
                    ..Default::default()
                },
                reactions::ActiveModel {
                    post_id: Set(POST_RUST_GO_1),
                    user_id: Set(ALICE_ID),
                    kind: Set(ReactionKind::Insightful),
                    ..Default::default()
                },
                reactions::ActiveModel {
                    post_id: Set(POST_RUST_GO_2),
                    user_id: Set(BOB_ID),
                    kind: Set(ReactionKind::Like),
                    ..Default::default()
                },
                reactions::ActiveModel {
                    post_id: Set(POST_DARK_1),
                    user_id: Set(MOD_ID),
                    kind: Set(ReactionKind::Like),
                    ..Default::default()
                },
                reactions::ActiveModel {
                    post_id: Set(POST_FAV_LANG_1),
                    user_id: Set(BOB_ID),
                    kind: Set(ReactionKind::Helpful),
                    ..Default::default()
                },
            ],
        )
        .await
    }

    async fn seed_user_roles(&self, admin_id: Uuid) -> Result<(), AppError> {
        use crate::entities::roles;

        // Fetch role IDs from DB (written by PgSystemSeedService at startup)
        let all_roles = roles::Entity::find()
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;

        let role_id = |slug: &str| -> Result<Uuid, AppError> {
            all_roles
                .iter()
                .find(|r| r.slug == slug)
                .map(|r| r.id)
                .ok_or_else(|| AppError::internal(format!("role '{slug}' not found — run migrations first")))
        };

        let mod_role_id = role_id("moderator")?;
        let member_role_id = role_id("member")?;

        // Example named users
        insert_or_ignore::<user_roles::Entity, _, _>(&self.db, [
            user_roles::ActiveModel {
                user_id: Set(MOD_ID),
                role_id: Set(mod_role_id),
                category_id: Set(None),
                granted_by: Set(Some(admin_id)),
                ..Default::default()
            },
            user_roles::ActiveModel {
                user_id: Set(ALICE_ID),
                role_id: Set(member_role_id),
                category_id: Set(None),
                granted_by: Set(Some(admin_id)),
                ..Default::default()
            },
            user_roles::ActiveModel {
                user_id: Set(BOB_ID),
                role_id: Set(member_role_id),
                category_id: Set(None),
                granted_by: Set(Some(admin_id)),
                ..Default::default()
            },
        ]).await?;

        // Bulk users — assign member role
        let bulk_user_ids: Vec<Uuid> = users::Entity::find()
            .filter(users::Column::Username.like("user_%"))
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .into_iter()
            .map(|u| u.id)
            .collect();

        let rows: Vec<user_roles::ActiveModel> = bulk_user_ids
            .into_iter()
            .map(|uid| user_roles::ActiveModel {
                user_id: Set(uid),
                role_id: Set(member_role_id),
                category_id: Set(None),
                granted_by: Set(Some(admin_id)),
                ..Default::default()
            })
            .collect();

        insert_in_chunks::<user_roles::Entity, _>(&self.db, rows, 200).await
    }

    async fn seed_example_tags(&self, admin_id: Uuid) -> Result<(), AppError> {
        insert_or_ignore::<tags::Entity, _, _>(&self.db, [
            tags::ActiveModel {
                id: Set(TAG_RUST),
                name: Set("rust".into()),
                slug: Set("rust".into()),
                color: Set(Some("#f74c00".into())),
                created_by_id: Set(Some(admin_id)),
                ..Default::default()
            },
            tags::ActiveModel {
                id: Set(TAG_WEBDEV),
                name: Set("webdev".into()),
                slug: Set("webdev".into()),
                color: Set(Some("#0d6efd".into())),
                created_by_id: Set(Some(admin_id)),
                ..Default::default()
            },
            tags::ActiveModel {
                id: Set(TAG_COMMUNITY),
                name: Set("community".into()),
                slug: Set("community".into()),
                color: Set(Some("#198754".into())),
                created_by_id: Set(Some(admin_id)),
                ..Default::default()
            },
            tags::ActiveModel {
                id: Set(TAG_QUESTION),
                name: Set("question".into()),
                slug: Set("question".into()),
                color: Set(Some("#6f42c1".into())),
                created_by_id: Set(Some(admin_id)),
                ..Default::default()
            },
        ]).await?;

        insert_or_ignore::<thread_tags::Entity, _, _>(&self.db, [
            // Welcome thread → community
            thread_tags::ActiveModel { thread_id: Set(TH_WELCOME),   tag_id: Set(TAG_COMMUNITY) },
            // Introductions → community
            thread_tags::ActiveModel { thread_id: Set(TH_INTRO),     tag_id: Set(TAG_COMMUNITY) },
            // Favourite language → question, rust
            thread_tags::ActiveModel { thread_id: Set(TH_FAV_LANG),  tag_id: Set(TAG_QUESTION)  },
            thread_tags::ActiveModel { thread_id: Set(TH_FAV_LANG),  tag_id: Set(TAG_RUST)      },
            // Rust vs Go → rust, question
            thread_tags::ActiveModel { thread_id: Set(TH_RUST_VS_GO), tag_id: Set(TAG_RUST)     },
            thread_tags::ActiveModel { thread_id: Set(TH_RUST_VS_GO), tag_id: Set(TAG_QUESTION) },
            // Dark mode → webdev
            thread_tags::ActiveModel { thread_id: Set(TH_DARK_MODE), tag_id: Set(TAG_WEBDEV)    },
        ]).await
    }

    // ── Bulk generation (Rust-generated rows, queried from DB for FK lookups) ──

    async fn seed_bulk_users(&self, hash: &str) -> Result<(), AppError> {
        let trust_levels = [
            TrustLevel::New,
            TrustLevel::Basic,
            TrustLevel::Basic,
            TrustLevel::Member,
            TrustLevel::Member,
        ];

        let rows: Vec<users::ActiveModel> = (1usize..=1000)
            .filter_map(|i| Some(users::ActiveModel {
                username: Set(format!("user_{i}")),
                email: Set(format!("user_{i}@ferum.local")),
                is_email_verified: Set(true),
                display_name: Set(Some(format!("User {i}"))),
                password_hash: Set(Some(hash.into())),
                trust_level: Set(cycle(&trust_levels, i - 1)?.clone()),
                trust_score: Set(((i * 7) % 100) as i32),
                ..Default::default()
            }))
            .collect();

        insert_in_chunks::<users::Entity, _>(&self.db, rows, 200).await
    }

    async fn seed_preferences(&self, admin_id: Uuid) -> Result<(), AppError> {
        let bulk_ids: Vec<Uuid> = users::Entity::find()
            .filter(users::Column::Username.like("user_%"))
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .into_iter()
            .map(|u| u.id)
            .collect();

        let rows: Vec<user_preferences::ActiveModel> = [admin_id, MOD_ID, ALICE_ID, BOB_ID]
            .into_iter()
            .chain(bulk_ids)
            .map(|uid| user_preferences::ActiveModel {
                user_id: Set(uid),
                ..Default::default()
            })
            .collect();

        insert_in_chunks::<user_preferences::Entity, _>(&self.db, rows, 200).await
    }

    async fn seed_bulk_threads(&self) -> Result<(), AppError> {
        let pub_cat_ids: Vec<Uuid> = categories::Entity::find()
            .filter(categories::Column::ViewPolicy.eq(ViewPolicy::Public))
            .order_by_asc(categories::Column::Position)
            .order_by_asc(categories::Column::Id)
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .into_iter()
            .map(|c| c.id)
            .collect();

        let all_user_ids: Vec<Uuid> = users::Entity::find()
            .order_by_asc(users::Column::CreatedAt)
            .order_by_asc(users::Column::Id)
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .into_iter()
            .map(|u| u.id)
            .collect();

        // `cycle` would return `None` for either of these and silently seed zero
        // rows. Say why instead — an empty forum after ticking the wizard's box
        // is exactly the moment an operator needs a log line.
        if pub_cat_ids.is_empty() || all_user_ids.is_empty() {
            tracing::warn!("no seeded categories or users found; skipping bulk threads");
            return Ok(());
        }
        let statuses = [
            ThreadStatus::Open,
            ThreadStatus::Open,
            ThreadStatus::Open,
            ThreadStatus::Open,
            ThreadStatus::Locked,
        ];
        let now = Utc::now().fixed_offset();

        let rows: Vec<threads::ActiveModel> = (1usize..=5000)
            .filter_map(|i| {
                let ts = now - Duration::minutes((i * 2) as i64);
                Some(threads::ActiveModel {
                    category_id: Set(*cycle(&pub_cat_ids, i - 1)?),
                    author_id: Set(*cycle(&all_user_ids, i - 1)?),
                    title: Set(format!(
                        "{} topic {}",
                        cycle(&BULK_THREAD_TITLE_PREFIXES, i - 1)?,
                        (i - 1) % 50 + 1
                    )),
                    slug: Set(format!("bulk-thread-{i}")),
                    status: Set(cycle(&statuses, i - 1)?.clone()),
                    view_count: Set(((i * 13) % 900) as i32),
                    reply_count: Set(0),
                    last_post_at: Set(Some(ts)),
                    created_at: Set(ts),
                    custom_fields: Set(serde_json::Value::Object(Default::default())),
                    ..Default::default()
                })
            })
            .collect();

        insert_in_chunks::<threads::Entity, _>(&self.db, rows, 200).await
    }

    async fn seed_bulk_posts(&self) -> Result<(), AppError> {
        // Fetch (id, author_id) so the opening post of each thread can be by the thread author.
        let bulk_threads: Vec<(Uuid, Uuid)> = threads::Entity::find()
            .filter(threads::Column::Slug.like("bulk-thread-%"))
            .order_by_asc(threads::Column::CreatedAt)
            .order_by_asc(threads::Column::Id)
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .into_iter()
            .map(|t| (t.id, t.author_id))
            .collect();

        let all_user_ids: Vec<Uuid> = users::Entity::find()
            .order_by_asc(users::Column::CreatedAt)
            .order_by_asc(users::Column::Id)
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .into_iter()
            .map(|u| u.id)
            .collect();

        if all_user_ids.is_empty() {
            tracing::warn!("no seeded users found; skipping bulk posts");
            return Ok(());
        }
        let posts_per_thread: usize = 18;
        let now = Utc::now().fixed_offset();

        let mut rows: Vec<posts::ActiveModel> = Vec::with_capacity(bulk_threads.len() * posts_per_thread);
        let nt = bulk_threads.len();

        for (ti, (thread_id, thread_author_id)) in bulk_threads.iter().enumerate() {
            // bulk_threads is ordered ASC by created_at, so ti=0 is the oldest thread.
            // Allocate a block of (posts_per_thread + 2) minutes per thread so that
            // posts from different threads never interleave.  Within each block, pi=0
            // (the opening post) is the earliest timestamp and pi increases toward now,
            // ensuring the opening post always sorts first when ORDER BY created_at ASC.
            let thread_base_minutes = (nt - 1 - ti) as i64 * (posts_per_thread as i64 + 2);

            for pi in 0..posts_per_thread {
                let i = ti * posts_per_thread + pi + 1;
                // Post #0 within a thread is the opening post — always by the thread author.
                let author_id = if pi == 0 {
                    *thread_author_id
                } else {
                    match cycle(&all_user_ids, i - 1) {
                        Some(id) => *id,
                        None => continue,
                    }
                };
                let Some(body) = cycle(&BULK_POST_BODIES, i - 1) else {
                    continue;
                };
                let content = if pi == 0 {
                    format!("This is the opening post for this discussion. {body}")
                } else {
                    format!("Post #{i} — {body}")
                };
                // pi=0 → thread_base_minutes + posts_per_thread (oldest within thread)
                // pi=17 → thread_base_minutes + 1            (newest within thread)
                let ts = now - Duration::minutes(thread_base_minutes + posts_per_thread as i64 - pi as i64);
                rows.push(posts::ActiveModel {
                    thread_id: Set(*thread_id),
                    author_id: Set(author_id),
                    parent_id: Set(None),
                    content_md: Set(content.clone()),
                    content_html: Set(format!("<p>{content}</p>")),
                    created_at: Set(ts),
                    ..Default::default()
                });
            }
        }

        insert_in_chunks::<posts::Entity, _>(&self.db, rows, 500).await
    }

    async fn sync_bulk_thread_stats(&self) -> Result<(), AppError> {
        use std::collections::HashMap;

        let all_posts = posts::Entity::find()
            .filter(
                posts::Column::ThreadId.in_subquery(
                    sea_orm::sea_query::Query::select()
                        .column(threads::Column::Id)
                        .from(threads::Entity)
                        .and_where(threads::Column::Slug.like("bulk-thread-%"))
                        .to_owned(),
                ),
            )
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;

        // Aggregate count and max(created_at) per thread_id in Rust.
        let mut stats: HashMap<Uuid, (i64, chrono::DateTime<chrono::FixedOffset>)> = HashMap::new();
        for post in all_posts {
            let entry = stats.entry(post.thread_id).or_insert((0, post.created_at));
            entry.0 += 1;
            if post.created_at > entry.1 {
                entry.1 = post.created_at;
            }
        }

        for (thread_id, (count, last_at)) in stats {
            threads::Entity::update_many()
                .col_expr(
                    threads::Column::ReplyCount,
                    Expr::value((count - 1).max(0) as i32),
                )
                .col_expr(threads::Column::LastPostAt, Expr::value(last_at))
                .filter(threads::Column::Id.eq(thread_id))
                .exec(&self.db)
                .await
                .map_err(|e| AppError::internal(e.to_string()))?;
        }

        Ok(())
    }

    async fn seed_bulk_reactions(&self) -> Result<(), AppError> {
        let bulk_post_ids: Vec<Uuid> = posts::Entity::find()
            .filter(
                posts::Column::ThreadId.in_subquery(
                    sea_orm::sea_query::Query::select()
                        .column(threads::Column::Id)
                        .from(threads::Entity)
                        .and_where(threads::Column::Slug.like("bulk-thread-%"))
                        .to_owned(),
                ),
            )
            .order_by_asc(posts::Column::CreatedAt)
            .order_by_asc(posts::Column::Id)
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .into_iter()
            .map(|p| p.id)
            .collect();

        let all_user_ids: Vec<Uuid> = users::Entity::find()
            .order_by_asc(users::Column::CreatedAt)
            .order_by_asc(users::Column::Id)
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .into_iter()
            .map(|u| u.id)
            .collect();

        if bulk_post_ids.is_empty() || all_user_ids.is_empty() {
            tracing::warn!("no seeded posts or users found; skipping bulk reactions");
            return Ok(());
        }
        let kinds = [
            ReactionKind::Like,
            ReactionKind::Helpful,
            ReactionKind::Insightful,
            ReactionKind::Funny,
        ];

        let mut seen: std::collections::HashSet<(Uuid, Uuid, u8)> = std::collections::HashSet::new();
        let rows: Vec<reactions::ActiveModel> = (1usize..=5000)
            .filter_map(|i| {
                let post_id = *cycle(&bulk_post_ids, i * 7)?;
                let user_id = *cycle(&all_user_ids, i * 11)?;
                let kind_idx = ((i - 1) % 4) as u8;
                let kind = cycle(&kinds, i - 1)?.clone();
                if seen.insert((post_id, user_id, kind_idx)) {
                    Some(reactions::ActiveModel {
                        post_id: Set(post_id),
                        user_id: Set(user_id),
                        kind: Set(kind),
                        ..Default::default()
                    })
                } else {
                    None
                }
            })
            .collect();

        insert_in_chunks::<reactions::Entity, _>(&self.db, rows, 200).await
    }

    async fn seed_bulk_notifications(&self) -> Result<(), AppError> {
        let all_user_ids: Vec<Uuid> = users::Entity::find()
            .order_by_asc(users::Column::CreatedAt)
            .order_by_asc(users::Column::Id)
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .into_iter()
            .map(|u| u.id)
            .collect();

        if all_user_ids.is_empty() {
            tracing::warn!("no seeded users found; skipping bulk notifications");
            return Ok(());
        }
        let kinds = [
            NotificationKind::Reply,
            NotificationKind::Mention,
            NotificationKind::Reaction,
            NotificationKind::System,
        ];

        // Realistic thread refs drawn from the example threads seeded above.
        let thread_refs: [(&str, &str); 5] = [
            ("welcome-to-ferum-board",                      "Welcome to Ferum Board!"),
            ("introduce-yourself-here",                     "Introduce yourself here!"),
            ("what-is-your-favorite-programming-language",  "What is your favorite programming language?"),
            ("rust-vs-go-for-backend-development",          "Rust vs Go for backend development"),
            ("feature-request-dark-mode",                   "Feature request: dark mode"),
        ];
        let actor_names: [&str; 3] = ["alice", "bob", "moderator"];
        let reaction_names: [&str; 4] = ["like", "helpful", "insightful", "funny"];

        let rows: Vec<notifications::ActiveModel> = (1usize..=2000)
            .filter_map(|i| {
                let kind = cycle(&kinds, i - 1)?.clone();
                let (thread_slug, thread_title) = *cycle(&thread_refs, i)?;
                let actor = *cycle(&actor_names, i)?;
                let payload = match kind {
                    NotificationKind::Reply | NotificationKind::Mention => serde_json::json!({
                        "thread_slug":   thread_slug,
                        "thread_title":  thread_title,
                        "actor_username": actor,
                    }),
                    NotificationKind::Reaction => serde_json::json!({
                        "thread_slug":   thread_slug,
                        "thread_title":  thread_title,
                        "actor_username": actor,
                        "reaction":      cycle(&reaction_names, i)?,
                    }),
                    // System notifications are used for "user followed you" events.
                    _ => serde_json::json!({ "actor_username": actor }),
                };
                Some(notifications::ActiveModel {
                    user_id: Set(*cycle(&all_user_ids, i * 3)?),
                    kind: Set(kind),
                    payload: Set(payload),
                    is_read: Set(i % 3 == 0),
                    ..Default::default()
                })
            })
            .collect();

        insert_in_chunks::<notifications::Entity, _>(&self.db, rows, 200).await
    }

    // ── Reference materials common in the Vietnamese furniture market ──────────
    // Opt-in starter taxonomy (seeded only when the admin chooses example data).
    // Production installs start empty and add materials via the admin UI.

    async fn seed_materials(&self) -> Result<(), AppError> {
        use crate::entities::materials;

        let rows: [(&str, &str, &str); 20] = [
            ("go-soi", "Gỗ sồi tự nhiên", "wood_natural"),
            ("go-oc-cho", "Gỗ óc chó", "wood_natural"),
            ("go-cao-su", "Gỗ cao su", "wood_natural"),
            ("go-thong", "Gỗ thông", "wood_natural"),
            ("go-xoan-dao", "Gỗ xoan đào", "wood_natural"),
            ("mdf-melamine", "MDF phủ Melamine", "wood_engineered"),
            ("mdf-laminate", "MDF phủ Laminate", "wood_engineered"),
            ("mdf-veneer", "MDF phủ Veneer", "wood_engineered"),
            ("hdf", "Gỗ HDF", "wood_engineered"),
            ("plywood", "Gỗ dán (Plywood)", "wood_engineered"),
            ("may-tre-dan", "Mây tre đan", "rattan_bamboo"),
            ("kim-loai-son", "Kim loại sơn tĩnh điện", "metal"),
            ("inox", "Inox (thép không gỉ)", "metal"),
            ("vai-ni", "Vải nỉ", "fabric"),
            ("vai-bo", "Vải bố (canvas)", "fabric"),
            ("da-that", "Da thật", "leather"),
            ("da-simili", "Da công nghiệp (simili)", "leather"),
            ("da-marble", "Đá marble", "stone"),
            ("kinh-cuong-luc", "Kính cường lực", "glass"),
            ("nhua-pp", "Nhựa PP", "plastic"),
        ];

        let models: Vec<materials::ActiveModel> = rows
            .iter()
            .map(|(slug, name, category)| materials::ActiveModel {
                slug: Set((*slug).to_string()),
                name: Set((*name).to_string()),
                category: Set((*category).to_string()),
                ..Default::default()
            })
            .collect();

        insert_or_ignore::<materials::Entity, _, _>(&self.db, models).await
    }

    async fn seed_brands(&self) -> Result<(), AppError> {
        use crate::entities::brands;

        // Fixed IDs so the demo products below can reference them.
        let rows: [(Uuid, &str, &str, &str, &str); 5] = [
            (BRAND_NHA_XINH, "nha-xinh", "Nhà Xinh", "Việt Nam", "Nội thất cao cấp phong cách hiện đại."),
            (BRAND_HOA_PHAT, "noi-that-hoa-phat", "Nội thất Hòa Phát", "Việt Nam", "Nội thất văn phòng và gia đình phổ thông."),
            (uuid!("61000000-0000-0000-0000-000000000003"), "an-cuong", "An Cường", "Việt Nam", "Gỗ công nghiệp và vật liệu bề mặt."),
            (uuid!("61000000-0000-0000-0000-000000000004"), "xuan-hoa", "Xuân Hòa", "Việt Nam", "Nội thất kim loại và gia dụng."),
            (uuid!("61000000-0000-0000-0000-000000000005"), "baya", "BAYA", "Việt Nam", "Nội thất thiết kế theo phong cách Bắc Âu."),
        ];

        let models: Vec<brands::ActiveModel> = rows
            .iter()
            .map(|(id, slug, name, country, description)| brands::ActiveModel {
                id: Set(*id),
                slug: Set((*slug).to_string()),
                name: Set((*name).to_string()),
                country: Set(Some((*country).to_string())),
                description: Set(Some((*description).to_string())),
                is_verified: Set(true),
                // `tier` left unset → DB default ('free'); allowed set is free|sponsored.
                ..Default::default()
            })
            .collect();

        insert_or_ignore::<brands::Entity, _, _>(&self.db, models).await
    }

    // ── Furniture-review demo (catalog + one full review with aggregate stats) ──

    async fn seed_furniture_demo(&self, admin_id: Uuid) -> Result<(), AppError> {
        use crate::entities::{
            materials, product_categories, product_materials, product_rating_stats, products,
            review_ratings, sea_orm_active_enums,
        };
        use rust_decimal::Decimal;

        let now = Utc::now().fixed_offset();

        // Catalogue categories come from PgSystemSeedService, which has already
        // run by the time setup calls this. Looked up by slug rather than by a
        // fixed UUID because an admin may have edited the taxonomy first.
        let pcat: std::collections::HashMap<String, Uuid> = product_categories::Entity::find()
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .into_iter()
            .map(|c| (c.slug, c.id))
            .collect();
        let cat = |slug: &str| pcat.get(slug).copied();

        // 1. Products (VN prices in đồng).
        insert_or_ignore::<products::Entity, _, _>(&self.db, [
            products::ActiveModel {
                id: Set(P_SOFA),
                slug: Set("sofa-vang-boc-ni-scandinavian".into()),
                name: Set("Sofa văng bọc nỉ Scandinavian".into()),
                product_type: Set(sea_orm_active_enums::ProductType::Furniture),
                status: Set(sea_orm_active_enums::ProductStatus::Published),
                brand_id: Set(Some(BRAND_NHA_XINH)),
                category_id: Set(cat("sofa")),
                style: Set(Some("Scandinavian".into())),
                price_min: Set(Some(6_500_000)),
                price_max: Set(Some(8_900_000)),
                origin: Set(Some("Việt Nam".into())),
                description_md: Set(Some("Sofa văng khung gỗ sồi, đệm bọc nỉ, dài 1m8.".into())),
                created_by_id: Set(Some(admin_id)),
                ..Default::default()
            },
            products::ActiveModel {
                id: Set(P_TABLE),
                slug: Set("ban-an-go-oc-cho-6-ghe".into()),
                name: Set("Bàn ăn gỗ óc chó 6 ghế".into()),
                product_type: Set(sea_orm_active_enums::ProductType::Furniture),
                status: Set(sea_orm_active_enums::ProductStatus::Published),
                brand_id: Set(Some(BRAND_HOA_PHAT)),
                category_id: Set(cat("ban")),
                style: Set(Some("Hiện đại".into())),
                price_min: Set(Some(18_000_000)),
                price_max: Set(Some(24_000_000)),
                origin: Set(Some("Việt Nam".into())),
                created_by_id: Set(Some(admin_id)),
                ..Default::default()
            },
            // Deliberately unfiled: the taxonomy describes furniture, and a
            // sheet of MDF is a material, not a piece of it. It also gives the
            // admin's Unfiled count and the auto-assign preview a real row to
            // act on instead of an empty screen.
            products::ActiveModel {
                id: Set(P_MDF),
                slug: Set("van-mdf-phu-melamine".into()),
                name: Set("Ván MDF phủ Melamine".into()),
                product_type: Set(sea_orm_active_enums::ProductType::Material),
                status: Set(sea_orm_active_enums::ProductStatus::Published),
                price_min: Set(Some(220_000)),
                price_max: Set(Some(450_000)),
                description_md: Set(Some("Đánh giá vật liệu ván MDF phủ Melamine.".into())),
                created_by_id: Set(Some(admin_id)),
                ..Default::default()
            },
        ])
        .await?;

        // 2. Link products to the materials `seed_materials` wrote above.
        let mat_ids: std::collections::HashMap<String, Uuid> = materials::Entity::find()
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .into_iter()
            .map(|m| (m.slug, m.id))
            .collect();

        let mut links: Vec<product_materials::ActiveModel> = Vec::new();
        for (product_id, slug) in [
            (P_SOFA, "go-soi"),
            (P_SOFA, "vai-ni"),
            (P_TABLE, "go-oc-cho"),
            (P_MDF, "mdf-melamine"),
        ] {
            if let Some(&material_id) = mat_ids.get(slug) {
                links.push(product_materials::ActiveModel {
                    product_id: Set(product_id),
                    material_id: Set(material_id),
                });
            }
        }
        if !links.is_empty() {
            insert_or_ignore::<product_materials::Entity, _, _>(&self.db, links).await?;
        }

        // 3. One review thread linked to the sofa, with its opening post.
        insert_or_ignore::<threads::Entity, _, _>(&self.db, [threads::ActiveModel {
            id: Set(TH_SOFA_REVIEW),
            category_id: Set(CAT_GENERAL),
            author_id: Set(ALICE_ID),
            title: Set("Đánh giá Sofa văng bọc nỉ sau 6 tháng sử dụng".into()),
            slug: Set("danh-gia-sofa-vang-boc-ni".into()),
            status: Set(ThreadStatus::Open),
            view_count: Set(42),
            last_post_at: Set(Some(now)),
            product_id: Set(Some(P_SOFA)),
            custom_fields: Set(serde_json::Value::Object(Default::default())),
            ..Default::default()
        }])
        .await?;

        insert_or_ignore::<posts::Entity, _, _>(&self.db, [posts::ActiveModel {
            id: Set(POST_SOFA_REVIEW),
            thread_id: Set(TH_SOFA_REVIEW),
            author_id: Set(ALICE_ID),
            parent_id: Set(None),
            content_md: Set("Khung gỗ sồi chắc chắn, vải nỉ ít bám bụi và dễ vệ sinh. Rất đáng tiền.".into()),
            content_html: Set("<p>Khung gỗ sồi chắc chắn, vải nỉ ít bám bụi và dễ vệ sinh. Rất đáng tiền.</p>".into()),
            created_at: Set(now),
            ..Default::default()
        }])
        .await?;

        // 4. Structured rating + its precomputed aggregate (numeric, exact).
        insert_or_ignore::<review_ratings::Entity, _, _>(&self.db, [review_ratings::ActiveModel {
            thread_id: Set(TH_SOFA_REVIEW),
            overall: Set(5),
            durability: Set(Some(5)),
            materials: Set(Some(5)),
            comfort: Set(Some(4)),
            aesthetics: Set(Some(5)),
            value_for_money: Set(Some(4)),
            verified_purchase: Set(true),
            ..Default::default()
        }])
        .await?;

        insert_or_ignore::<product_rating_stats::Entity, _, _>(
            &self.db,
            [product_rating_stats::ActiveModel {
                product_id: Set(P_SOFA),
                review_count: Set(1),
                avg_overall: Set(Some(Decimal::from(5))),
                avg_durability: Set(Some(Decimal::from(5))),
                avg_materials: Set(Some(Decimal::from(5))),
                avg_comfort: Set(Some(Decimal::from(4))),
                avg_aesthetics: Set(Some(Decimal::from(5))),
                avg_value_for_money: Set(Some(Decimal::from(4))),
                updated_at: Set(now),
            }],
        )
        .await?;

        Ok(())
    }

    // ── Moderation demo ───────────────────────────────────────────────────────
    // Without these, /mod/reports, /mod/queue and the dashboard's pending-reports
    // tile all render empty on a freshly seeded install — the surfaces a Power
    // User most wants to look at are the ones with nothing to show.

    async fn seed_moderation_demo(&self, admin_id: Uuid) -> Result<(), AppError> {
        use crate::entities::{reports, sea_orm_active_enums::ReportStatus};

        let now = Utc::now().fixed_offset();

        insert_or_ignore::<reports::Entity, _, _>(&self.db, [
            reports::ActiveModel {
                id: Set(REPORT_SPAM),
                reporter_id: Set(ALICE_ID),
                post_id: Set(Some(POST_RUST_GO_1)),
                thread_id: Set(None),
                reason: Set("Off-topic promotion in the middle of a technical thread.".into()),
                status: Set(ReportStatus::Pending),
                created_at: Set(now - Duration::hours(4)),
                ..Default::default()
            },
            reports::ActiveModel {
                id: Set(REPORT_RUDE),
                reporter_id: Set(BOB_ID),
                post_id: Set(Some(POST_FAV_LANG_3)),
                thread_id: Set(None),
                reason: Set("Dismissive tone towards other members.".into()),
                status: Set(ReportStatus::Pending),
                created_at: Set(now - Duration::hours(2)),
                ..Default::default()
            },
            // Thread-level report: the queue must show both shapes, since the
            // resolve action differs between them.
            reports::ActiveModel {
                id: Set(REPORT_THREAD),
                reporter_id: Set(MOD_ID),
                post_id: Set(None),
                thread_id: Set(Some(TH_DARK_MODE)),
                reason: Set("Duplicate of an existing feature request.".into()),
                status: Set(ReportStatus::Resolved),
                moderator_notes: Set(Some("Merged into the roadmap discussion.".into())),
                resolved_by_id: Set(Some(admin_id)),
                resolved_at: Set(Some(now - Duration::hours(20))),
                created_at: Set(now - Duration::days(1)),
                ..Default::default()
            },
            reports::ActiveModel {
                id: Set(REPORT_DISMISSED),
                reporter_id: Set(ALICE_ID),
                post_id: Set(Some(POST_WELCOME_3)),
                thread_id: Set(None),
                reason: Set("Suspected bot account.".into()),
                status: Set(ReportStatus::Dismissed),
                moderator_notes: Set(Some("Genuine member; no action taken.".into())),
                resolved_by_id: Set(Some(MOD_ID)),
                resolved_at: Set(Some(now - Duration::days(1))),
                created_at: Set(now - Duration::days(2)),
                ..Default::default()
            },
        ])
        .await
    }

    // ── Social demo ───────────────────────────────────────────────────────────

    async fn seed_social_demo(&self, admin_id: Uuid) -> Result<(), AppError> {
        use crate::entities::{bookmarks, user_follows};

        let now = Utc::now().fixed_offset();

        insert_or_ignore::<bookmarks::Entity, _, _>(&self.db, [
            bookmarks::ActiveModel {
                id: Set(Uuid::new_v4()),
                user_id: Set(ALICE_ID),
                thread_id: Set(TH_WELCOME),
                created_at: Set(now - Duration::days(2)),
            },
            bookmarks::ActiveModel {
                id: Set(Uuid::new_v4()),
                user_id: Set(ALICE_ID),
                thread_id: Set(TH_RUST_VS_GO),
                created_at: Set(now - Duration::days(1)),
            },
            bookmarks::ActiveModel {
                id: Set(Uuid::new_v4()),
                user_id: Set(BOB_ID),
                thread_id: Set(TH_SOFA_REVIEW),
                created_at: Set(now - Duration::hours(6)),
            },
            bookmarks::ActiveModel {
                id: Set(Uuid::new_v4()),
                user_id: Set(MOD_ID),
                thread_id: Set(TH_FAV_LANG),
                created_at: Set(now - Duration::hours(9)),
            },
        ])
        .await?;

        insert_or_ignore::<user_follows::Entity, _, _>(&self.db, [
            user_follows::ActiveModel {
                id: Set(Uuid::new_v4()),
                follower_id: Set(BOB_ID),
                followed_id: Set(ALICE_ID),
                created_at: Set(now - Duration::days(3)),
            },
            user_follows::ActiveModel {
                id: Set(Uuid::new_v4()),
                follower_id: Set(ALICE_ID),
                followed_id: Set(MOD_ID),
                created_at: Set(now - Duration::days(2)),
            },
            user_follows::ActiveModel {
                id: Set(Uuid::new_v4()),
                follower_id: Set(MOD_ID),
                followed_id: Set(admin_id),
                created_at: Set(now - Duration::days(2)),
            },
            user_follows::ActiveModel {
                id: Set(Uuid::new_v4()),
                follower_id: Set(ALICE_ID),
                followed_id: Set(admin_id),
                created_at: Set(now - Duration::days(1)),
            },
        ])
        .await
    }

    /// Set every user's `post_count` to the number of posts they actually have.
    ///
    /// The counter is maintained incrementally by the post use case at runtime;
    /// seeding writes rows underneath it, so the two only agree if the total is
    /// recomputed once at the end. Deleted posts are excluded, matching what the
    /// profile page lists.
    async fn sync_user_post_counts(&self) -> Result<(), AppError> {
        use sea_orm::{ConnectionTrait, Statement};

        self.db
            .execute_raw(Statement::from_string(
                self.db.get_database_backend(),
                "UPDATE users u SET post_count = coalesce(c.n, 0) \
                 FROM ( \
                     SELECT u2.id, count(p.id) AS n \
                     FROM users u2 \
                     LEFT JOIN posts p ON p.author_id = u2.id AND p.is_deleted = false \
                     GROUP BY u2.id \
                 ) c \
                 WHERE u.id = c.id AND u.post_count IS DISTINCT FROM coalesce(c.n, 0)"
                    .to_owned(),
            ))
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        Ok(())
    }
}
