use std::sync::LazyLock;

use pulldown_cmark::{html, Options, Parser};

/// Maximum raw Markdown length accepted before rendering is refused.
pub const MAX_MARKDOWN_BYTES: usize = crate::constants::MAX_POST_CONTENT_BYTES;

/// The sanitizer applied to every rendered post body.
///
/// Built once. `ammonia::Builder::default()` is not a cheap struct literal — it
/// populates the crate's full default allow-lists (tags, per-tag attributes, URL
/// schemes, relative-URL policy) into several hash sets, and this function then
/// replaced most of them with its own. That whole construction used to run on
/// every post create, edit and Markdown preview, and was discarded immediately
/// after one `clean` call.
///
/// `clean` takes `&self`, so one shared instance serves every caller.
static POST_SANITIZER: LazyLock<ammonia::Builder<'static>> = LazyLock::new(|| {
    // Allowed URL schemes for <a href> and <img src>.
    // This blocks javascript:, data:, vbscript: and other dangerous protocols.
    let allowed_schemes = std::collections::HashSet::from(["http", "https"]);

    let mut builder = ammonia::Builder::default();
    builder
        .tags(std::collections::HashSet::from([
            "p",
            "br",
            "strong",
            "em",
            "del",
            "code",
            "pre",
            "blockquote",
            "ul",
            "ol",
            "li",
            "h1",
            "h2",
            "h3",
            "h4",
            "h5",
            "h6",
            "a",
            "img",
            "table",
            "thead",
            "tbody",
            "tr",
            "th",
            "td",
        ]))
        .allowed_classes(Default::default())
        .link_rel(Some("nofollow noopener noreferrer"))
        .url_schemes(allowed_schemes);
    builder
});

/// Markdown extensions enabled for post bodies. `Options` is a bitflag set, so
/// this is a plain constant rather than a lazy one.
static MARKDOWN_OPTIONS: LazyLock<Options> = LazyLock::new(|| {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts
});

/// Convert Markdown to sanitized HTML.
/// Returns `None` when `md` exceeds [`MAX_MARKDOWN_BYTES`].
/// Call this in use cases before storing `content_html`.
pub fn render_and_sanitize(md: &str) -> Option<String> {
    if md.len() > MAX_MARKDOWN_BYTES {
        return None;
    }

    let parser = Parser::new_ext(md, *MARKDOWN_OPTIONS);
    let mut raw_html = String::new();
    html::push_html(&mut raw_html, parser);

    Some(POST_SANITIZER.clean(&raw_html).to_string())
}
