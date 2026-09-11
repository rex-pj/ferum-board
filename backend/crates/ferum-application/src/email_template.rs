//! Rendering and validation for transactional email copy.
//!
//! Pure functions only — resolving which row to use is the renderer port's job.
//!
//! **The syntax is `{{ name }}` and nothing else.** No filters, no conditionals,
//! no `{% %}`. That is a deliberate narrowing rather than a missing feature: the
//! copy is edited by an admin in a textarea, and every richer option was worse
//! for that. Fluent treats `{` as special and a paste containing `style="{…}"`
//! breaks the whole bundle; Tera would hand the admin `{% include %}` against
//! the template directory. Neither can express the rule that actually matters
//! here — that a thread title and an href need *different* escaping.
//!
//! The scanners below walk `&str` by `str::find` offsets, which are always char
//! boundaries — but they read through `get` rather than `[..]` so that stays a
//! property of the code instead of a claim in a comment. Every `else { break }`
//! in them is unreachable, and each degrades to "stop parsing, emit what we
//! have", which is what the unclosed-`{{` branch already did deliberately.

use std::collections::HashMap;

use ferum_domain::models::email_template::{template_def, VarDef, VarKind};

use crate::ports::TransArg;
use crate::shared::AppError;

/// One message, ready to hand to the provider.
pub struct RenderedEmail {
    pub subject: String,
    pub html: String,
    /// The `text/plain` alternative, derived from the HTML body.
    ///
    /// Derived rather than authored: a second editable field would double the
    /// work on every copy change and drift from the HTML the first time someone
    /// edited one and not the other.
    pub text: String,
}

impl RenderedEmail {
    /// Addresses this message, for handing straight to
    /// [`EmailService::send`](crate::ports::EmailService::send).
    ///
    /// Exists so no call site spells out which field is the HTML and which the
    /// text — the pairing is decided once, here.
    pub fn to<'a>(&'a self, recipient: &'a str) -> crate::ports::OutgoingEmail<'a> {
        crate::ports::OutgoingEmail {
            to: recipient,
            subject: &self.subject,
            html: &self.html,
            text: &self.text,
        }
    }
}

/// Escapes a value for an HTML text node.
///
/// `&#39;` rather than XML's `&apos;`, which predates HTML5 and is not defined
/// in HTML 4.
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Reduces a URL to something safe to put in an `href`.
///
/// Every URL reaching this is built by the application, so this is
/// defence-in-depth rather than a filter on user input — but `href` is the one
/// place where a wrong value executes rather than merely displays, and the cost
/// of the check is nothing.
pub fn sanitize_url(s: &str) -> String {
    let lowered = s.trim().to_ascii_lowercase();
    if lowered.starts_with("http://") || lowered.starts_with("https://") {
        escape_html(s.trim())
    } else {
        // Not silently dropped: a link that visibly goes nowhere is a bug
        // report, an invisible one is a mystery.
        "#".to_string()
    }
}

/// Every `{{ name }}` a template names, in source order, deduplicated.
pub fn placeholders(template: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let Some(after) = rest.get(start + 2..) else { break };
        let Some(end) = after.find("}}") else { break };
        let (Some(name), Some(next)) = (after.get(..end), after.get(end + 2..)) else {
            break;
        };
        let name = name.trim();
        if !name.is_empty() && !found.iter().any(|f: &String| f == name) {
            found.push(name.to_string());
        }
        rest = next;
    }
    found
}

/// Replaces every `{{ name }}` with its value.
///
/// `html` selects the escaping policy, and the two callers are not
/// interchangeable: a body is HTML and escapes per [`VarKind`]; a subject is
/// plain text, where escaping would show the recipient a literal `&amp;` in
/// their inbox list.
///
/// An unknown or unsupplied name is left standing as written, matching what the
/// translator used to do with a missing key: visibly wrong beats silently
/// empty, because only one of the two gets reported.
pub fn substitute(
    template: &str,
    values: &HashMap<&str, String>,
    defs: &[VarDef],
    html: bool,
) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        let Some(after) = rest.get(start + 2..) else { break };
        let Some(end) = after.find("}}") else {
            // An unclosed `{{` is the rest of the template, verbatim.
            out.push_str(rest);
            return out;
        };
        // Read before anything is emitted, so a `break` leaves `rest` intact for
        // the tail push below rather than half-copying it.
        let (Some(head), Some(name), Some(next)) =
            (rest.get(..start), after.get(..end), after.get(end + 2..))
        else {
            break;
        };
        let name = name.trim();
        out.push_str(head);

        match values.get(name) {
            Some(value) if html => {
                let kind = defs
                    .iter()
                    .find(|d| d.name == name)
                    .map(|d| d.kind)
                    // A value with no declaration is treated as prose. Erring
                    // toward escaping is the only safe default here.
                    .unwrap_or(VarKind::Text);
                match kind {
                    VarKind::Text => out.push_str(&escape_html(value)),
                    VarKind::Url => out.push_str(&sanitize_url(value)),
                    VarKind::Raw => out.push_str(value),
                }
            }
            Some(value) => out.push_str(value),
            None => {
                out.push_str("{{ ");
                out.push_str(name);
                out.push_str(" }}");
            }
        }
        rest = next;
    }

    out.push_str(rest);
    out
}

