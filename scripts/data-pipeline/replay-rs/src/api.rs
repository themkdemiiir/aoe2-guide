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

// --- age archive fallback (api.ageofempires.com) ------------------------------
// For matches that aged out of getReplayFiles. Serves a ZIP with one
// AgeIIDE_Replay_<id>.aoe2record member; needs a participant profileId; rate
// limit is harsh (see config). aoe.ms/replay is a bare 301 onto this endpoint.

/// Outcome of one archive lookup across a match's participants.
pub enum ArchiveFetch {
    /// Raw .aoe2record bytes + the profile id that worked.
    Ok(bytes::Bytes, i64),
    /// Every tried participant returned 404 — the archive has no copy.
    NotFound,
    /// 429 wall persisted through retries — retry the match on a later run.
    RateLimited,
    /// 200 arrived but the ZIP was unusable — a deterministic failure, so this
    /// is terminal (retrying only re-burns the archive budget). Carries why.
    BadPayload(String),
}

/// Try each participant until one yields the replay. Paced by `limiter`
/// (ARCHIVE_SPACING_SECS between requests, shared across the run); honors
/// Retry-After on 429. Network/protocol failures bubble up as Err (retryable).
pub fn download_archive_replay(
    client: &reqwest::blocking::Client,
    limiter: &RateLimiter,
    match_id: i64,
    profile_ids: &[i64],
    requests_made: &mut usize,
) -> Result<ArchiveFetch> {
    for &pid in profile_ids.iter().take(config::ARCHIVE_MAX_PIDS_PER_MATCH) {
        let url = format!(
            "{}?gameId={match_id}&profileId={pid}&matchId={match_id}",
            config::ARCHIVE_BASE
        );
        let mut tries_429 = 0u32;
        loop {
            limiter.wait();
            *requests_made += 1;
            let resp = client.get(&url).send()?;
            match resp.status().as_u16() {
                200 => {
                    let body = resp.bytes()?; // network failure here stays retryable
                    return match unzip_single_member(&body) {
                        Ok(record) => Ok(ArchiveFetch::Ok(record, pid)),
                        // A structurally bad ZIP won't get better on retry.
                        Err(e) => Ok(ArchiveFetch::BadPayload(format!(
                            "archive zip for match {match_id} profile {pid}: {e}"
                        ))),
                    };
                }
                404 => break, // this participant has no copy — try the next one
                429 => {
                    if tries_429 >= config::ARCHIVE_MAX_429_RETRIES {
                        return Ok(ArchiveFetch::RateLimited);
                    }
                    tries_429 += 1;
                    let wait = resp
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(config::ARCHIVE_RETRY_DEFAULT_SECS)
                        .min(config::ARCHIVE_RETRY_MAX_SECS);
                    std::thread::sleep(Duration::from_secs(wait));
                }
                other => {
                    return Err(anyhow!(
                        "archive: match {match_id} profile {pid}: HTTP {other}"
                    ))
                }
            }
        }
    }
    Ok(ArchiveFetch::NotFound)
}

/// Minimal single-member ZIP extraction (the archive always ships exactly one
/// .aoe2record, produced by .NET). Handles stored + deflate members; anything
/// else fails loud rather than guessing. Hand-rolled to keep the offline build
/// dependency-free — flate2 (already a dep) does the inflation.
fn unzip_single_member(zip: &[u8]) -> Result<bytes::Bytes> {
    use flate2::read::DeflateDecoder;
    if zip.len() < 30 || &zip[0..4] != b"PK\x03\x04" {
        return Err(anyhow!("not a zip local-file header ({} bytes)", zip.len()));
    }
    let u16le = |o: usize| u16::from_le_bytes([zip[o], zip[o + 1]]) as usize;
    let u32le = |o: usize| u32::from_le_bytes([zip[o], zip[o + 1], zip[o + 2], zip[o + 3]]) as usize;
    let method = u16le(8);
    let compressed_size = u32le(18);
    let data_off = 30 + u16le(26) + u16le(28);
    if data_off > zip.len() {
        return Err(anyhow!("zip header runs past the buffer"));
    }
    match method {
        // Deflate: inflate from the data offset; the decoder stops at the
        // stream's own end marker, so a streaming zip's zeroed size field
        // (general-purpose bit 3) is harmless.
        8 => {
            let mut out = Vec::new();
            DeflateDecoder::new(&zip[data_off..]).read_to_end(&mut out)?;
            Ok(bytes::Bytes::from(out))
        }
        // Stored: the size field must be real (streaming+stored is ambiguous).
        0 => {
            if compressed_size == 0 || data_off + compressed_size > zip.len() {
                return Err(anyhow!("stored zip member with unusable size field"));
            }
            Ok(bytes::Bytes::copy_from_slice(
                &zip[data_off..data_off + compressed_size],
            ))
        }
        m => Err(anyhow!("unsupported zip compression method {m}")),
    }
}

