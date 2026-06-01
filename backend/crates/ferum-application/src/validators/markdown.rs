use pulldown_cmark::{html, Options, Parser};

/// Maximum raw Markdown length accepted before rendering is refused.
pub const MAX_MARKDOWN_BYTES: usize = crate::constants::MAX_POST_CONTENT_BYTES;

/// Convert Markdown to sanitized HTML.
/// Returns `None` when `md` exceeds [`MAX_MARKDOWN_BYTES`].
/// Call this in use cases before storing `content_html`.
pub fn render_and_sanitize(md: &str) -> Option<String> {
    if md.len() > MAX_MARKDOWN_BYTES {
        return None;
    }

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(md, opts);
    let mut raw_html = String::new();
    html::push_html(&mut raw_html, parser);

    // Allowed URL schemes for <a href> and <img src>.
    // This blocks javascript:, data:, vbscript: and other dangerous protocols.
    let mut allowed_schemes = std::collections::HashSet::new();
    allowed_schemes.insert("http");
    allowed_schemes.insert("https");

    let result = ammonia::Builder::default()
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
        .url_schemes(allowed_schemes)
        .clean(&raw_html)
        .to_string();

    Some(result)
}
