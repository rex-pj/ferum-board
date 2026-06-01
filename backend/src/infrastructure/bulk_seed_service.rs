use async_trait::async_trait;
use chrono::{Duration, Utc};
use sea_orm::{
    sea_query::{Expr, OnConflict}, ActiveModelTrait, ActiveValue::Set, ColumnTrait,
    DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder,
};
use uuid::{uuid, Uuid};

use crate::application::ports::BulkSeedService;
use crate::application::shared::AppError;
use crate::entities::{
    categories::{self, PostPolicy, ViewPolicy},
    notifications::{self, NotificationKind},
    posts,
    reactions::{self, ReactionKind},
    threads::{self, ThreadStatus},
    user_preferences,
    users::{self, TrustLevel, UserRole},
};

// Fixed UUIDs for example seed users (moderator, alice, bob).
// The admin user's real ID is passed in at runtime.
const MOD_ID:   Uuid = uuid!("00000000-0000-0000-0000-000000000002");
const ALICE_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000003");
const BOB_ID:   Uuid = uuid!("00000000-0000-0000-0000-000000000004");

const CAT_GENERAL:       Uuid = uuid!("10000000-0000-0000-0000-000000000001");
const CAT_INTRODUCTIONS: Uuid = uuid!("10000000-0000-0000-0000-000000000002");
const CAT_OFF_TOPIC:     Uuid = uuid!("10000000-0000-0000-0000-000000000003");
const CAT_TECHNOLOGY:    Uuid = uuid!("10000000-0000-0000-0000-000000000004");
const CAT_PROGRAMMING:   Uuid = uuid!("10000000-0000-0000-0000-000000000005");
const CAT_HARDWARE:      Uuid = uuid!("10000000-0000-0000-0000-000000000006");
const CAT_FEEDBACK:      Uuid = uuid!("10000000-0000-0000-0000-000000000007");
const CAT_STAFF:         Uuid = uuid!("10000000-0000-0000-0000-000000000008");

const TH_WELCOME:    Uuid = uuid!("20000000-0000-0000-0000-000000000001");
const TH_INTRO:      Uuid = uuid!("20000000-0000-0000-0000-000000000002");
const TH_FAV_LANG:   Uuid = uuid!("20000000-0000-0000-0000-000000000003");
const TH_RUST_VS_GO: Uuid = uuid!("20000000-0000-0000-0000-000000000004");
const TH_DARK_MODE:  Uuid = uuid!("20000000-0000-0000-0000-000000000005");

const POST_WELCOME_1:  Uuid = uuid!("30000000-0000-0000-0000-000000000001");
const POST_WELCOME_2:  Uuid = uuid!("30000000-0000-0000-0000-000000000002");
const POST_WELCOME_3:  Uuid = uuid!("30000000-0000-0000-0000-000000000003");
const POST_INTRO_1:    Uuid = uuid!("30000000-0000-0000-0000-000000000004");
const POST_INTRO_2:    Uuid = uuid!("30000000-0000-0000-0000-000000000005");
const POST_FAV_LANG_1: Uuid = uuid!("30000000-0000-0000-0000-000000000006");
const POST_FAV_LANG_2: Uuid = uuid!("30000000-0000-0000-0000-000000000007");
const POST_FAV_LANG_3: Uuid = uuid!("30000000-0000-0000-0000-000000000008");
const POST_RUST_GO_1:  Uuid = uuid!("30000000-0000-0000-0000-000000000009");
const POST_RUST_GO_2:  Uuid = uuid!("30000000-0000-0000-0000-000000000010");
const POST_RUST_GO_3:  Uuid = uuid!("30000000-0000-0000-0000-000000000011");
const POST_DARK_1:     Uuid = uuid!("30000000-0000-0000-0000-000000000012");
const POST_DARK_2:     Uuid = uuid!("30000000-0000-0000-0000-000000000013");

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
        self.seed_preferences(admin_id).await?;
        self.seed_categories().await?;
        self.seed_example_threads(admin_id).await?;
        self.seed_example_posts(admin_id).await?;
        self.seed_example_reactions(admin_id).await?;
        self.seed_bulk_threads().await?;
        self.seed_bulk_posts().await?;
        self.sync_bulk_thread_stats().await?;
        self.seed_bulk_reactions().await?;
        self.seed_bulk_notifications().await?;

        Ok(())
    }
}

