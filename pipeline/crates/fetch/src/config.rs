//! Endpoint + rate configuration for the Relic fetch adapter.
//!
//! The API-facing subset of the old `scripts/data-pipeline/replay-rs/src/config.rs` (the semantic
//! AoE2 decoding constants — ages, eco-unit ids — live in the `replay` crate, not here; `fetch`
//! only downloads bytes, it never decodes them).

// --- World's Edge Link API (getReplayFiles + getRecentMatchHistory) ----------
pub const API_BASE: &str = "https://aoe-api.worldsedgelink.com/community/leaderboard";
pub const TITLE: &str = "age2";
pub const USER_AGENT: &str = "aoe2guide-replays/1.0 (self-collect)";

/// getReplayFiles is rate-limited (~120/min per the rlink docs); stay safely under.
pub const REPLAYFILES_PER_MIN: u32 = 100;
/// Max matchIDs per getReplayFiles call.
pub const REPLAYFILES_BATCH: usize = 10;

/// Bounded concurrency: a handful of connections, never a flood. Distinct from the request RATE
/// above — the governor limiter paces *how often* we send, the semaphore caps *how many* are in
/// flight at once (see the playbook: separate constraints).
pub const MAX_CONCURRENCY: usize = 4;
/// Per-request timeout (matches the old blocking client).
pub const REQUEST_TIMEOUT_SECS: u64 = 90;
/// Reactive retries (429/5xx, exponential backoff + jitter) per request, on top of the steady
/// governor rate. Matches the old client's 4-try `_get`.
pub const RETRY_MAX_RETRIES: u32 = 4;

// --- age archive (api.ageofempires.com — what aoe.ms redirects to) -----------
// Serves replays for matches YEARS old as a ZIP holding one
// AgeIIDE_Replay_<id>.aoe2record. Needs a PARTICIPANT profileId. The rate limit is harsh (a
// 5s-spaced probe hit a 429 wall after ~19 requests): the per-run pacing + budget for this
// endpoint is an ORCHESTRATION concern (M6), not a building block — `download_archive_replay`
// here does one match's participant loop and surfaces a persistent 429 as `RateLimited` for the
// caller to defer.
pub const ARCHIVE_BASE: &str = "https://api.ageofempires.com/api/GameStats/AgeII/GetMatchReplay/";
/// Try at most this many of a match's participants before giving up (404s are per-MATCH, so extra
/// tries mostly burn budget — 2 is cheap insurance). Probe-derived, 2026-07-04.
pub const ARCHIVE_MAX_PIDS_PER_MATCH: usize = 2;
