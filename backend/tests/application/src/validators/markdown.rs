use ferum_application::validators::markdown::{render_and_sanitize, MAX_MARKDOWN_BYTES};

// ─── Basic rendering ──────────────────────────────────────────────────────────

#[test]
fn plain_text_is_wrapped_in_paragraph() {
    let html = render_and_sanitize("hello world").unwrap();
    assert!(html.contains("hello world"));
    assert!(html.contains("<p>"));
}

#[test]
fn bold_syntax_renders_strong_tag() {
    let html = render_and_sanitize("**bold**").unwrap();
    assert!(html.contains("<strong>bold</strong>"));
}

#[test]
fn italic_syntax_renders_em_tag() {
    let html = render_and_sanitize("*italic*").unwrap();
    assert!(html.contains("<em>italic</em>"));
}

#[test]
fn strikethrough_renders_del_tag() {
    let html = render_and_sanitize("~~strike~~").unwrap();
    assert!(html.contains("<del>strike</del>"));
}

#[test]
fn inline_code_renders_code_tag() {
    let html = render_and_sanitize("`code`").unwrap();
    assert!(html.contains("<code>code</code>"));
}

#[test]
fn fenced_code_block_renders_pre_code() {
    let html = render_and_sanitize("```\nfn main() {}\n```").unwrap();
    assert!(html.contains("<pre>") || html.contains("<code>"));
}

#[test]
fn headings_h1_to_h3_are_allowed() {
    assert!(render_and_sanitize("# H1").unwrap().contains("<h1>"));
    assert!(render_and_sanitize("## H2").unwrap().contains("<h2>"));
    assert!(render_and_sanitize("### H3").unwrap().contains("<h3>"));
}

#[test]
fn unordered_list_renders_ul_li() {
    let html = render_and_sanitize("- item1\n- item2").unwrap();
    assert!(html.contains("<ul>"));
    assert!(html.contains("<li>"));
}

#[test]
fn ordered_list_renders_ol_li() {
    let html = render_and_sanitize("1. first\n2. second").unwrap();
    assert!(html.contains("<ol>"));
    assert!(html.contains("<li>"));
}

#[test]
fn blockquote_renders_blockquote_tag() {
    let html = render_and_sanitize("> quoted text").unwrap();
    assert!(html.contains("<blockquote>"));
}

#[test]
fn table_extension_renders_table_tags() {
    let md = "| Col1 | Col2 |\n|------|------|\n| A    | B    |";
    let html = render_and_sanitize(md).unwrap();
    assert!(html.contains("<table>"));
    assert!(html.contains("<th>") || html.contains("<td>"));
}

// ─── XSS sanitization ────────────────────────────────────────────────────────

#[test]
fn script_tag_is_stripped() {
    let html = render_and_sanitize("<script>alert('xss')</script>safe text").unwrap();
    assert!(!html.contains("<script>"), "script tag must be removed");
    assert!(!html.contains("alert("), "script content must be removed");
}

#[test]
fn onclick_attribute_is_stripped() {
    let html = render_and_sanitize(r#"<p onclick="evil()">click</p>"#).unwrap();
    assert!(!html.contains("onclick"), "event handler attributes must be stripped");
}

#[test]
fn javascript_href_is_stripped() {
    let html = render_and_sanitize(r#"[click](javascript:alert(1))"#).unwrap();
    assert!(!html.contains("javascript:"), "javascript: scheme must be blocked");
}

#[test]
fn data_uri_href_is_stripped() {
    let html = render_and_sanitize(r#"[img](data:text/html,<script>evil()</script>)"#).unwrap();
    assert!(!html.contains("data:"), "data: URI scheme must be blocked");
}

#[test]
fn http_link_is_allowed() {
    let html = render_and_sanitize("[link](http://example.com)").unwrap();
    assert!(html.contains("http://example.com"), "http links must be preserved");
}

#[test]
fn https_link_is_allowed() {
    let html = render_and_sanitize("[link](https://example.com)").unwrap();
    assert!(html.contains("https://example.com"), "https links must be preserved");
}

#[test]
fn link_gets_nofollow_rel() {
    let html = render_and_sanitize("[link](https://example.com)").unwrap();
    assert!(html.contains("nofollow"), "links must have rel=nofollow");
}

#[test]
fn iframe_is_stripped() {
    let html = render_and_sanitize(r#"<iframe src="https://evil.com"></iframe>"#).unwrap();
    assert!(!html.contains("<iframe>"), "iframe must be stripped");
}

#[test]
fn style_attribute_is_stripped() {
    let html = render_and_sanitize(r#"<p style="color:red">text</p>"#).unwrap();
    assert!(!html.contains("style="), "style attributes must be stripped");
}

// ─── Size guard ───────────────────────────────────────────────────────────────

#[test]
fn content_within_limit_returns_some() {
    let md = "a".repeat(MAX_MARKDOWN_BYTES);
    assert!(render_and_sanitize(&md).is_some());
}

#[test]
fn content_exceeding_limit_returns_none() {
    let md = "a".repeat(MAX_MARKDOWN_BYTES + 1);
    assert!(render_and_sanitize(&md).is_none());
}

#[test]
fn empty_string_returns_some_empty_or_whitespace() {
    let result = render_and_sanitize("");
    assert!(result.is_some(), "empty string should not be rejected");
    assert!(result.unwrap().trim().is_empty());
}
