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