/// A readable `text/plain` rendering of an HTML body.
///
/// Keeps link targets — `<a href="X">Y</a>` becomes `Y (X)` — because the whole
/// point of most of these messages is a link, and a plain-text part that drops
/// it is worse than none.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(start) = rest.find('<') {
        let Some(after) = rest.get(start..) else { break };
        let Some(end) = after.find('>') else { break };
        let (Some(head), Some(tag), Some(body)) =
            (rest.get(..start), after.get(..=end), after.get(end + 1..))
        else {
            break;
        };
        out.push_str(&decode_entities(head));
        let lowered = tag.to_ascii_lowercase();

        if lowered.starts_with("<a ") {
            if let Some(href) = attr_value(tag, "href") {
                // Offsets relative to `body` rather than absolute into `after`:
                // the same two slices, with no arithmetic to get wrong.
                if let Some(close) = body.find("</a>") {
                    let (Some(inner), Some(next)) = (body.get(..close), body.get(close + 4..))
                    else {
                        break;
                    };
                    // Emitted after the label, so the sentence still reads.
                    let label = html_to_text(inner);
                    let label = label.trim();
                    if label == href {
                        out.push_str(&href);
                    } else {
                        out.push_str(label);
                        out.push_str(" (");
                        out.push_str(&href);
                        out.push(')');
                    }
                    rest = next;
                    continue;
                }
            }
        } else if lowered.starts_with("<br")
            || lowered.starts_with("</p")
            || lowered.starts_with("<hr")
            || lowered.starts_with("</div")
            || lowered.starts_with("</h")
        {
            out.push('\n');
        }

        rest = body;
    }
    out.push_str(&decode_entities(rest));

    // Collapse the runs of blank lines that dropping block tags leaves behind.
    let mut text = String::with_capacity(out.len());
    let mut blank_run = 0usize;
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        text.push_str(line);
        text.push('\n');
    }
    text.trim().to_string()
}

/// The five entities [`escape_html`] produces, reversed. Deliberately not a
/// general HTML entity decoder — this only ever sees output we generated.
fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        // Last, or an escaped `&amp;lt;` would decode twice.
        .replace("&amp;", "&")
}

fn attr_value(tag: &str, attr: &str) -> Option<String> {
    let lowered = tag.to_ascii_lowercase();
    let at = lowered.find(&format!("{attr}=\""))?;
    let value = tag.get(at + attr.len() + 2..)?;
    let end = value.find('"')?;
    Some(decode_entities(value.get(..end)?))
}

/// Checks a template an admin is trying to save.
///
/// **Runs on save, never on send.** A template that cannot render is a mistake
/// to reject while the author is looking at it, not a broken email discovered
/// by its recipient.
///
/// # Errors
/// - `unknown_email_template` — no such key in the catalogue.
/// - `email_template_logic_unsupported` — contains `{%`.
/// - `email_template_unknown_variable` — names something the template has no value for.
/// - `email_template_missing_variable` — omits one the message cannot work without.
pub fn validate_template(key: &str, subject: &str, body_html: &str) -> Result<(), AppError> {
    let Some(def) = template_def(key) else {
        return Err(AppError::invalid("unknown_email_template"));
    };

    // Caught explicitly rather than falling through to "unknown variable": an
    // author typing `{% if %}` has a wrong idea about what this is, and the
    // generic message would not correct it.
    if subject.contains("{%") || body_html.contains("{%") {
        return Err(AppError::invalid("email_template_logic_unsupported"));
    }

    for field in [subject, body_html] {
        for name in placeholders(field) {
            if !def.vars.iter().any(|v| v.name == name) {
                return Err(AppError::invalid_with(
                    "email_template_unknown_variable",
                    [("name", TransArg::Str(name))],
                ));
            }
        }
    }

    let in_body = placeholders(body_html);
    for var in def.vars.iter().filter(|v| v.required_in_body) {
        if !in_body.iter().any(|p| p == var.name) {
            return Err(AppError::invalid_with(
                "email_template_missing_variable",
                [("name", TransArg::Str(var.name.to_string()))],
            ));
        }
    }

    Ok(())
}