impl PgBulkSeedService {
    // ── Example data (compile-time-typed via Sea-ORM ActiveModel) ─────────────

    async fn seed_example_users(&self, hash: &str) -> Result<(), AppError> {
        insert_or_ignore::<users::Entity, _, _>(&self.db, [
            users::ActiveModel {
                id:             Set(MOD_ID),
                username:       Set("moderator".into()),
                email:          Set("mod@ferum.local".into()),
                is_email_verified: Set(true),
                display_name:   Set(Some("Moderator".into())),
                password_hash:  Set(Some(hash.into())),
                role:           Set(UserRole::Moderator),
                trust_level:    Set(TrustLevel::Member),
                is_global_mod:  Set(true),
                trust_score:    Set(80),
                post_count:     Set(12),
                ..Default::default()
            },
            users::ActiveModel {
                id:             Set(ALICE_ID),
                username:       Set("alice".into()),
                email:          Set("alice@ferum.local".into()),
                is_email_verified: Set(true),
                display_name:   Set(Some("Alice".into())),
                password_hash:  Set(Some(hash.into())),
                role:           Set(UserRole::Member),
                trust_level:    Set(TrustLevel::Member),
                is_global_mod:  Set(false),
                trust_score:    Set(40),
                post_count:     Set(8),
                ..Default::default()
            },
            users::ActiveModel {
                id:             Set(BOB_ID),
                username:       Set("bob".into()),
                email:          Set("bob@ferum.local".into()),
                is_email_verified: Set(true),
                display_name:   Set(Some("Bob".into())),
                password_hash:  Set(Some(hash.into())),
                role:           Set(UserRole::Member),
                trust_level:    Set(TrustLevel::Basic),
                is_global_mod:  Set(false),
                trust_score:    Set(10),
                post_count:     Set(2),
                ..Default::default()
            },
        ]).await
    }

