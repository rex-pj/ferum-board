//! Enforces the workspace's timezone conventions by scanning source.
//!
//! Written as a test rather than a CI grep on purpose. A grep in a workflow file
//! only runs on push, only on Linux, and is invisible to whoever is actually
//! writing the code; this runs in every `cargo test --workspace`, on every
//! platform, and fails next to the change that caused it. The repo already
//! scans source this way — `models/plugin.rs` reads every `plugin.toml`, and
//! `tests/web/src/template_integrity.rs` walks the template tree.
//!
//! ## The conventions
//!
//! 1. **Instants are `DateTime<Utc>`.** Nothing may read the ambient process
//!    timezone: it makes behaviour depend on where the binary happens to run,
//!    and it is invisible to everyone whose machine is already UTC.
//! 2. **"Today" in SQL is named, not inherited.** `CURRENT_DATE` and `NOW()`
//!    resolve against the session `TimeZone`, so any day boundary built on them
//!    is only as stable as the server's configuration.
//!
//! Both rules were clean-ish when written and are cheap to keep that way; the
//! expensive version of this file is the one written after a shipped bug.

use std::path::{Path, PathBuf};

/// Source roots that must obey the conventions. Test crates are deliberately
/// excluded: a test is *supposed* to be able to construct a fixed offset or
/// pin a hostile session timezone, which is precisely what these rules forbid
/// in production code.
const SCANNED_ROOTS: [&str; 2] = ["crates", "migration"];

fn backend() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn all_sources() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in SCANNED_ROOTS {
        rust_files(&backend().join(root), &mut out);
    }
    assert!(
        !out.is_empty(),
        "found no Rust sources to scan — the path walk is broken, which would \
         make every test in this file pass vacuously"
    );
    out
}

/// True for a line that is entirely a comment.
///
/// Deliberately not a full comment stripper. Removing trailing `//` needs string
/// awareness (`"postgres://localhost"` contains `//`), and getting that wrong
/// silently shrinks what is scanned — a guard that quietly stops guarding is
/// worse than none. Whole-line comments cover the real case, which is prose in
/// a doc comment naming the very construct being banned, as this file's own
/// header does.
fn is_comment_line(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with("* ") || t.starts_with("*/") || t.starts_with("/*")
}

fn scan(needles: &[(&str, &str)], allowlist: &[&str]) -> Vec<String> {
    let mut findings = Vec::new();
    for path in all_sources() {
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        if allowlist.contains(&name.as_str()) {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(&path) else { continue };
        for (n, line) in src.lines().enumerate() {
            if is_comment_line(line) {
                continue;
            }
            for (needle, why) in needles {
                if line.contains(needle) {
                    findings.push(format!(
                        "{}:{}\n      found: {needle}\n      why:   {why}\n      line:  {}",
                        path.display(),
                        n + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    findings
}

#[test]
fn no_source_reads_the_ambient_process_timezone() {
    let findings = scan(
        &[
            (
                "Local::now()",
                "reads the machine's timezone — the same code then behaves \
                 differently in dev and prod. Use Utc::now().",
            ),
            (
                "naive_local()",
                "discards the offset and yields a wall-clock reading of the \
                 ambient zone. Use Utc::now(), or .naive_utc() if a naive value \
                 is genuinely wanted.",
            ),
            (
                "and_local_timezone",
                "interprets a naive value in the ambient zone. If the value is \
                 UTC use .and_utc(); if it is in a named zone, name the zone.",
            ),
            (
                "Local>",
                "DateTime<Local> makes the ambient zone part of a type. Instants \
                 are DateTime<Utc>; convert at the presentation boundary only.",
            ),
            (
                "FixedOffset::east",
                "a hardcoded offset breaks under DST and future timezone-law \
                 changes. Store an IANA zone name instead.",
            ),
            (
                "FixedOffset::west",
                "a hardcoded offset breaks under DST and future timezone-law \
                 changes. Store an IANA zone name instead.",
            ),
        ],
        // `.fixed_offset()` is unavoidable at the sea-orm boundary (TIMESTAMPTZ
        // maps to DateTime<FixedOffset>) and is not banned; it is always paired
        // with a `.with_timezone(&Utc)` on the way back in.
        &[],
    );

    assert!(
        findings.is_empty(),
        "\n\nAmbient-timezone usage found in {} place(s).\n\n{}\n\n\
         Instants are DateTime<Utc> end to end; conversion to a viewer's zone \
         happens in the browser, never in Rust. See CLAUDE.md \u{2192} \
         \"Time and timezones\".\n",
        findings.len(),
        findings.join("\n\n")
    );
}

#[test]
fn sql_day_boundaries_are_named_rather_than_inherited() {
    // `CURRENT_DATE` / `NOW()` resolve against the session TimeZone. startup.rs
    // pins that to UTC, but the pin has an operator escape hatch, so any query
    // that means a *reporting* day must convert explicitly with AT TIME ZONE
    // rather than trusting the session.
    let findings = scan(
        &[
            (
                "CURRENT_DATE",
                "resolves against the session TimeZone. For a reporting day \
                 boundary convert explicitly: (ts AT TIME ZONE <reporting_tz>)::date.",
            ),
            (
                "current_date()",
                "sea-query's Expr::current_date() emits CURRENT_DATE — same \
                 session-TimeZone dependency.",
            ),
        ],
        // Allowlist, kept as short as it can be. Every entry here is a file
        // where the day boundary is applied deliberately and is covered by a
        // test that runs under a hostile session timezone.
        &[
            // Buckets analytics by the configured `reporting_timezone`.
            "stats_repository.rs",
            // days_visited: compares two dates in one named zone.
            "user_repository.rs",
            // The doc comment on SESSION_TIME_ZONE explains the pin itself.
            "startup.rs",
        ],
    );

    assert!(
        findings.is_empty(),
        "\n\nSession-dependent day boundary in {} place(s).\n\n{}\n\n\
         If the file genuinely owns a reviewed day boundary, add it to the \
         allowlist in this test together with a test that exercises it under a \
         non-UTC session timezone (see TestDb::new_in_timezone).\n",
        findings.len(),
        findings.join("\n\n")
    );
}

#[test]
fn the_scan_actually_matches_something() {
    // A guard whose file walk silently returns nothing passes forever. This
    // asserts the machinery finds a string that is definitely present, so a
    // broken path or a changed layout fails loudly instead of going quiet.
    let hits = scan(&[("Utc::now", "sentinel")], &[]);
    assert!(
        !hits.is_empty(),
        "the scanner found no `Utc::now` anywhere in crates/ or migration/, \
         which cannot be true — the file walk or the matcher is broken"
    );
}
