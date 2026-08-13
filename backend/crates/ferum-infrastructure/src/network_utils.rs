/// Resolves + validates the target host's IP at call time (an attacker can flip
/// the DNS record to a private IP between when a URL is stored and when it is
/// fetched), then pins the actual request to the exact address(es) just
/// validated. Letting reqwest re-resolve the hostname itself would reopen the
/// DNS-rebinding window this check exists to close.
///
/// This is the ONLY supported way to make an outbound request to a
/// user/admin/plugin-supplied URL. Do not validate a URL and then build a
/// separate client — that is the exact TOCTOU bug this closes.
pub(crate) async fn build_pinned_client(url: &str) -> Result<reqwest::Client, String> {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .ok_or_else(|| "unparseable URL".to_string())?;
    let addrs = resolve_and_validate(url).await?;
    reqwest::Client::builder()
        .resolve_to_addrs(&host, &addrs)
        // Redirects are refused outright. `resolve_to_addrs` pins ONLY `host`;
        // a redirect hop to any other host is resolved by reqwest normally,
        // with no validation at all — which would hand back the exact
        // DNS-rebinding/private-range bypass this module exists to close. A
        // 302 to http://169.254.169.254/ is the canonical attack. Following
        // redirects safely would mean re-running `resolve_and_validate` per
        // hop; until a caller genuinely needs that, refusing is the honest
        // default. The redirect response itself is still returned to the
        // caller as a normal 3xx, so a caller that wants to inspect it can.
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))
}

/// Resolves the hostname in `url`, validates every candidate address is public,
/// and returns the validated addresses so the caller can pin the connection to
/// them — re-resolving DNS for the actual request would reopen the TOCTOU window
/// this check exists to close (DNS re-binding between validation and connect).
///
/// Deliberately not public: callers outside this module must go through
/// [`build_pinned_client`], which cannot be misused to validate without pinning.
pub(crate) async fn resolve_and_validate(url: &str) -> Result<Vec<std::net::SocketAddr>, String> {
    let parsed = url::Url::parse(url)
        .map_err(|_| format!("unparseable URL: {}", url))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?;
    let port = parsed
        .port()
        .unwrap_or(if parsed.scheme() == "https" { 443 } else { 80 });

    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host(format!("{}:{}", host, port))
        .await
        .map_err(|e| format!("DNS resolution failed for {}: {}", host, e))?
        .collect();

    for socket_addr in &addrs {
        let ip = socket_addr.ip();
        if is_private_ip(ip) {
            return Err(format!(
                "resolved IP {} is in a private/reserved range",
                ip
            ));
        }
    }
    Ok(addrs)
}

/// Re-exported from the domain so the application layer (webhook URL validation)
/// and this layer (connect-time validation) can never disagree on what counts as
/// a private address. Kept public: exercised directly by the infra test suite.
pub use ferum_domain::net::is_private_ip;

/// Shortens a third-party error body for a log line or an error message.
///
/// Every adapter that talks to an external HTTP API needs this: the body is the
/// only thing that says *why* the call was rejected, but Google's are XML and
/// Resend's are JSON, and neither belongs in a log line in full. The status plus
/// the opening of the body is what identifies the fault.
///
/// Lives here rather than beside its first caller because that caller —
/// `storage/gcs.rs` — is behind `--features gcs`, while the mail adapters are
/// compiled unconditionally. A `pub(crate)` helper in a feature-gated module
/// cannot be shared with one that is always present.
///
/// Cuts on a `char` boundary, so a multi-byte body cannot panic the slice.
pub(crate) fn truncate_for_log(body: &str) -> String {
    const MAX: usize = 512;
    if body.len() <= MAX {
        return body.to_string();
    }
    let cut = body
        .char_indices()
        .take_while(|(i, _)| *i <= MAX)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    format!("{}…", &body[..cut])
}

/// Concrete [`HostResolver`] backed by the tokio resolver. Used only for the
/// advisory admin-facing webhook check; connect-time safety lives in
/// [`build_pinned_client`].
pub struct TokioHostResolver;

#[async_trait::async_trait]
impl ferum_application::ports::HostResolver for TokioHostResolver {
    async fn resolve(
        &self,
        host: &str,
    ) -> Result<Vec<std::net::IpAddr>, ferum_application::shared::AppError> {
        // Port is irrelevant to the caller; lookup_host just requires one.
        let addrs = tokio::net::lookup_host(format!("{host}:80"))
            .await
            .map_err(|e| {
                ferum_application::shared::AppError::unprocessable(&format!(
                    "DNS resolution failed for {host}: {e}"
                ))
            })?;
        Ok(addrs.map(|sa| sa.ip()).collect())
    }
}
