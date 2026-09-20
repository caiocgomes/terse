//! arXiv `id_list` adapter: the effectful half of arXiv resolution, plus
//! the sequential rate-limited request worker (14.5). Normalization
//! itself is pure (`terse_core::references::arxiv`).

use std::time::Duration;

use terse_core::references::arxiv::{parse_atom_entry, split_version, ArxivResolution, AtomError};

use super::clock::Clock;
use super::transport::{MetadataTransport, TransportError, TransportRequest};

/// arXiv's documented minimum interval between API requests.
pub const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(3);
pub const MAX_RETRIES: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArxivResolveError {
    Transport(TransportError),
    UnexpectedStatus(u16),
    Parse(AtomError),
}

impl std::fmt::Display for ArxivResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArxivResolveError::Transport(e) => write!(f, "{e}"),
            ArxivResolveError::UnexpectedStatus(s) => write!(f, "arXiv responded with status {s}"),
            ArxivResolveError::Parse(e) => write!(f, "{e}"),
        }
    }
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(2u64.saturating_pow(attempt))
}

/// One sequential arXiv request worker: every call to [`resolve`] waits
/// out [`MIN_REQUEST_INTERVAL`] since the worker's last request (real time
/// via [`super::clock::RealClock`], virtual/instant in tests via
/// [`super::clock::fake::FakeClock`]) before issuing the next one, and
/// retries a bounded number of times with exponential backoff on
/// transient failures. There is exactly one worker per resolution run, so
/// requests across many aliases are still strictly sequential.
pub struct ArxivWorker {
    last_request: Option<std::time::Instant>,
}

impl ArxivWorker {
    pub fn new() -> Self {
        ArxivWorker { last_request: None }
    }

    fn throttle(&mut self, clock: &mut dyn Clock) {
        if let Some(last) = self.last_request {
            let elapsed = clock.now().saturating_duration_since(last);
            if elapsed < MIN_REQUEST_INTERVAL {
                clock.sleep(MIN_REQUEST_INTERVAL - elapsed);
            }
        }
        self.last_request = Some(clock.now());
    }

    /// Resolves one arXiv id. No PDF/HTML page scraping fallback exists at
    /// any point: a metadata failure is always a clean error.
    pub fn resolve(
        &mut self,
        transport: &mut dyn MetadataTransport,
        clock: &mut dyn Clock,
        id: &str,
    ) -> Result<ArxivResolution, ArxivResolveError> {
        let (base, _) = split_version(id);
        let url = format!("https://export.arxiv.org/api/query?id_list={base}");
        let request = TransportRequest { url, accept: None };

        let mut attempt = 0;
        loop {
            self.throttle(clock);
            match transport.fetch(&request) {
                Ok(response) if response.status == 200 => {
                    return parse_atom_entry(&response.body, id).map_err(ArxivResolveError::Parse);
                }
                Ok(response) => return Err(ArxivResolveError::UnexpectedStatus(response.status)),
                Err(_) if attempt < MAX_RETRIES => {
                    attempt += 1;
                    clock.sleep(backoff(attempt));
                }
                Err(e) => return Err(ArxivResolveError::Transport(e)),
            }
        }
    }
}

impl Default for ArxivWorker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::references::clock::fake::FakeClock;
    use crate::references::transport::fake::{ok, FakeTransport};
    use crate::references::transport::TransportResponse;

    const FEED: &str = r#"<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>http://arxiv.org/abs/2301.12345v2</id>
    <published>2023-01-30T18:00:00Z</published>
    <title>An Example Paper</title>
    <author><name>Jane Doe</name></author>
  </entry>
</feed>"#;

    #[test]
    fn versionless_id_stays_pinned_to_resolved_version() {
        let mut t = FakeTransport::new();
        t.script("https://export.arxiv.org/api/query?id_list=2301.12345", ok(FEED, "url"));
        let mut c = FakeClock::new();
        let mut worker = ArxivWorker::new();
        let res = worker.resolve(&mut t, &mut c, "2301.12345").unwrap();
        assert_eq!(res.resolved_id, "2301.12345");
        assert_eq!(res.resolved_version.as_deref(), Some("v2"));
    }

    #[test]
    fn explicit_version_is_verified() {
        let mut t = FakeTransport::new();
        t.script("https://export.arxiv.org/api/query?id_list=2301.12345", ok(FEED, "url"));
        let mut c = FakeClock::new();
        let mut worker = ArxivWorker::new();
        assert!(worker.resolve(&mut t, &mut c, "2301.12345v2").is_ok());

        let mut t2 = FakeTransport::new();
        t2.script("https://export.arxiv.org/api/query?id_list=2301.12345", ok(FEED, "url"));
        let mut c2 = FakeClock::new();
        let mut worker2 = ArxivWorker::new();
        assert!(matches!(
            worker2.resolve(&mut t2, &mut c2, "2301.12345v1").unwrap_err(),
            ArxivResolveError::Parse(AtomError::IdentityMismatch { .. })
        ));
    }

    #[test]
    fn sequential_requests_wait_the_minimum_interval() {
        let mut t = FakeTransport::new();
        for _ in 0..2 {
            t.script("https://export.arxiv.org/api/query?id_list=2301.12345", ok(FEED, "url"));
        }
        let mut c = FakeClock::new();
        let mut worker = ArxivWorker::new();
        worker.resolve(&mut t, &mut c, "2301.12345").unwrap();
        worker.resolve(&mut t, &mut c, "2301.12345").unwrap();
        // No real time passed (FakeClock's `now` never advances on its
        // own), so the second request must have triggered a sleep of the
        // full minimum interval.
        assert!(c.slept.contains(&MIN_REQUEST_INTERVAL));
        assert_eq!(t.request_count("https://export.arxiv.org/api/query?id_list=2301.12345"), 2);
    }

    #[test]
    fn transient_failure_retries_with_backoff_then_gives_up() {
        let mut t = FakeTransport::new();
        for _ in 0..=MAX_RETRIES {
            t.script(
                "https://export.arxiv.org/api/query?id_list=2301.12345",
                Err(TransportError::Transient("connection reset".to_string())),
            );
        }
        let mut c = FakeClock::new();
        let mut worker = ArxivWorker::new();
        let err = worker.resolve(&mut t, &mut c, "2301.12345").unwrap_err();
        assert_eq!(err, ArxivResolveError::Transport(TransportError::Transient("connection reset".to_string())));
        assert_eq!(
            t.request_count("https://export.arxiv.org/api/query?id_list=2301.12345") as u32,
            MAX_RETRIES + 1
        );
    }

    #[test]
    fn unexpected_status_never_falls_back_to_scraping() {
        let mut t = FakeTransport::new();
        t.script(
            "https://export.arxiv.org/api/query?id_list=2301.12345",
            Ok(TransportResponse { status: 500, body: Vec::new(), final_url: "url".to_string() }),
        );
        let mut c = FakeClock::new();
        let mut worker = ArxivWorker::new();
        assert_eq!(worker.resolve(&mut t, &mut c, "2301.12345").unwrap_err(), ArxivResolveError::UnexpectedStatus(500));
        // Exactly one request: a non-200 status is not retried (only
        // transport-level transient errors are), and no second request
        // (e.g. to an HTML abstract page) is ever made.
        assert_eq!(t.request_count("https://export.arxiv.org/api/query?id_list=2301.12345"), 1);
    }
}
