use ferum_application::usecases::webhook_usecase::validate_webhook_url;

#[test]
fn rejects_ipv4_private_and_loopback_literals() {
    for u in [
        "http://127.0.0.1/hook",
        "http://10.0.0.5/hook",
        "http://192.168.1.1/hook",
        "http://169.254.169.254/latest/meta-data",
        "http://localhost:5173/hook",
    ] {
        assert!(validate_webhook_url(u).is_err(), "{u} must be rejected");
    }
}

/// `http::Uri::host()` keeps the brackets on IPv6 literals, so a check that
/// compares the raw host against "::1" never fires. Regression guard.
#[test]
fn rejects_bracketed_ipv6_private_and_loopback_literals() {
    for u in [
        "http://[::1]/hook",
        "http://[::1]:9000/hook",
        "http://[fd00::1]/hook",
        "http://[fe80::1]/hook",
        "http://[::ffff:127.0.0.1]/hook",
    ] {
        assert!(validate_webhook_url(u).is_err(), "{u} must be rejected");
    }
}

#[test]
fn rejects_non_http_schemes_and_empty() {
    assert!(validate_webhook_url("").is_err());
    assert!(validate_webhook_url("ftp://example.com/x").is_err());
    assert!(validate_webhook_url("file:///etc/passwd").is_err());
}

#[test]
fn accepts_public_hosts() {
    for u in [
        "https://hooks.slack.com/services/abc",
        "http://example.com:5173/hook",
        "https://8.8.8.8/hook",
        "https://[2606:4700::1111]/hook",
    ] {
        assert!(validate_webhook_url(u).is_ok(), "{u} must be accepted");
    }
}
