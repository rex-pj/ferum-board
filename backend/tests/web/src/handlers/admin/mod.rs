mod api;
mod pages;

use ferum_web::handlers::admin::{parse_opt_uuid, PageQuery};
use uuid::Uuid;

// ─── parse_opt_uuid ───────────────────────────────────────────────────────────

#[test]
fn parse_opt_uuid_none_input_returns_none() {
    assert!(parse_opt_uuid(None).is_none());
}

#[test]
fn parse_opt_uuid_empty_string_returns_none() {
    assert!(parse_opt_uuid(Some("")).is_none());
}

#[test]
fn parse_opt_uuid_invalid_string_returns_none() {
    assert!(parse_opt_uuid(Some("not-a-uuid")).is_none());
}

#[test]
fn parse_opt_uuid_valid_uuid_returns_some() {
    let id = Uuid::new_v4();
    let parsed = parse_opt_uuid(Some(&id.to_string()));
    assert_eq!(parsed, Some(id));
}

#[test]
fn parse_opt_uuid_whitespace_string_returns_none() {
    assert!(parse_opt_uuid(Some("   ")).is_none());
}

// ─── PageQuery ────────────────────────────────────────────────────────────────

#[test]
fn page_query_all_optional_fields() {
    let q: PageQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.page.is_none());
    assert!(q.per_page.is_none());
    assert!(q.q.is_none());
    assert!(q.status.is_none());
    assert!(q.sort_by.is_none());
    assert!(q.sort_dir.is_none());
    assert!(q.actor_id.is_none());
    assert!(q.target_type.is_none());
    assert!(q.category_id.is_none());
    assert!(q.author_id.is_none());
    assert!(q.date_from.is_none());
    assert!(q.date_to.is_none());
}

