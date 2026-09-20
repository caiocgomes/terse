//! Bounded archive downloads for `terse toolchain install`: streamed to a
//! file, size-bounded, HTTPS only, every redirect hop validated, and the
//! SHA-512 computed while streaming. Only the install/update path ever
//! constructs the real implementation.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sha2::{Digest, Sha512};

use crate::references::transport::validate_hop;

const MAX_REDIRECTS: usize = 5;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);
const USER_AGENT: &str = concat!("terse/", env!("CARGO_PKG_VERSION"), " (toolchain install)");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub bytes: u64,
    pub sha512: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadError {
    Denied(String),
    Transport(String),
    Http(u16),
    TooLarge(u64),
    Io(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::Denied(url) => write!(f, "network access denied for '{url}'"),
            DownloadError::Transport(msg) => write!(f, "{msg}"),
            DownloadError::Http(status) => write!(f, "HTTP status {status}"),
            DownloadError::TooLarge(max) => write!(f, "response exceeded the {max}-byte bound"),
            DownloadError::Io(msg) => write!(f, "{msg}"),
        }
    }
}

pub trait ArchiveDownloader {
    fn fetch_to_file(&mut self, url: &str, dest: &Path, max_bytes: u64) -> Result<Download, DownloadError>;
}

/// Scripted by URL; records every request. With nothing scripted (or
/// built with [`FakeArchiveDownloader::denied_with_log`]) every request
/// is refused and still recorded, which is how tests prove a command
/// never asks for the network.
pub struct FakeArchiveDownloader {
    responses: HashMap<String, Vec<u8>>,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Default for FakeArchiveDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeArchiveDownloader {
    pub fn new() -> Self {
        Self { responses: HashMap::new(), requests: Arc::new(Mutex::new(Vec::new())) }
    }

    pub fn denied_with_log(log: Arc<Mutex<Vec<String>>>) -> Self {
        Self { responses: HashMap::new(), requests: log }
    }

    pub fn serve(&mut self, url: &str, bytes: Vec<u8>) {
        self.responses.insert(url.to_string(), bytes);
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl ArchiveDownloader for FakeArchiveDownloader {
    fn fetch_to_file(&mut self, url: &str, dest: &Path, max_bytes: u64) -> Result<Download, DownloadError> {
        self.requests.lock().unwrap().push(url.to_string());
        let Some(bytes) = self.responses.get(url) else {
            return Err(DownloadError::Denied(url.to_string()));
        };
        if bytes.len() as u64 > max_bytes {
            return Err(DownloadError::TooLarge(max_bytes));
        }
        std::fs::write(dest, bytes).map_err(|e| DownloadError::Io(e.to_string()))?;
        Ok(Download { bytes: bytes.len() as u64, sha512: sha512_hex(bytes) })
    }
}

pub fn sha512_hex(bytes: &[u8]) -> String {
    let mut h = Sha512::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Real HTTPS downloads through reqwest with redirects disabled so every
/// hop is validated by `references::transport::validate_hop` before it is
/// followed; the body is streamed to `dest` and hashed as it arrives, and
/// the transfer stops as soon as it exceeds `max_bytes`.
pub struct RealArchiveDownloader {
    client: reqwest::blocking::Client,
}

impl RealArchiveDownloader {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .expect("reqwest client builds with static configuration");
        RealArchiveDownloader { client }
    }
}

impl Default for RealArchiveDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl ArchiveDownloader for RealArchiveDownloader {
    fn fetch_to_file(&mut self, url: &str, dest: &Path, max_bytes: u64) -> Result<Download, DownloadError> {
        let mut url = url.to_string();
        for _ in 0..=MAX_REDIRECTS {
            validate_hop(&url).map_err(|e| DownloadError::Transport(e.to_string()))?;
            let response = self.client.get(&url).send().map_err(|e| DownloadError::Transport(e.to_string()))?;
            let status = response.status();
            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| DownloadError::Transport("redirect without a Location header".to_string()))?;
                url = resolve_location(&url, location);
                continue;
            }
            if !status.is_success() {
                return Err(DownloadError::Http(status.as_u16()));
            }
            return stream_to_file(response, dest, max_bytes);
        }
        Err(DownloadError::Transport(format!("more than {MAX_REDIRECTS} redirects")))
    }
}

fn resolve_location(base: &str, location: &str) -> String {
    if location.starts_with("https://") || location.starts_with("http://") {
        return location.to_string();
    }
    if let Some(rest) = location.strip_prefix('/') {
        let origin_end = base.find("://").map(|i| i + 3).unwrap_or(0);
        let origin = &base[..base[origin_end..].find('/').map(|i| origin_end + i).unwrap_or(base.len())];
        return format!("{origin}/{rest}");
    }
    let dir = &base[..base.rfind('/').map(|i| i + 1).unwrap_or(base.len())];
    format!("{dir}{location}")
}

fn stream_to_file(mut response: reqwest::blocking::Response, dest: &Path, max_bytes: u64) -> Result<Download, DownloadError> {
    let mut file = std::fs::File::create(dest).map_err(|e| DownloadError::Io(e.to_string()))?;
    let mut hasher = Sha512::new();
    let mut total: u64 = 0;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = response.read(&mut buf).map_err(|e| DownloadError::Transport(e.to_string()))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > max_bytes {
            return Err(DownloadError::TooLarge(max_bytes));
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| DownloadError::Io(e.to_string()))?;
    }
    file.flush().map_err(|e| DownloadError::Io(e.to_string()))?;
    Ok(Download { bytes: total, sha512: hasher.finalize().iter().map(|b| format!("{b:02x}")).collect() })
}
