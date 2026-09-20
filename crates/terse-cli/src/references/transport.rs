//! Injectable metadata-fetch transport: the only network boundary
//! reference resolution (group 15's `refs resolve`) is allowed to cross.
//! Ordinary `check`/`build`/`fmt`/`watch` never construct or call this.

use std::net::{IpAddr, ToSocketAddrs};
use std::time::Duration;

pub const MAX_RESPONSE_BYTES: usize = 5 * 1024 * 1024;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
pub const MAX_REDIRECTS: usize = 5;
pub const USER_AGENT: &str = concat!("terse/", env!("CARGO_PKG_VERSION"), " (+https://github.com/terse-lang/terse)");

#[derive(Debug, Clone)]
pub struct TransportRequest {
    pub url: String,
    pub accept: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportResponse {
    pub status: u16,
    pub body: Vec<u8>,
    /// The URL the response was actually served from, after any redirects.
    pub final_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// Connection refused/timed out/DNS failure, or any other transport
    /// failure that is worth retrying.
    Transient(String),
    /// A URL is not `https`.
    NotHttps(String),
    /// A redirect targeted a local/private/link-local address (SSRF
    /// protection) or exceeded the redirect bound.
    UnsafeRedirect(String),
    /// The response body exceeded [`MAX_RESPONSE_BYTES`].
    ResponseTooLarge,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Transient(msg) => write!(f, "transient transport error: {msg}"),
            TransportError::NotHttps(url) => write!(f, "'{url}' is not an https URL"),
            TransportError::UnsafeRedirect(msg) => write!(f, "unsafe redirect: {msg}"),
            TransportError::ResponseTooLarge => write!(f, "response exceeded the size bound"),
        }
    }
}

/// The metadata-fetch boundary. Real resolution uses [`RealTransport`];
/// tests inject a fake to assert exact request counts/URLs and control
/// failure/redirect scenarios without any real sockets or DNS.
pub trait MetadataTransport {
    fn fetch(&mut self, request: &TransportRequest) -> Result<TransportResponse, TransportError>;
}

/// Rejects loopback, link-local, and other IETF-reserved private address
/// ranges as a redirect (or initial) target, so a compromised or
/// misconfigured provider cannot be used to reach internal services.
pub fn is_disallowed_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_disallowed_ip(ip);
    }
    // A hostname resolving only to disallowed addresses is also rejected;
    // resolution failure is treated as "not provably safe" and disallowed.
    match (host, 0u16).to_socket_addrs() {
        Ok(addrs) => {
            let addrs: Vec<_> = addrs.collect();
            !addrs.is_empty() && addrs.iter().all(|a| is_disallowed_ip(a.ip()))
        }
        Err(_) => false,
    }
}

fn is_disallowed_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified() || (v6.segments()[0] & 0xfe00) == 0xfc00,
    }
}

fn host_of(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    let end = rest.find(['/', ':']).unwrap_or(rest.len());
    Some(&rest[..end])
}

/// Validates a URL is `https` and does not target a disallowed host,
/// checked at every hop (initial request and each redirect) by callers.
pub fn validate_hop(url: &str) -> Result<(), TransportError> {
    if !url.starts_with("https://") {
        return Err(TransportError::NotHttps(url.to_string()));
    }
    let host = host_of(url).ok_or_else(|| TransportError::UnsafeRedirect(format!("could not parse host from '{url}'")))?;
    if is_disallowed_host(host) {
        return Err(TransportError::UnsafeRedirect(format!("'{host}' is a local/private address")));
    }
    Ok(())
}

/// Real HTTPS transport: bounded redirects (each hop revalidated), bounded
/// response size, a fixed timeout, and a fixed identifying user agent.
/// Reqwest's own redirect following is disabled so every hop can be
/// inspected and rejected before it is followed.
pub struct RealTransport {
    client: reqwest::blocking::Client,
}

impl RealTransport {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .expect("reqwest client builds with static configuration");
        RealTransport { client }
    }
}

impl Default for RealTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataTransport for RealTransport {
    fn fetch(&mut self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
        let mut url = request.url.clone();
        for _ in 0..=MAX_REDIRECTS {
            validate_hop(&url)?;
            let mut builder = self.client.get(&url);
            if let Some(accept) = &request.accept {
                builder = builder.header(reqwest::header::ACCEPT, accept.as_str());
            }
            let response = builder.send().map_err(|e| TransportError::Transient(e.to_string()))?;
            let status = response.status();
            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| TransportError::UnsafeRedirect("redirect with no Location header".to_string()))?
                    .to_string();
                url = location;
                continue;
            }
            let bytes = response.bytes().map_err(|e| TransportError::Transient(e.to_string()))?;
            if bytes.len() > MAX_RESPONSE_BYTES {
                return Err(TransportError::ResponseTooLarge);
            }
            return Ok(TransportResponse { status: status.as_u16(), body: bytes.to_vec(), final_url: url });
        }
        Err(TransportError::UnsafeRedirect(format!("exceeded {MAX_REDIRECTS} redirects")))
    }
}

pub mod fake {
    use super::*;
    use std::collections::VecDeque;

    /// A scripted response queue keyed by request URL, recording exactly
    /// how many times each URL was requested — the exact-request-count
    /// assertions named in tests.md are built on this.
    #[derive(Default)]
    pub struct FakeTransport {
        pub scripted: std::collections::HashMap<String, VecDeque<Result<TransportResponse, TransportError>>>,
        pub requests: Vec<String>,
    }

    impl FakeTransport {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn script(&mut self, url: &str, outcome: Result<TransportResponse, TransportError>) {
            self.scripted.entry(url.to_string()).or_default().push_back(outcome);
        }

        pub fn request_count(&self, url: &str) -> usize {
            self.requests.iter().filter(|u| u.as_str() == url).count()
        }
    }

    impl MetadataTransport for FakeTransport {
        fn fetch(&mut self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
            self.requests.push(request.url.clone());
            self.scripted
                .get_mut(&request.url)
                .and_then(|q| q.pop_front())
                .unwrap_or_else(|| Err(TransportError::Transient(format!("no scripted response for '{}'", request.url))))
        }
    }

    pub fn ok(body: impl Into<Vec<u8>>, final_url: &str) -> Result<TransportResponse, TransportError> {
        Ok(TransportResponse { status: 200, body: body.into(), final_url: final_url.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_https() {
        assert_eq!(validate_hop("http://doi.org/10.1/x"), Err(TransportError::NotHttps("http://doi.org/10.1/x".to_string())));
    }

    #[test]
    fn rejects_loopback_and_private_hosts() {
        assert!(is_disallowed_host("localhost"));
        assert!(is_disallowed_host("127.0.0.1"));
        assert!(is_disallowed_host("10.0.0.5"));
        assert!(is_disallowed_host("192.168.1.1"));
        assert!(is_disallowed_host("169.254.1.1"));
        assert!(!is_disallowed_host("doi.org"));
    }

    #[test]
    fn fake_transport_records_exact_request_counts() {
        let mut t = fake::FakeTransport::new();
        t.script("https://doi.org/x", fake::ok("body", "https://doi.org/x"));
        let req = TransportRequest { url: "https://doi.org/x".to_string(), accept: None };
        t.fetch(&req).unwrap();
        assert_eq!(t.request_count("https://doi.org/x"), 1);
    }
}