    async fn seed_categories(&self) -> Result<(), AppError> {
        insert_or_ignore::<categories::Entity, _, _>(&self.db, [
            categories::ActiveModel {
                id:          Set(CAT_GENERAL),
                parent_id:   Set(None),
                slug:        Set("general".into()),
                name:        Set("General Discussion".into()),
                description: Set(Some("A place for general topics and community chat.".into())),
                position:    Set(1),
                view_policy: Set(ViewPolicy::Public),
                post_policy: Set(PostPolicy::Members),
                color:       Set(Some("#0d6efd".into())),
                ..Default::default()
            },
            categories::ActiveModel {
                id:          Set(CAT_INTRODUCTIONS),
                parent_id:   Set(Some(CAT_GENERAL)),
                slug:        Set("introductions".into()),
                name:        Set("Introductions".into()),
                description: Set(Some("New here? Say hello!".into())),
                position:    Set(1),
                view_policy: Set(ViewPolicy::Public),
                post_policy: Set(PostPolicy::Members),
                color:       Set(Some("#198754".into())),
                ..Default::default()
            },
            categories::ActiveModel {
                id:          Set(CAT_OFF_TOPIC),
                parent_id:   Set(Some(CAT_GENERAL)),
                slug:        Set("off-topic".into()),
                name:        Set("Off-Topic".into()),
                description: Set(Some("Anything that does not fit elsewhere.".into())),
                position:    Set(2),
                view_policy: Set(ViewPolicy::Public),
                post_policy: Set(PostPolicy::Members),
                color:       Set(Some("#6c757d".into())),
                ..Default::default()
            },
            categories::ActiveModel {
                id:          Set(CAT_TECHNOLOGY),
                parent_id:   Set(None),
                slug:        Set("technology".into()),
                name:        Set("Technology".into()),
                description: Set(Some("Discussions about tech, software, and hardware.".into())),
                position:    Set(2),
                view_policy: Set(ViewPolicy::Public),
                post_policy: Set(PostPolicy::Members),
                color:       Set(Some("#6f42c1".into())),
                ..Default::default()
            },
            categories::ActiveModel {
                id:          Set(CAT_PROGRAMMING),
                parent_id:   Set(Some(CAT_TECHNOLOGY)),
                slug:        Set("programming".into()),
                name:        Set("Programming".into()),
                description: Set(Some("Languages, frameworks, tools, and code.".into())),
                position:    Set(1),
                view_policy: Set(ViewPolicy::Public),
                post_policy: Set(PostPolicy::Members),
                color:       Set(Some("#0dcaf0".into())),
                ..Default::default()
            },
            categories::ActiveModel {
                id:          Set(CAT_HARDWARE),
                parent_id:   Set(Some(CAT_TECHNOLOGY)),
                slug:        Set("hardware".into()),
                name:        Set("Hardware".into()),
                description: Set(Some("CPUs, GPUs, peripherals, and builds.".into())),
                position:    Set(2),
                view_policy: Set(ViewPolicy::Public),
                post_policy: Set(PostPolicy::Members),
                color:       Set(Some("#fd7e14".into())),
                ..Default::default()
            },
            categories::ActiveModel {
                id:          Set(CAT_FEEDBACK),
                parent_id:   Set(None),
                slug:        Set("site-feedback".into()),
                name:        Set("Site Feedback".into()),
                description: Set(Some("Report bugs and suggest improvements.".into())),
                position:    Set(3),
                view_policy: Set(ViewPolicy::Public),
                post_policy: Set(PostPolicy::Members),
                color:       Set(Some("#ffc107".into())),
                ..Default::default()
            },
            categories::ActiveModel {
                id:          Set(CAT_STAFF),
                parent_id:   Set(None),
                slug:        Set("staff-only".into()),
                name:        Set("Staff Only".into()),
                description: Set(Some("Internal staff coordination.".into())),
                position:    Set(4),
                view_policy: Set(ViewPolicy::StaffOnly),
                post_policy: Set(PostPolicy::StaffOnly),
                color:       Set(Some("#dc3545".into())),
                ..Default::default()
            },
        ]).await
    }

    async fn seed_example_threads(&self, admin_id: Uuid) -> Result<(), AppError> {
        let now = Utc::now().fixed_offset();
        insert_or_ignore::<threads::Entity, _, _>(&self.db, [
            threads::ActiveModel {
                id:           Set(TH_WELCOME),
                category_id:  Set(CAT_GENERAL),
                author_id:    Set(admin_id),
                title:        Set("Welcome to Ferum Board!".into()),
                slug:         Set("welcome-to-ferum-board".into()),
                status:       Set(ThreadStatus::Open),
                is_pinned:    Set(true),
                reply_count:  Set(2),
                view_count:   Set(120),
                last_post_at: Set(Some(now - Duration::hours(1))),
                ..Default::default()
            },
            threads::ActiveModel {
                id:           Set(TH_INTRO),
                category_id:  Set(CAT_INTRODUCTIONS),
                author_id:    Set(MOD_ID),
                title:        Set("Introduce yourself here!".into()),
                slug:         Set("introduce-yourself-here".into()),
                status:       Set(ThreadStatus::Open),
                is_pinned:    Set(true),
                reply_count:  Set(1),
                view_count:   Set(54),
                last_post_at: Set(Some(now - Duration::hours(2))),
                ..Default::default()
            },
            threads::ActiveModel {
                id:           Set(TH_FAV_LANG),
                category_id:  Set(CAT_PROGRAMMING),
                author_id:    Set(ALICE_ID),
                title:        Set("What is your favorite programming language?".into()),
                slug:         Set("what-is-your-favorite-programming-language".into()),
                status:       Set(ThreadStatus::Open),
                is_pinned:    Set(false),
                reply_count:  Set(2),
                view_count:   Set(87),
                last_post_at: Set(Some(now - Duration::hours(3))),
                ..Default::default()
            },
            threads::ActiveModel {
                id:           Set(TH_RUST_VS_GO),
                category_id:  Set(CAT_PROGRAMMING),
                author_id:    Set(BOB_ID),
                title:        Set("Rust vs Go for backend development".into()),
                slug:         Set("rust-vs-go-for-backend-development".into()),
                status:       Set(ThreadStatus::Open),
                is_pinned:    Set(false),
                reply_count:  Set(2),
                view_count:   Set(210),
                last_post_at: Set(Some(now - Duration::hours(5))),
                ..Default::default()
            },
            threads::ActiveModel {
                id:           Set(TH_DARK_MODE),
                category_id:  Set(CAT_FEEDBACK),
                author_id:    Set(ALICE_ID),
                title:        Set("Feature request: dark mode".into()),
                slug:         Set("feature-request-dark-mode".into()),
                status:       Set(ThreadStatus::Open),
                is_pinned:    Set(false),
                reply_count:  Set(1),
                view_count:   Set(33),
                last_post_at: Set(Some(now - Duration::hours(6))),
                ..Default::default()
            },
        ]).await
    }

