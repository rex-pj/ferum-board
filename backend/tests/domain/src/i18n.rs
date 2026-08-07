use ferum_domain::i18n::error_key;

#[test]
fn error_key_is_kebab_cased_and_prefixed() {
    assert_eq!(error_key("thread_locked"), "error-thread-locked");
    assert_eq!(error_key("not_author"), "error-not-author");
    // Already single-word codes still get the namespace prefix.
    assert_eq!(error_key("unauthorized"), "error-unauthorized");
}
