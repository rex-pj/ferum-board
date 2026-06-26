use serde::{Deserialize, Serialize};

use ferum_domain::models::user::User;

/// Generic option shape consumed by the `<ferum-remote-select>` web component.
/// Every lookup endpoint returns a paged list of these so the component has a
/// single, stable contract regardless of the underlying entity.
#[derive(Serialize)]
pub struct LookupOption {
    /// Stable identifier submitted with the form (e.g. a UUID string).
    pub value: String,
    /// Primary text shown in the dropdown and the selected chip.
    pub label: String,
    /// Optional secondary text (e.g. "@username" or an email).
    pub sublabel: Option<String>,
    /// Optional avatar/thumbnail URL.
    pub avatar_url: Option<String>,
}

impl From<User> for LookupOption {
    fn from(u: User) -> Self {
        let label = u.display_name.clone().unwrap_or_else(|| u.username.clone());
        LookupOption {
            value: u.id.to_string(),
            label,
            sublabel: Some(format!("@{}", u.username)),
            avatar_url: u.avatar_url,
        }
    }
}

/// Query string for lookup endpoints: free-text `q` + page.
#[derive(Deserialize)]
pub struct LookupQuery {
    pub q: Option<String>,
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}