#[test]
fn page_query_pagination_fields_deserialize() {
    let q: PageQuery =
        serde_json::from_str(r#"{"page":2,"per_page":25,"q":"test"}"#).unwrap();
    assert_eq!(q.page, Some(2));
    assert_eq!(q.per_page, Some(25));
    assert_eq!(q.q.as_deref(), Some("test"));
}

#[test]
fn page_query_filter_fields_deserialize() {
    let q: PageQuery = serde_json::from_str(
        r#"{"status":"pending","sort_by":"created_at","sort_dir":"desc","target_type":"thread"}"#,
    )
    .unwrap();
    assert_eq!(q.status.as_deref(), Some("pending"));
    assert_eq!(q.sort_by.as_deref(), Some("created_at"));
    assert_eq!(q.sort_dir.as_deref(), Some("desc"));
    assert_eq!(q.target_type.as_deref(), Some("thread"));
}

// ─── Admin date-range filters ─────────────────────────────────────────────────
//
// `?date_from=2026-08-08&date_to=2026-08-08` is an admin saying "the 8th". Two
// things have to be true for that to mean what they think:
//
//   * it is *their* 8th, not UTC's — these used to be parsed as UTC midnight, so
//     the window was shifted by the site's offset and silently lost the rows at
//     one end of the day while including the adjacent day's at the other;
//   * consecutive days tile the timeline exactly once — the old upper bound was
//     23:59:59 compared inclusively, which both dropped the final second and
//     double-counted anything landing on a shared boundary.

mod date_filters {
    use chrono::{TimeZone, Utc};
    use ferum_web::handlers::admin::{parse_date_from, parse_date_to, reporting_tz};

    /// `Asia/Kathmandu` is UTC+05:45 with no DST.
    ///
    /// The **:45** is why it is the fixture rather than a whole-hour zone: an
    /// offset that is not a multiple of an hour breaks any code that reasons in
    /// whole hours, or that truncates rather than converts. A test on a
    /// whole-hour zone passes for both the correct implementation and several
    /// wrong ones.
    fn offset_zone() -> chrono_tz::Tz {
        reporting_tz("Asia/Kathmandu")
    }

    #[test]
    fn a_day_starts_at_local_midnight_not_utc_midnight() {
        // 00:00 on Aug 8 at +05:45 is 18:15 on Aug 7 UTC.
        assert_eq!(
            parse_date_from(Some("2026-08-08"), offset_zone()),
            Some(Utc.with_ymd_and_hms(2026, 8, 7, 18, 15, 0).unwrap())
        );
    }

    #[test]
    fn the_upper_bound_is_the_start_of_the_next_day() {
        // Exclusive: everything strictly before 00:00 Aug 9 local.
        assert_eq!(
            parse_date_to(Some("2026-08-08"), offset_zone()),
            Some(Utc.with_ymd_and_hms(2026, 8, 8, 18, 15, 0).unwrap())
        );
    }

    /// The property that makes "no gap, no overlap" true rather than intended.
    #[test]
    fn consecutive_days_tile_the_timeline_exactly_once() {
        for tz in ["UTC", "Asia/Kathmandu", "America/New_York", "Pacific/Kiritimati"] {
            let tz = reporting_tz(tz);
            let end_of_first = parse_date_to(Some("2026-08-08"), tz);
            let start_of_second = parse_date_from(Some("2026-08-09"), tz);
            assert_eq!(
                end_of_first, start_of_second,
                "in {tz}, one day's exclusive end must be the next day's inclusive start"
            );
        }
    }

    /// Filtering "Aug 8" east of UTC must catch a post made just after local
    /// midnight, which is still the *previous* calendar day in UTC. This is the
    /// concrete failure the old code produced.
    #[test]
    fn a_post_just_after_local_midnight_falls_inside_its_own_local_day() {
        let tz = offset_zone();
        let from = parse_date_from(Some("2026-08-08"), tz).unwrap();
        let to = parse_date_to(Some("2026-08-08"), tz).unwrap();

        // 00:30 local on the 8th == 18:45 UTC on the 7th.
        let posted = Utc.with_ymd_and_hms(2026, 8, 7, 18, 45, 0).unwrap();
        assert!(
            posted >= from && posted < to,
            "a post 30 minutes into the local 8th must be inside the 8th's filter, \
             even though UTC still calls it the 7th"
        );

        // The mirror: 23:30 local on the 8th == 17:45 UTC, still the 8th.
        let late = Utc.with_ymd_and_hms(2026, 8, 8, 17, 45, 0).unwrap();
        assert!(late >= from && late < to, "23:30 local on the 8th is still the 8th");

        // 00:30 local on the 9th must NOT be.
        let next = Utc.with_ymd_and_hms(2026, 8, 8, 18, 45, 0).unwrap();
        assert!(next >= to, "00:30 local on the 9th belongs to the 9th");
    }

    /// A DST spring-forward day is 23 hours long, and its start shifts by the
    /// offset change. A fixed `+05:00` would get one of these two wrong.
    #[test]
    fn a_dst_spring_forward_day_is_twenty_three_hours() {
        let ny = reporting_tz("America/New_York");
        // 2026-03-08 is the US spring-forward date: 02:00 EST becomes 03:00 EDT.
        let start = parse_date_from(Some("2026-03-08"), ny).unwrap();
        let end = parse_date_to(Some("2026-03-08"), ny).unwrap();

        assert_eq!(start, Utc.with_ymd_and_hms(2026, 3, 8, 5, 0, 0).unwrap(), "EST is -05");
        assert_eq!(end, Utc.with_ymd_and_hms(2026, 3, 9, 4, 0, 0).unwrap(), "EDT is -04");
        assert_eq!((end - start).num_hours(), 23);
    }

    #[test]
    fn a_dst_fall_back_day_is_twenty_five_hours() {
        let ny = reporting_tz("America/New_York");
        // 2026-11-01: 02:00 EDT becomes 01:00 EST.
        let start = parse_date_from(Some("2026-11-01"), ny).unwrap();
        let end = parse_date_to(Some("2026-11-01"), ny).unwrap();
        assert_eq!((end - start).num_hours(), 25);
    }

    #[test]
    fn leap_day_is_a_real_day_and_february_ends_correctly() {
        let tz = reporting_tz("UTC");

        // 2028 is a leap year: the 28th is followed by the 29th.
        assert_eq!(
            parse_date_to(Some("2028-02-28"), tz),
            Some(Utc.with_ymd_and_hms(2028, 2, 29, 0, 0, 0).unwrap())
        );
        // ...and the 29th by March.
        assert_eq!(
            parse_date_to(Some("2028-02-29"), tz),
            Some(Utc.with_ymd_and_hms(2028, 3, 1, 0, 0, 0).unwrap())
        );
        // 2027 is not: the 28th rolls straight to March.
        assert_eq!(
            parse_date_to(Some("2027-02-28"), tz),
            Some(Utc.with_ymd_and_hms(2027, 3, 1, 0, 0, 0).unwrap())
        );
    }

    /// A handful of zones shift their clock *at* midnight, so on one night a
    /// year local 00:00 does not exist. Resolving that to `None` would silently
    /// drop the filter and quietly widen the query to everything.
    ///
    /// Written as a sweep rather than a hardcoded date because the transition
    /// date moves year to year — and because the property wanted is "every day
    /// resolves", not "this one day does".
    #[test]
    fn every_day_of_the_year_resolves_even_where_midnight_is_skipped() {
        for name in ["America/Santiago", "Asia/Beirut", "America/Havana", "UTC"] {
            let tz = reporting_tz(name);
            let mut day = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
            let end = chrono::NaiveDate::from_ymd_opt(2027, 1, 1).unwrap();
            while day < end {
                let s = day.format("%Y-%m-%d").to_string();
                assert!(
                    parse_date_from(Some(&s), tz).is_some(),
                    "{name}: {s} resolved to no instant, so the filter would be dropped"
                );
                assert!(parse_date_to(Some(&s), tz).is_some(), "{name}: {s} has no end");
                day = day.succ_opt().unwrap();
            }
        }
    }

    #[test]
    fn an_unknown_zone_name_falls_back_to_utc_rather_than_panicking() {
        // The config endpoint validates against Postgres before storing, so this
        // is the backstop for a hand-edited row — and UTC is the right fallback
        // because dropping the filter entirely would silently widen the query.
        let tz = reporting_tz("Mars/Olympus_Mons");
        assert_eq!(
            parse_date_from(Some("2026-08-08"), tz),
            Some(Utc.with_ymd_and_hms(2026, 8, 8, 0, 0, 0).unwrap())
        );
    }

    #[test]
    fn absent_and_malformed_input_yield_no_filter() {
        let tz = reporting_tz("UTC");
        assert!(parse_date_from(None, tz).is_none());
        assert!(parse_date_to(None, tz).is_none());
        assert!(parse_date_from(Some(""), tz).is_none());
        assert!(parse_date_from(Some("08/08/2026"), tz).is_none());
        assert!(parse_date_to(Some("not-a-date"), tz).is_none());
    }

    #[test]
    fn surrounding_whitespace_is_tolerated() {
        let tz = reporting_tz("UTC");
        assert_eq!(
            parse_date_from(Some("  2026-08-08  "), tz),
            Some(Utc.with_ymd_and_hms(2026, 8, 8, 0, 0, 0).unwrap())
        );
    }
}
