use ferum_domain::Locale;

#[test]
fn accepts_plain_language_tags() {
    assert_eq!(Locale::parse("en").unwrap().as_str(), "en");
    assert_eq!(Locale::parse("vi").unwrap().as_str(), "vi");
    assert_eq!(Locale::parse("fil").unwrap().as_str(), "fil");
}

#[test]
fn canonicalizes_casing_so_one_language_maps_to_one_locale() {
    // A cookie saying "EN-us" and a URL saying "en-US" must not produce two
    // distinct Locales, or they would split the per-locale Tera cache.
    assert_eq!(Locale::parse("EN-us").unwrap(), Locale::parse("en-US").unwrap());
    assert_eq!(Locale::parse("en-us").unwrap().as_str(), "en-US");
    assert_eq!(Locale::parse("ZH-hant-tw").unwrap().as_str(), "zh-Hant-TW");
}

#[test]
fn rejects_path_traversal_attempts() {
    // The whole point of the type: these must never reach a file path.
    for hostile in [
        "..",
        "../..",
        "en/../..",
        "en/../../etc/passwd",
        "en\\..\\..",
        "./en",
        "en.ftl",
        "e n",
        "en\0",
        "en\n",
    ] {
        assert!(Locale::parse(hostile).is_none(), "should reject {hostile:?}");
    }
}

#[test]
fn rejects_malformed_shapes() {
    for bad in [
        "",
        "e",                // too short
        "engl",             // 4-char language
        "en-",              // empty subtag
        "-en",              // empty language
        "en-USA",           // 3-alpha region
        "en-US-extra-more", // too many subtags
        "en-US-GB",         // region cannot follow region
        "日本語",           // non-ASCII
        "enenenenenenen",   // over length cap
    ] {
        assert!(Locale::parse(bad).is_none(), "should reject {bad:?}");
    }
}

#[test]
fn fallback_chain_narrows_to_default() {
    let chain: Vec<String> = Locale::parse("zh-Hant-TW")
        .unwrap()
        .fallback_chain()
        .iter()
        .map(|l| l.to_string())
        .collect();
    assert_eq!(chain, vec!["zh-Hant-TW", "zh-Hant", "zh", "en"]);
}

#[test]
fn fallback_chain_never_duplicates_default() {
    let chain = Locale::default_locale().fallback_chain();
    assert_eq!(chain.len(), 1, "en should not appear twice: {chain:?}");
}

#[test]
fn deserialize_rejects_invalid_tag() {
    assert!(serde_json::from_str::<Locale>(r#""../etc""#).is_err());
    assert_eq!(
        serde_json::from_str::<Locale>(r#""vi""#).unwrap(),
        Locale::parse("vi").unwrap()
    );
}
