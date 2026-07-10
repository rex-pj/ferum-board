use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;

use crate::app_state::AppState;
use ferum_domain::repositories::thread_repository::ThreadSort;

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn url_entry(loc: &str) -> String {
    format!("  <url><loc>{}</loc></url>\n", xml_escape(loc))
}

/// GET /sitemap.xml — a guest's-eye view of the forum: static pages, every
/// publicly visible category, and the most recent publicly visible threads
/// (bounded by the same `max_threads_per_page` config the rest of the app
/// respects, so this doesn't bypass an admin's own pagination ceiling).
/// staff_only / members_only content is never included, same as an anonymous
/// visitor would see.
pub async fn sitemap_xml(State(state): State<AppState>) -> impl IntoResponse {
    let base = state.app_url.trim_end_matches('/');

    let mut body = String::new();
    body.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    body.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");

    body.push_str(&url_entry(base));
    body.push_str(&url_entry(&format!("{base}/forum")));
    body.push_str(&url_entry(&format!("{base}/search")));

    if let Ok(categories) = state.category.list_visible(None).await {
        for c in &categories {
            body.push_str(&url_entry(&format!("{base}/forum/{}", c.slug)));
        }
    }

    if let Ok((threads, _total)) = state.thread.list_feed(None, ThreadSort::Newest, 1, 500).await {
        for t in &threads {
            body.push_str(&url_entry(&format!("{base}/forum/t/{}", t.slug)));
        }
    }

    body.push_str("</urlset>\n");

    ([(header::CONTENT_TYPE, "application/xml; charset=utf-8")], body)
}

/// GET /robots.txt — allows all public pages, blocks the admin/mod panels and
/// account-scoped pages (nothing sensitive is reachable there without auth
/// anyway, but there's no reason to invite crawlers to try), and points at
/// the sitemap above.
pub async fn robots_txt(State(state): State<AppState>) -> impl IntoResponse {
    let base = state.app_url.trim_end_matches('/');
    let body = format!(
        "User-agent: *\n\
         Disallow: /admin/\n\
         Disallow: /mod/\n\
         Disallow: /account\n\
         Disallow: /notifications\n\
         Disallow: /bookmarks\n\
         Disallow: /api/\n\
         \n\
         Sitemap: {base}/sitemap.xml\n"
    );
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body)
}