    async fn seed_example_posts(&self, admin_id: Uuid) -> Result<(), AppError> {
        let now = Utc::now().fixed_offset();
        insert_or_ignore::<posts::Entity, _, _>(&self.db, [
            posts::ActiveModel {
                id:           Set(POST_WELCOME_1),
                thread_id:    Set(TH_WELCOME),
                author_id:    Set(admin_id),
                parent_id:    Set(None),
                content_md:   Set("## Welcome to Ferum Board!\n\nThis forum is built with **Rust** and **SvelteKit**.".into()),
                content_html: Set("<h2>Welcome to Ferum Board!</h2><p>This forum is built with <strong>Rust</strong> and <strong>SvelteKit</strong>.</p>".into()),
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
        insert_or_ignore::<reactions::Entity, _, _>(&self.db, [
            reactions::ActiveModel { post_id: Set(POST_WELCOME_1),  user_id: Set(ALICE_ID),  kind: Set(ReactionKind::Like),       ..Default::default() },
            reactions::ActiveModel { post_id: Set(POST_WELCOME_1),  user_id: Set(BOB_ID),    kind: Set(ReactionKind::Helpful),    ..Default::default() },
            reactions::ActiveModel { post_id: Set(POST_WELCOME_2),  user_id: Set(admin_id),  kind: Set(ReactionKind::Like),       ..Default::default() },
            reactions::ActiveModel { post_id: Set(POST_RUST_GO_1),  user_id: Set(ALICE_ID),  kind: Set(ReactionKind::Insightful), ..Default::default() },
            reactions::ActiveModel { post_id: Set(POST_RUST_GO_2),  user_id: Set(BOB_ID),    kind: Set(ReactionKind::Like),       ..Default::default() },
            reactions::ActiveModel { post_id: Set(POST_DARK_1),     user_id: Set(MOD_ID),    kind: Set(ReactionKind::Like),       ..Default::default() },
            reactions::ActiveModel { post_id: Set(POST_FAV_LANG_1), user_id: Set(BOB_ID),    kind: Set(ReactionKind::Helpful),    ..Default::default() },
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
            .map(|i| users::ActiveModel {
                username:       Set(format!("user_{i}")),
                email:          Set(format!("user_{i}@ferum.local")),
                is_email_verified: Set(true),
                display_name:   Set(Some(format!("User {i}"))),
                password_hash:  Set(Some(hash.into())),
                role:           Set(UserRole::Member),
                trust_level:    Set(trust_levels[(i - 1) % 5].clone()),
                trust_score:    Set(((i * 7) % 100) as i32),
                post_count:     Set(((i * 3) % 30) as i32),
                ..Default::default()
            })
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

        let nc = pub_cat_ids.len();
        let nu = all_user_ids.len();
        let statuses = [
            ThreadStatus::Open,
            ThreadStatus::Open,
            ThreadStatus::Open,
            ThreadStatus::Open,
            ThreadStatus::Locked,
        ];
        let now = Utc::now().fixed_offset();

        let rows: Vec<threads::ActiveModel> = (1usize..=5000)
            .map(|i| {
                let ts = now - Duration::minutes((i * 2) as i64);
                threads::ActiveModel {
                    category_id:   Set(pub_cat_ids[(i - 1) % nc]),
                    author_id:     Set(all_user_ids[(i - 1) % nu]),
                    title:         Set(format!("{} topic {}", BULK_THREAD_TITLE_PREFIXES[(i - 1) % 10], (i - 1) % 50 + 1)),
                    slug:          Set(format!("bulk-thread-{i}")),
                    status:        Set(statuses[(i - 1) % 5].clone()),
                    view_count:    Set(((i * 13) % 900) as i32),
                    reply_count:   Set(0),
                    last_post_at:  Set(Some(ts)),
                    created_at:    Set(ts),
                    custom_fields: Set(serde_json::Value::Object(Default::default())),
                    ..Default::default()
                }
            })
            .collect();

        insert_in_chunks::<threads::Entity, _>(&self.db, rows, 200).await
    }

    async fn seed_bulk_posts(&self) -> Result<(), AppError> {
        let bulk_thread_ids: Vec<Uuid> = threads::Entity::find()
            .filter(threads::Column::Slug.like("bulk-thread-%"))
            .order_by_asc(threads::Column::CreatedAt)
            .order_by_asc(threads::Column::Id)
            .all(&self.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .into_iter()
            .map(|t| t.id)
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

        let nt = bulk_thread_ids.len();
        let nu = all_user_ids.len();
        let now = Utc::now().fixed_offset();

        let rows: Vec<posts::ActiveModel> = (1usize..=90000)
            .map(|i| {
                let body = BULK_POST_BODIES[(i - 1) % 10];
                posts::ActiveModel {
                    thread_id:    Set(bulk_thread_ids[((i - 1) / 18) % nt]),
                    author_id:    Set(all_user_ids[(i - 1) % nu]),
                    parent_id:    Set(None),
                    content_md:   Set(format!("Post #{i} — {body}")),
                    content_html: Set(format!("<p>Post #{i} — {body}</p>")),
                    created_at:   Set(now - Duration::minutes(i as i64)),
                    ..Default::default()
                }
            })
            .collect();

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
        let mut stats: HashMap<Uuid, (i64, chrono::DateTime<chrono::FixedOffset>)> =
            HashMap::new();
        for post in all_posts {
            let entry = stats.entry(post.thread_id).or_insert((0, post.created_at));
            entry.0 += 1;
            if post.created_at > entry.1 {
                entry.1 = post.created_at;
            }
        }

        for (thread_id, (count, last_at)) in stats {
            threads::Entity::update_many()
                .col_expr(threads::Column::ReplyCount, Expr::value((count - 1).max(0) as i32))
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

        let np = bulk_post_ids.len();
        let nu = all_user_ids.len();
        let kinds = [
            ReactionKind::Like,
            ReactionKind::Helpful,
            ReactionKind::Insightful,
            ReactionKind::Funny,
        ];

        let rows: Vec<reactions::ActiveModel> = (1usize..=5000)
            .map(|i| reactions::ActiveModel {
                post_id: Set(bulk_post_ids[(i * 7) % np]),
                user_id: Set(all_user_ids[(i * 11) % nu]),
                kind:    Set(kinds[(i - 1) % 4].clone()),
                ..Default::default()
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

        let nu = all_user_ids.len();
        let kinds = [
            NotificationKind::Reply,
            NotificationKind::Mention,
            NotificationKind::Reaction,
            NotificationKind::System,
        ];

        let rows: Vec<notifications::ActiveModel> = (1usize..=2000)
            .map(|i| notifications::ActiveModel {
                user_id: Set(all_user_ids[(i * 3) % nu]),
                kind:    Set(kinds[(i - 1) % 4].clone()),
                payload: Set(serde_json::json!({ "message": format!("Notification {i}"), "ref": i })),
                is_read: Set(i % 3 == 0),
                ..Default::default()
            })
            .collect();

        insert_in_chunks::<notifications::Entity, _>(&self.db, rows, 200).await
    }
}
