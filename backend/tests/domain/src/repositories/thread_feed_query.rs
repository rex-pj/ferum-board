//! `parse_feed_query` — the one place that knows how a listing URL is spelled,
//! including the pre-split vocabulary.
//!
//! Worth testing exhaustively because every failure mode here is SILENT. A
//! dropped legacy value does not error; it quietly serves the default listing,
//! so a bookmark that used to show unanswered threads starts showing everything
//! and nothing anywhere reports a problem. The function is pure — no DB, no
//! HTTP — so the whole matrix costs microseconds.

use ferum_domain::repositories::thread_repository::{
    parse_feed_query, ThreadFeedFilter, ThreadSort,
};

// ─── Current vocabulary ───────────────────────────────────────────────────────

#[test]
fn current_sort_values_parse_and_are_not_flagged_legacy() {
    for (input, expected) in [
        ("activity", ThreadSort::Activity),
        ("newest", ThreadSort::Newest),
        ("most_replies", ThreadSort::MostReplies),
    ] {
        let (sort, filter, legacy) = parse_feed_query(Some(input), None);
        assert_eq!(sort, expected, "sort={input}");
        assert_eq!(filter, ThreadFeedFilter::All, "sort={input}");
        assert!(!legacy, "sort={input} must not trigger a redirect");
    }
}

#[test]
fn current_filter_values_parse_and_are_not_flagged_legacy() {
    for (input, expected) in [
        ("unanswered", ThreadFeedFilter::Unanswered),
        ("solved", ThreadFeedFilter::Solved),
    ] {
        let (sort, filter, legacy) = parse_feed_query(None, Some(input));
        assert_eq!(sort, ThreadSort::Activity, "filter={input}");
        assert_eq!(filter, expected, "filter={input}");
        assert!(!legacy, "filter={input} must not trigger a redirect");
    }
}

/// The combination the pre-split URL scheme could not express at all, and the
/// entire reason the two axes were separated.
#[test]
fn sort_and_filter_are_independent_axes() {
    let (sort, filter, legacy) = parse_feed_query(Some("newest"), Some("unanswered"));
    assert_eq!(sort, ThreadSort::Newest);
    assert_eq!(filter, ThreadFeedFilter::Unanswered);
    assert!(!legacy);
}

// ─── Pre-split vocabulary ─────────────────────────────────────────────────────

#[test]
fn legacy_sort_only_values_map_to_the_same_ordering() {
    for (input, expected) in [
        ("latest", ThreadSort::Activity),
        ("hottest", ThreadSort::MostReplies),
    ] {
        let (sort, filter, legacy) = parse_feed_query(Some(input), None);
        assert_eq!(sort, expected, "sort={input}");
        assert_eq!(filter, ThreadFeedFilter::All, "sort={input}");
        assert!(legacy, "sort={input} is a pre-split spelling and must redirect");
    }
}

/// `unanswered` / `solved` were filters wearing a sort's name. They carry no
/// ordering of their own, so they resolve to the default ordering — which is
/// also the one they actually used before the split.
#[test]
fn legacy_filter_values_move_to_the_filter_axis() {
    for (input, expected) in [
        ("unanswered", ThreadFeedFilter::Unanswered),
        ("solved", ThreadFeedFilter::Solved),
    ] {
        let (sort, filter, legacy) = parse_feed_query(Some(input), None);
        assert_eq!(sort, ThreadSort::Activity, "sort={input}");
        assert_eq!(filter, expected, "sort={input}");
        assert!(legacy, "sort={input} is a pre-split spelling and must redirect");
    }
}

/// A half-updated bookmark: new-vocabulary `filter` alongside a legacy `sort`.
/// The explicit filter is the caller stating intent and wins.
#[test]
fn explicit_filter_overrides_one_implied_by_a_legacy_sort() {
    let (sort, filter, legacy) = parse_feed_query(Some("unanswered"), Some("solved"));
    assert_eq!(sort, ThreadSort::Activity);
    assert_eq!(filter, ThreadFeedFilter::Solved);
    assert!(legacy);
}

// ─── Absent and unrecognised input ────────────────────────────────────────────

#[test]
fn absent_and_unknown_input_falls_back_to_defaults_without_redirecting() {
    for (sort_in, filter_in) in [
        (None, None),
        (Some("wat"), None),
        (None, Some("wat")),
        (Some(""), Some("")),
    ] {
        let (sort, filter, legacy) = parse_feed_query(sort_in, filter_in);
        assert_eq!(sort, ThreadSort::Activity, "input={sort_in:?}/{filter_in:?}");
        assert_eq!(
            filter,
            ThreadFeedFilter::All,
            "input={sort_in:?}/{filter_in:?}"
        );
        // Junk is not a legacy spelling: redirecting would turn a typo into a
        // 301 that teaches caches a URL nobody asked for.
        assert!(!legacy, "input={sort_in:?}/{filter_in:?}");
    }
}

// ─── Round trip ───────────────────────────────────────────────────────────────

/// `as_str` must produce something `parse_feed_query` reads back identically,
/// or the URLs the application generates disagree with the ones it accepts.
#[test]
fn as_str_round_trips_through_the_parser() {
    for sort in [ThreadSort::Activity, ThreadSort::Newest, ThreadSort::MostReplies] {
        for filter in [
            ThreadFeedFilter::All,
            ThreadFeedFilter::Unanswered,
            ThreadFeedFilter::Solved,
        ] {
            let (parsed_sort, parsed_filter, legacy) =
                parse_feed_query(Some(sort.as_str()), Some(filter.as_str()));
            assert_eq!(parsed_sort, sort);
            assert_eq!(parsed_filter, filter);
            assert!(!legacy, "generated URLs must never be flagged as legacy");
        }
    }
}

/// `ThreadSort::from_label` is the ordering-only door used by the admin list. It
/// must agree with the full parser rather than carry a second table.
#[test]
fn thread_sort_from_str_agrees_with_the_full_parser() {
    for input in [
        "activity",
        "newest",
        "most_replies",
        "latest",
        "hottest",
        "unanswered",
        "solved",
        "junk",
    ] {
        assert_eq!(
            ThreadSort::from_label(input),
            parse_feed_query(Some(input), None).0,
            "input={input}"
        );
    }
}
