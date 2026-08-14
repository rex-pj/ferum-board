
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub is_email_verified: bool,
    pub display_name: Option<String>,
    pub password_hash: Option<String>,
    pub trust_level: TrustLevel,
    /// Slug of the user's highest-priority global role (lowest position value). Used for badge display.
    pub primary_role_slug: Option<String>,
    pub trust_score: i32,
    pub post_count: i32,
    pub days_visited: i32,
    pub avatar_url: Option<String>,
    pub cover_url: Option<String>,
    pub bio: Option<String>,
    pub website: Option<String>,
    pub is_banned: bool,
    pub banned_until: Option<DateTime<Utc>>,
    pub ban_reason: Option<String>,
    pub warn_count: i32,
    pub failed_login_count: i32,
    pub locked_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

impl User {
    pub fn is_active(&self) -> bool {
        self.deleted_at.is_none()
    }

    pub fn display(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.username)
    }

    pub fn is_account_locked(&self) -> bool {
        self.locked_until.map(|t| t > Utc::now()).unwrap_or(false)
    }

    pub fn is_currently_banned(&self) -> bool {
        if !self.is_banned {
            return false;
        }
        self.banned_until.map(|t| t > Utc::now()).unwrap_or(true)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash, Copy)]
pub enum TrustLevel {
    New = 0,
    Basic = 1,
    Member = 2,
    Regular = 3,
    Leader = 4,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserPreferences {
    pub user_id: Uuid,
    pub theme: String,
    pub font_size: String,
    pub layout: String,
    pub email_notifications: serde_json::Value,
    pub muted_categories: Vec<Uuid>,
    pub watched_categories: Vec<Uuid>,
    /// Chosen display language. `None` = never chosen, so negotiation falls
    /// through to the cookie, `Accept-Language`, and finally the site default.
    /// Storing an explicit `Some("en")` is a real choice and pins the user to
    /// English even if the admin later changes the site default.
    pub locale: Option<crate::Locale>,
    /// IANA timezone for displaying timestamps to this user, e.g.
    /// `"Europe/Lisbon"`.
    ///
    /// `None` — the default — means "use whatever zone the device reports",
    /// which is right for almost everyone and is the only thing that can be
    /// right for a guest. A value here overrides the device, for the user whose
    /// machine is set wrong or who wants the community's zone while travelling.
    ///
    /// It is also the only zone a server-rendered email could use — but note
    /// that **no email currently renders a date**: the catalog has four keys,
    /// verify and reset, neither carrying a timestamp. Wire this up when one
    /// does; do not assume it already is.
    ///
    /// Kept as a `String` rather than a parsed zone: the domain never converts
    /// with it (the browser does), it is validated on write, and parsing it
    /// here would pull a timezone database into a crate that has no I/O and no
    /// framework dependencies.
    pub timezone: Option<String>,
}

/// Which notification kinds a user wants delivered by email.
///
/// Stored as the `user_preferences.email_notifications` JSONB object, one boolean
/// per key. Only the kinds listed here are ever emailed — reactions, follows and
/// system notices stay in-app, because they are the highest-volume and lowest-value
/// of the five and are the usual reason someone turns email off wholesale.
///
/// ## An absent key means "do not send", and that is the migration story
///
/// Every account created before this feature has `{}`, so every one of them is
/// silent until the user opts in. New accounts get both keys written explicitly at
/// registration (see `AuthUseCase::register`), so they are opted **in** by default.
///
/// That asymmetry is the whole design: it is the only way to have a sensible
/// default for new members without mailing an existing community that never agreed
/// to it. Reading a missing key as `true` would, on the first deploy, send mail to
/// every account the forum has ever had — and a spam complaint spike is the one
/// mistake here that cannot be undone, because it is the sending domain's
/// reputation that pays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EmailNotificationPrefs {
    /// A reply to a thread this user started.
    pub reply: bool,
    /// An `@username` mention of this user.
    pub mention: bool,
}

impl EmailNotificationPrefs {
    /// What a newly registered account gets. Opt-out from here on.
    pub const OPTED_IN: Self = Self {
        reply: true,
        mention: true,
    };

    /// Reads the stored JSON. Anything absent, non-boolean or malformed is `false`
    /// — the safe direction, and it means a hand-edited row cannot turn mail on.
    pub fn from_json(value: &serde_json::Value) -> Self {
        let flag = |key: &str| value.get(key).and_then(serde_json::Value::as_bool) == Some(true);
        Self {
            reply: flag("reply"),
            mention: flag("mention"),
        }
    }

    pub fn to_json(self) -> serde_json::Value {
        serde_json::json!({ "reply": self.reply, "mention": self.mention })
    }

    /// True when nothing at all would be emailed. Lets the unsubscribe endpoint
    /// and the account page report state without duplicating the field list.
    pub fn all_off(self) -> bool {
        !self.reply && !self.mention
    }

    /// Every flag off — what a one-click unsubscribe link writes.
    pub const OPTED_OUT: Self = Self {
        reply: false,
        mention: false,
    };
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            user_id: Uuid::nil(),
            theme: "auto".to_string(),
            font_size: "medium".to_string(),
            layout: "comfortable".to_string(),
            // `{}` — not `OPTED_IN`. This default is used when a user has no
            // preferences row at all, which is the shape every pre-existing
            // account has, so it must read as "off". Registration writes
            // `OPTED_IN` explicitly instead.
            email_notifications: serde_json::json!({}),
            muted_categories: vec![],
            watched_categories: vec![],
            locale: None,
            // None = follow the device's zone. See the field's doc comment for
            // why that is the default rather than UTC.
            timezone: None,
        }
    }
}
