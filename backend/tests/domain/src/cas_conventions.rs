//! Enforces how a CAS reference is given back, by scanning source.
//!
//! Releasing a reference is a **two-step** operation — decrement, then collect
//! only if that reached zero — and only the second step deletes the stored
//! object. Writing it inline is how three call sites came to be wrong at once:
//! the site logo, the favicon and theme previews all deleted the `stored_files`
//! row and left the object in the bucket, unreachable and uncountable.
//!
//! None of it was visible in development, because `DatabaseStorageService`
//! keeps the bytes *in* the row. It only surfaced on S3/GCS/R2 — the production
//! configuration — which is exactly why a convention test earns its place here
//! rather than a code review checklist.
//!
//! A test rather than a CI grep, for the same reason as
//! `timezone_conventions.rs`: it runs everywhere and fails beside the change
//! that caused it.

use std::path::{Path, PathBuf};

/// Production source only. Test crates legitimately implement the repository
/// trait — including the method banned below — as doubles.
const SCANNED_ROOTS: [&str; 1] = ["crates"];

/// The one file allowed to name `delete_by_key`: its own implementation.
///
/// The trait declaration and the doc comment on `release_cas_ref` are matched
/// by the comment filter instead.
const IMPLEMENTATION: &str = "repositories/stored_file_repository.rs";

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

fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with("*") || t.starts_with("/*")
}

fn normalise(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[test]
fn nothing_outside_the_repository_calls_delete_by_key() {
    // `delete_by_key` removes the row and nothing else. Under an object store
    // that strands the bytes forever — and worse, removes the only record that
    // could ever find them again. `release_cas_ref` is the correct call: it
    // decrements, and enqueues `GcStorageKey`, which deletes row *and* object
    // and re-checks the count while holding the row lock.
    let mut offenders = Vec::new();

    for path in all_sources() {
        let display = normalise(&path);
        if display.ends_with(IMPLEMENTATION) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            if !is_comment(line) && line.contains("delete_by_key") {
                offenders.push(format!("{display}:{}", i + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "these call `delete_by_key`, which deletes the stored_files row without \
         deleting the object behind it — use \
         `ferum_application::storage_utils::release_cas_ref` instead:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn nothing_decrements_a_reference_without_scheduling_collection() {
    // The inline two-step form. Every remaining use of `decrement_ref` outside
    // the helper is a place where somebody must remember to enqueue
    // `GcStorageKey` on zero — and the ones that forgot are why this file
    // exists. New code should call `release_cas_ref`.
    //
    // The pre-existing sites are listed rather than rewritten: each carries
    // extra behaviour around the decrement (rollback on a failed write, walking
    // a plugin's whole namespace, deliberately *not* collecting for post
    // attachments), so folding them in would change behaviour, not just shape.
    const KNOWN: [&str; 5] = [
        "usecases/user_usecase.rs",
        "usecases/thread_usecase.rs",
        "usecases/post_usecase.rs",
        "usecases/product_usecase.rs",
        "usecases/plugin_usecase.rs",
    ];

    let mut offenders = Vec::new();
    for path in all_sources() {
        let display = normalise(&path);
        if display.ends_with("storage_utils.rs")
            || display.ends_with("repositories/stored_file_repository.rs")
            || KNOWN.iter().any(|k| display.ends_with(k))
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            if !is_comment(line) && line.contains(".decrement_ref(") {
                offenders.push(format!("{display}:{}", i + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "these decrement a CAS reference inline. Unless the surrounding code \
         needs something `release_cas_ref` cannot express, call that instead — \
         forgetting the `GcStorageKey` enqueue leaks the object silently:\n  {}",
        offenders.join("\n  ")
    );
}
