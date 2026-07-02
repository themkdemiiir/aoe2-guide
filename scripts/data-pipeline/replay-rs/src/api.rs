//! World's Edge Link API client: getReplayFiles + blob download.
//!
//! Direct port of `scripts/data-pipeline/replays/api.py`. getReplayFiles returns
//! SIGNED Azure blob URLs (one per player) for a batch of match ids; the bare
//! matchurls from getRecentMatchHistory are unsigned and 403. URLs expire
//! (expiryUnix), so download promptly.

use anyhow::{anyhow, Result};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::io::Read;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use replay_rs::config;

/// Monotonic-spacing rate limiter shared across threads (matches RateLimiter in
/// api.py). Each `wait()` blocks until at least `interval` has elapsed since the
/// previous granted slot, so the *global* request rate stays under the cap.
pub struct RateLimiter {
    interval: Duration,
    next: Mutex<Option<Instant>>,
}

impl RateLimiter {
    pub fn new(per_min: u32) -> Self {
        let per_min = per_min.max(1);
        RateLimiter {
            interval: Duration::from_secs_f64(60.0 / per_min as f64),
            next: Mutex::new(None),
        }
    }

    pub fn wait(&self) {
        // Hold the lock across the sleep so slots are handed out strictly in
        // order — same single-threaded spacing guarantee as the Python version.
        let mut guard = self.next.lock().unwrap();
        let now = Instant::now();
        let next = guard.unwrap_or(now);
        if now < next {
            std::thread::sleep(next - now);
        }
        let granted = Instant::now();
        *guard = Some(granted + self.interval);
    }
}

#[derive(Debug, Deserialize)]
struct ReplayFilesResponse {
    #[serde(default)]
    result: Option<ApiResult>,
    #[serde(default, rename = "replayFiles")]
    replay_files: Vec<ReplayFile>,
}

#[derive(Debug, Deserialize)]
struct ApiResult {
    #[serde(default)]
    message: Option<String>,
}

/// One uploaded replay file for a match (one per player who stayed in).
#[derive(Debug, Clone, Deserialize)]
pub struct ReplayFile {
    #[serde(default)]
    pub profile_id: Option<i64>,
    #[serde(default)]
    pub matchhistory_id: Option<i64>,
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(default)]
    pub url: Option<String>,
}

/// Shared blocking HTTP client (rustls + gzip for JSON). Reuse keeps connections
/// pooled across the many calls a run makes.
pub fn build_client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(config::UA)
        .timeout(Duration::from_secs(90))
        .build()?)
}

/// GET with a few retries on transient network errors (mirrors api._get).
fn get_bytes(client: &reqwest::blocking::Client, url: &str, tries: u32) -> Result<Vec<u8>> {
    let mut last: Option<anyhow::Error> = None;
    for attempt in 1..=tries {
        match client.get(url).send().and_then(|r| r.error_for_status()) {
            Ok(resp) => match resp.bytes() {
                Ok(b) => return Ok(b.to_vec()),
                Err(e) => last = Some(anyhow!(e)),
            },
            Err(e) => last = Some(anyhow!(e)),
        }
        // Quadratic backoff: 0.5 * attempt^2 seconds, like the Python client.
        std::thread::sleep(Duration::from_secs_f64(0.5 * (attempt * attempt) as f64));
    }
    Err(last.unwrap_or_else(|| anyhow!("request failed")))
}

/// Look up signed URLs for a batch of match ids.
///
/// Returns `per_match`: match_id -> files for matches the API knew about. Match
/// ids absent from the result have aged out (the caller marks them `expired`).
pub fn get_replay_files(
    client: &reqwest::blocking::Client,
    match_ids: &[i64],
) -> Result<std::collections::HashMap<i64, Vec<ReplayFile>>> {
    // URL-encode the matchIDs list exactly as api.py does: %5B id %2C id %5D.
    let inner = match_ids
        .iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join("%2C");
    let url = format!(
        "{}/getReplayFiles?matchIDs=%5B{}%5D&title={}",
        config::API_BASE,
        inner,
        config::TITLE
    );
    let raw = get_bytes(client, &url, 4)?;
    let doc: ReplayFilesResponse = serde_json::from_slice(&raw)?;
    let _ = doc.result.and_then(|r| r.message); // message is informational only

    let mut per_match: std::collections::HashMap<i64, Vec<ReplayFile>> = Default::default();
    for f in doc.replay_files {
        if let Some(mid) = f.matchhistory_id {
            per_match.entry(mid).or_default().push(f);
        }
    }
    Ok(per_match)
}

/// Largest non-(-1) replay (the player who stayed longest = most complete).
pub fn best_file(files: &[ReplayFile]) -> Option<&ReplayFile> {
    files
        .iter()
        .filter(|f| f.size.unwrap_or(-1) > 0 && f.url.is_some())
        .max_by_key(|f| f.size.unwrap_or(-1))
}

/// Download a signed blob and gunzip it to raw `.aoe2record` bytes.
///
/// We decompress the gzip member ourselves (rather than relying on reqwest's
/// transparent gzip) so aoe2rec always receives the raw record bytes regardless
/// of how the CDN sets its content headers.
pub fn download_replay(client: &reqwest::blocking::Client, url: &str) -> Result<bytes::Bytes> {
    let raw = get_bytes(client, url, 4)?;
    let mut out = Vec::new();
    GzDecoder::new(raw.as_slice()).read_to_end(&mut out)?;
    Ok(bytes::Bytes::from(out))
}
