use serde::{Deserialize, Serialize};

use ferum_domain::models::category::Category;
use ferum_domain::models::report::Report;
use ferum_domain::models::thread::Thread;

// ─── Category / Forum index ───────────────────────────────────────────────────

pub struct SubcategoryCount {
    pub category: Category,
    pub thread_count: u64,
}

pub struct ForumIndexItem {
    pub category: Category,
    pub thread_count: u64,
    pub subcategories: Vec<SubcategoryCount>,
    pub recent_threads: Vec<Thread>,
}

// ─── Moderation ───────────────────────────────────────────────────────────────

pub struct ReportWithContext {
    pub report: Report,
    pub reporter_username: String,
    pub thread_slug: Option<String>,
    pub thread_title: Option<String>,
}

// ─── Social ───────────────────────────────────────────────────────────────────

pub struct FollowStatus {
    pub following: bool,
    pub follower_count: u64,
    pub following_count: u64,
}

// ─── Admin stats ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct DashboardStats {
    pub users_total: u64,
    pub threads_total: u64,
    pub posts_total: u64,
    pub reports_pending: u64,
    pub new_users_today: u64,
    pub new_threads_today: u64,
    pub new_posts_today: u64,
    pub new_reactions_today: u64,
    pub new_views_today: u64,
    pub dau: u64,
    pub mau: u64,
    /// DAU / MAU × 100, rounded to 1 decimal — forum engagement stickiness indicator.
    pub dau_mau_ratio: f64,
    /// % of users registered in the past 24 h who posted at least once today.
    pub activation_rate_pct: u64,
    /// Hours the oldest pending report has been waiting (None if no pending reports).
    pub oldest_pending_report_hours: Option<u64>,
}

/// One data point in a time-series history response.
#[derive(Serialize)]
pub struct StatPoint {
    pub date: String,
    pub value: u64,
}
