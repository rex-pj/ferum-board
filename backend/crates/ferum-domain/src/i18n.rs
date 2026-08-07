//! Translation primitives shared across layers.
//!
//! `TransArg` lives in the domain rather than beside the `Translator` port
//! because `AppError` carries translation arguments, and the domain cannot
//! depend on the application crate. It is a pure value type with no behaviour,
//! so domain is a legitimate home for it.

/// A value interpolated into a translated string.
///
/// The numeric variants are not a convenience — they are load-bearing. Plural
/// selection is driven by the *type* of the argument: a catalog entry that
/// branches on `$count` can only pick the right CLDR plural category if it
/// receives a number rather than a pre-formatted string. Passing `Str("2")`
/// where `Int(2)` was meant silently degrades every pluralized message to its
/// catch-all branch, in every language, without erroring.
#[derive(Clone, Debug, PartialEq)]
pub enum TransArg {
    Str(String),
    Int(i64),
    Float(f64),
}

impl From<&str> for TransArg {
    fn from(v: &str) -> Self {
        TransArg::Str(v.to_string())
    }
}
impl From<String> for TransArg {
    fn from(v: String) -> Self {
        TransArg::Str(v)
    }
}
impl From<i64> for TransArg {
    fn from(v: i64) -> Self {
        TransArg::Int(v)
    }
}
impl From<u64> for TransArg {
    fn from(v: u64) -> Self {
        TransArg::Int(v as i64)
    }
}
impl From<usize> for TransArg {
    fn from(v: usize) -> Self {
        TransArg::Int(v as i64)
    }
}
impl From<i32> for TransArg {
    fn from(v: i32) -> Self {
        TransArg::Int(v as i64)
    }
}
impl From<i16> for TransArg {
    fn from(v: i16) -> Self {
        TransArg::Int(v as i64)
    }
}
impl From<u16> for TransArg {
    fn from(v: u16) -> Self {
        TransArg::Int(v as i64)
    }
}
impl From<u32> for TransArg {
    fn from(v: u32) -> Self {
        TransArg::Int(v as i64)
    }
}
impl From<f64> for TransArg {
    fn from(v: f64) -> Self {
        TransArg::Float(v)
    }
}

/// Maps a machine error code to its catalog key.
///
/// Fluent message identifiers are `[a-zA-Z][a-zA-Z0-9_-]*` — dots are illegal
/// and underscores, while legal, read badly next to Fluent convention. Error
/// codes are snake_case by long-standing convention in this codebase and are
/// part of the public API contract, so they are translated to kebab-case here
/// rather than being renamed at the source.
///
/// `thread_locked` → `error-thread-locked`
pub fn error_key(code: &str) -> String {
    format!("error-{}", code.replace('_', "-"))
}
