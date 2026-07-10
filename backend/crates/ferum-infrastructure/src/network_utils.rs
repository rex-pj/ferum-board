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