// --- getRecentMatchHistory (recent ranked games for one profile) --------------
// source: same endpoint + normalization rules as scripts/data-pipeline/stream-relic.mjs
// (in production via the 3h cron): AUTOMATCH description = ranked matchmaking;
// outcome 1 = win; newrating falls back to oldrating.

#[derive(Debug, Deserialize)]
struct RecentHistoryResponse {
    #[serde(default, rename = "matchHistoryStats")]
    match_history_stats: Vec<MatchStat>,
    #[serde(default)]
    profiles: Vec<ProfileEntry>,
}

#[derive(Debug, Deserialize)]
struct MatchStat {
    id: i64,
    #[serde(default)]
    completiontime: i64,
    #[serde(default)]
    description: String,
    #[serde(default)]
    matchhistorymember: Vec<MatchMember>,
}

#[derive(Debug, Deserialize)]
struct MatchMember {
    profile_id: i64,
    #[serde(default)]
    civilization_id: Option<u32>,
    #[serde(default)]
    oldrating: Option<i32>,
    #[serde(default)]
    newrating: Option<i32>,
    #[serde(default)]
    outcome: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct ProfileEntry {
    profile_id: i64,
    #[serde(default)]
    alias: Option<String>,
}

/// One recent ranked game from the profile's point of view.
#[derive(Debug, Clone)]
pub struct RecentMatch {
    pub match_id: i64,
    pub completed_unix: i64,
    // NOTE: the API's per-match `mapname` is deliberately NOT surfaced — it is
    // wrong for most matches (43% agreement vs replays); `analyze` reads the
    // real map from the replay itself.
    pub team_size: usize,
    pub my_civ_id: Option<u32>,
    pub my_rating: Option<i32>,
    pub my_won: Option<bool>,
    pub my_alias: Option<String>,
}

/// AUTOMATCH + completed only, newest first, "me" fields joined by profile_id.
fn normalize_recent(doc: RecentHistoryResponse, profile_id: i64) -> Vec<RecentMatch> {
    let alias = doc
        .profiles
        .iter()
        .find(|p| p.profile_id == profile_id)
        .and_then(|p| p.alias.clone());
    let mut out: Vec<RecentMatch> = doc
        .match_history_stats
        .into_iter()
        // AUTOMATCH marks ranked matchmaking (source: stream-relic.mjs; custom
        // lobbies observed as "." — see FIXTURE comment below).
        // completiontime > 0 is DEFENSIVE, not from stream-relic.mjs: excludes
        // in-progress/unreported matches (none observed in the 129-entry probe);
        // analyzing an unfinished match is meaningless.
        .filter(|m| m.description == "AUTOMATCH" && m.completiontime > 0)
        .map(|m| {
            let me = m
                .matchhistorymember
                .iter()
                .find(|x| x.profile_id == profile_id);
            RecentMatch {
                match_id: m.id,
                completed_unix: m.completiontime,
                team_size: m.matchhistorymember.len(),
                my_civ_id: me.and_then(|x| x.civilization_id),
                // source: newrating is the post-game rating; oldrating is the
                // pre-game fallback when newrating is absent (e.g. unrated game).
                my_rating: me.and_then(|x| x.newrating.or(x.oldrating)),
                // source: outcome 1 = win (stream-relic.mjs normalization rule).
                my_won: me.and_then(|x| x.outcome.map(|o| o == 1)),
                my_alias: alias.clone(),
            }
        })
        .collect();
    out.sort_by_key(|m| std::cmp::Reverse(m.completed_unix));
    out
}

/// Recent ranked games for one profile. The probe returned 129 history entries
/// of all types for one profile; the AUTOMATCH subset varies by profile.
pub fn get_recent_matches(
    client: &reqwest::blocking::Client,
    profile_id: i64,
) -> Result<Vec<RecentMatch>> {
    let url = format!(
        "{}/getRecentMatchHistory?title={}&profile_ids=%5B{}%5D",
        config::API_BASE,
        config::TITLE,
        profile_id
    );
    let raw = get_bytes(client, &url, 4)?;
    let doc: RecentHistoryResponse = serde_json::from_slice(&raw)?;
    Ok(normalize_recent(doc, profile_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a one-member zip in memory: local header + (deflated|stored) data.
    /// Mirrors what the archive endpoint ships (single .aoe2record member).
    fn make_zip(payload: &[u8], deflate: bool, zero_size_fields: bool) -> Vec<u8> {
        use flate2::{write::DeflateEncoder, Compression};
        use std::io::Write as _;
        let name = b"AgeIIDE_Replay_1.aoe2record";
        let data: Vec<u8> = if deflate {
            let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
            enc.write_all(payload).unwrap();
            enc.finish().unwrap()
        } else {
            payload.to_vec()
        };
        let mut z = Vec::new();
        z.extend_from_slice(b"PK\x03\x04");
        z.extend_from_slice(&20u16.to_le_bytes()); // version needed
        // general-purpose flags: bit 3 = sizes live in a trailing descriptor
        z.extend_from_slice(&if zero_size_fields { 8u16 } else { 0u16 }.to_le_bytes());
        z.extend_from_slice(&if deflate { 8u16 } else { 0u16 }.to_le_bytes());
        z.extend_from_slice(&[0u8; 8]); // dos time/date + crc (unchecked here)
        let sz = if zero_size_fields { 0 } else { data.len() as u32 };
        z.extend_from_slice(&sz.to_le_bytes()); // compressed size
        z.extend_from_slice(&sz.to_le_bytes()); // uncompressed size
        z.extend_from_slice(&(name.len() as u16).to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes()); // extra len
        z.extend_from_slice(name);
        z.extend_from_slice(&data);
        z
    }

    #[test]
    fn unzip_handles_deflate_stored_and_streaming_headers() {
        let payload = b"fake aoe2record bytes: not a real replay, just round-trip data";
        for (deflate, zero) in [(true, false), (true, true), (false, false)] {
            let z = make_zip(payload, deflate, zero);
            let out = unzip_single_member(&z).unwrap();
            assert_eq!(out.as_ref(), payload, "deflate={deflate} zero={zero}");
        }
        // Streaming + stored is ambiguous — must fail loud, never guess.
        assert!(unzip_single_member(&make_zip(payload, false, true)).is_err());
        // Garbage in — loud error out.
        assert!(unzip_single_member(b"not a zip at all").is_err());
        assert!(unzip_single_member(b"").is_err());
    }

    // Trimmed REAL getRecentMatchHistory response (probed 2026-07-02, profile_id
    // 199325 = "VIT | Hera", pulled live off the 1v1 RM ladder via getLeaderBoard2):
    // 2 AUTOMATCH matches (out of chronological order) + 1 non-AUTOMATCH entry
    // (real observed description value is "." here, not "CUSTOM" — the live API
    // does not use "CUSTOM" as a description string) that must be filtered out.
    // Note the "." entry has the LATEST completiontime of the three, proving the
    // filter runs before the newest-first sort.
    // Entry id 74166094 is a real probed AUTOMATCH match, but its completiontime
    // is SYNTHETIC (real value 1614653045, set to 0 here): no completiontime <= 0
    // appeared anywhere in the 129-entry probe, so this one field is fabricated
    // solely to exercise the defensive completed-only branch of the filter.
    const FIXTURE: &str = r#"{
      "matchHistoryStats": [
        {"id": 54100641, "completiontime": 1607456129, "description": "AUTOMATCH", "mapname": "Arabia.rms", "matchtype_id": 2,
         "matchhistorymember": [
           {"profile_id": 199325, "civilization_id": 6, "oldrating": 2062, "newrating": 2026, "outcome": 0},
           {"profile_id": 2653793, "civilization_id": 6, "oldrating": 2125, "newrating": 2134, "outcome": 1}]},
        {"id": 489464526, "completiontime": 1782951012, "description": ".", "mapname": "my map", "matchtype_id": 0,
         "matchhistorymember": [
           {"profile_id": 199325, "civilization_id": 43, "oldrating": 1714, "newrating": 1729, "outcome": 1},
           {"profile_id": 271202, "civilization_id": 25, "oldrating": 1686, "newrating": 1671, "outcome": 0}]},
        {"id": 74166094, "completiontime": 0, "description": "AUTOMATCH", "mapname": "Fortress.rms", "matchtype_id": 18,
         "matchhistorymember": [
           {"profile_id": 3309375, "civilization_id": 14, "oldrating": 1002, "newrating": 970, "outcome": 0},
           {"profile_id": 199325, "civilization_id": 11, "oldrating": 1000, "newrating": 1048, "outcome": 1}]},
        {"id": 83138685, "completiontime": 1618070120, "description": "AUTOMATCH", "mapname": "goldenpit.rms2", "matchtype_id": 2,
         "matchhistorymember": [
           {"profile_id": 214031, "civilization_id": 2, "oldrating": 1854, "newrating": 1846, "outcome": 0},
           {"profile_id": 199325, "civilization_id": 2, "oldrating": 2026, "newrating": 2052, "outcome": 1}]}
      ],
      "profiles": [{"profile_id": 199325, "alias": "VIT | Hera"}, {"profile_id": 214031, "alias": "HAMZA"}]
    }"#;

    #[test]
    fn recent_matches_filters_sorts_and_joins_me() {
        let doc: RecentHistoryResponse = serde_json::from_str(FIXTURE).unwrap();
        let ms = normalize_recent(doc, 199325);
        assert_eq!(ms.len(), 2); // "." (non-AUTOMATCH) and completiontime=0 dropped
        // the AUTOMATCH-but-uncompleted entry is filtered by the defensive branch
        assert!(ms.iter().all(|m| m.match_id != 74166094));
        assert_eq!(ms[0].match_id, 83138685); // newest first among AUTOMATCH only
        assert_eq!(ms[0].my_won, Some(true));
        assert_eq!(ms[0].my_rating, Some(2052)); // newrating preferred
        assert_eq!(ms[1].my_civ_id, Some(6));
        assert_eq!(ms[0].my_alias.as_deref(), Some("VIT | Hera"));
        assert_eq!(ms[0].team_size, 2);
    }
}
