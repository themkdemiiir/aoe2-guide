//! The Relic/World's-Edge endpoints, as async building blocks over [`FetchClient`], plus their
//! PURE request-URL builders and response parsers (unit-tested with no network).
//!
//! Faithful async re-port of `scripts/data-pipeline/replay-rs/src/api.rs` (blocking → async;
//! strangler rule — the old crate is untouched). Three capabilities:
//!   - **discovery** ([`discover_recent`]) — recent ranked matches for one profile, as
//!     [`DiscoverySeed`]s (getRecentMatchHistory);
//!   - **fast-path download** ([`get_replay_files`] → [`best_file`] → [`download_replay`]) — the
//!     signed-blob path for recent matches;
//!   - **archive fallback** ([`download_archive_replay`]) — api.ageofempires.com, for matches that
//!     aged out of the fast path (what `aoe.ms/replay/…` 301-redirects to).

use std::collections::HashMap;
use std::io::{Cursor, Read as _};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use serde::Deserialize;

use pipeline_core::{MatchId, ProfileId};

use crate::client::{parse_retry_after, FetchClient};
use crate::config;
use crate::error::{Error, Result};
use crate::seed::{DiscoverySeed, RelicMatchType};

// --- pure request-URL builders ---------------------------------------------------------------
//
// Each takes `base` explicitly (rather than reaching for `config::API_BASE`/`ARCHIVE_BASE`
// itself) so the `_at` variants below — and their wiremock tests — can point at a mock server;
// the public `pub async fn`s always supply the real constant, so production call sites and the
// public API are unaffected.

/// getReplayFiles URL for a batch of match ids. The `matchIDs` list is URL-encoded exactly as the
/// old client did (`%5B id %2C id %5D`) — the endpoint is picky about the bracket/comma encoding.
pub(crate) fn replay_files_url(base: &str, match_ids: &[MatchId]) -> String {
    let inner = match_ids
        .iter()
        .map(|m| m.0.to_string())
        .collect::<Vec<_>>()
        .join("%2C");
    format!(
        "{base}/getReplayFiles?matchIDs=%5B{inner}%5D&title={}",
        config::TITLE
    )
}

/// getRecentMatchHistory URL for one profile.
pub(crate) fn recent_history_url(base: &str, profile_id: ProfileId) -> String {
    format!(
        "{base}/getRecentMatchHistory?title={}&profile_ids=%5B{}%5D",
        config::TITLE,
        profile_id.0
    )
}

/// Age-archive (GetMatchReplay) URL for one (match, participant).
pub(crate) fn archive_url(base: &str, match_id: MatchId, profile_id: ProfileId) -> String {
    format!(
        "{base}?gameId={}&profileId={}&matchId={}",
        match_id.0, profile_id.0, match_id.0
    )
}

// --- getReplayFiles (signed-blob fast path) ---------------------------------------------------

#[derive(Debug, Deserialize)]
struct ReplayFilesResponse {
    #[serde(default, rename = "replayFiles")]
    replay_files: Vec<ReplayFile>,
}

/// One uploaded replay file for a match (one per player who stayed in). The signed `url` is a
/// short-lived Azure blob URL — never log it.
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

/// Parse a getReplayFiles response body into `match_id → files`. Match ids absent from the result
/// have aged out of the fast path (the caller then tries the archive, or marks them expired).
pub(crate) fn parse_replay_files(raw: &[u8]) -> Result<HashMap<MatchId, Vec<ReplayFile>>> {
    let doc: ReplayFilesResponse = serde_json::from_slice(raw)?;
    let mut per_match: HashMap<MatchId, Vec<ReplayFile>> = HashMap::new();
    for f in doc.replay_files {
        if let Some(mid) = f.matchhistory_id {
            per_match.entry(MatchId(mid)).or_default().push(f);
        }
    }
    Ok(per_match)
}

/// The largest usable (size > 0, has a URL) file — the player who stayed longest, i.e. the most
/// complete replay.
pub fn best_file(files: &[ReplayFile]) -> Option<&ReplayFile> {
    files
        .iter()
        .filter(|f| f.size.unwrap_or(-1) > 0 && f.url.is_some())
        .max_by_key(|f| f.size.unwrap_or(-1))
}

/// Look up signed URLs for any number of match ids (rate-limited via `client`), chunking
/// internally at `config::REPLAYFILES_BATCH` — the Relic API's own per-call cap — so callers don't
/// need to know about that quirk. Each chunk is a separate, individually rate-limited request.
pub async fn get_replay_files(
    client: &FetchClient,
    match_ids: &[MatchId],
) -> Result<HashMap<MatchId, Vec<ReplayFile>>> {
    get_replay_files_at(client, config::API_BASE, match_ids).await
}

/// [`get_replay_files`] against an explicit `base` — the seam wiremock tests use to point at a
/// mock server instead of the live Relic API.
pub(crate) async fn get_replay_files_at(
    client: &FetchClient,
    base: &str,
    match_ids: &[MatchId],
) -> Result<HashMap<MatchId, Vec<ReplayFile>>> {
    let mut per_match = HashMap::new();
    for chunk in match_ids.chunks(config::REPLAYFILES_BATCH) {
        let url = replay_files_url(base, chunk);
        let raw = client.get_bytes(&url, "getReplayFiles").await?;
        per_match.extend(parse_replay_files(&raw)?);
    }
    Ok(per_match)
}

/// Download a signed blob and gunzip it to raw `.aoe2record` bytes. We gunzip the gzip MEMBER
/// ourselves (rather than relying on transparent transfer decoding) so the parser always receives
/// raw record bytes regardless of how the CDN sets its content headers. `url` is `skip`ped from the
/// span — it holds a SAS token.
#[tracing::instrument(skip(client, url), fields(match_id = %match_id))]
pub async fn download_replay(client: &FetchClient, match_id: MatchId, url: &str) -> Result<Bytes> {
    let raw = client.get_bytes(url, "signed replay blob").await?;
    let mut out = Vec::new();
    GzDecoder::new(raw.as_ref()).read_to_end(&mut out)?;
    Ok(Bytes::from(out))
}

// --- getRecentMatchHistory (discovery) --------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RecentHistoryResponse {
    #[serde(default, rename = "matchHistoryStats")]
    match_history_stats: Vec<MatchStat>,
}

#[derive(Debug, Deserialize)]
struct MatchStat {
    id: i64,
    #[serde(default)]
    completiontime: i64,
    #[serde(default)]
    description: String,
    #[serde(default)]
    matchtype_id: i32,
    #[serde(default)]
    matchhistorymember: Vec<MatchMember>,
}

#[derive(Debug, Deserialize)]
struct MatchMember {
    profile_id: i64,
    #[serde(default)]
    oldrating: Option<i32>,
    #[serde(default)]
    newrating: Option<i32>,
}

/// AUTOMATCH + completed only, newest first, each mapped to a [`DiscoverySeed`]. The 1v1/team
/// classification comes from `matchtype_id` (the SAME field + vocab
/// `scripts/data-pipeline/lib/relic-api.mjs`'s `normalizeMatches`/`relic-map.mjs`'s `isRankedRm`
/// key the production JS crawler's ranked-RM filter on — see
/// [`RelicMatchType::from_matchtype_id`]). Most AUTOMATCH matches are NOT ranked RM (empire wars,
/// death match, quick play, ...) and are expected to be skipped here at `debug` — this is routine,
/// not a data-corruption signal. Member count alone would NOT be a safe substitute: a ranked death
/// match 1v1 has exactly 2 members too, so counting members can't tell it apart from a ranked RM
/// 1v1 — exactly the silent-mislabel failure mode `matchtype_id`-based classification avoids.
fn normalize_recent(doc: RecentHistoryResponse, profile_id: ProfileId) -> Result<Vec<DiscoverySeed>> {
    let mut out = Vec::new();
    for m in doc.match_history_stats {
        // AUTOMATCH = ranked matchmaking; completiontime > 0 excludes in-progress/unreported.
        if m.description != "AUTOMATCH" || m.completiontime <= 0 {
            continue;
        }
        let match_type = match RelicMatchType::from_matchtype_id(m.matchtype_id) {
            Ok(t) => t,
            Err(err) => {
                tracing::debug!(
                    match_id = m.id,
                    matchtype_id = m.matchtype_id,
                    error = %err,
                    "skipping discovered automatch with a non-ranked-RM matchtype_id"
                );
                continue;
            }
        };
        let played_at: DateTime<Utc> =
            DateTime::from_timestamp(m.completiontime, 0).ok_or(Error::BadTimestamp(m.completiontime))?;
        // "me" fields joined by profile_id; newrating is the post-game rating, oldrating the
        // pre-game fallback when newrating is absent (e.g. an unrated game).
        let me = m.matchhistorymember.iter().find(|x| x.profile_id == profile_id.0);
        let new_rating = me.and_then(|x| x.newrating.or(x.oldrating));
        out.push(DiscoverySeed {
            match_id: MatchId(m.id),
            match_type,
            played_at,
            new_rating,
        });
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.played_at));
    Ok(out)
}

/// Recent ranked (AUTOMATCH) games for one profile, as discovery seeds, newest first.
pub async fn discover_recent(
    client: &FetchClient,
    profile_id: ProfileId,
) -> Result<Vec<DiscoverySeed>> {
    discover_recent_at(client, config::API_BASE, profile_id).await
}

/// [`discover_recent`] against an explicit `base` — the seam wiremock tests use to point at a
/// mock server instead of the live Relic API.
pub(crate) async fn discover_recent_at(
    client: &FetchClient,
    base: &str,
    profile_id: ProfileId,
) -> Result<Vec<DiscoverySeed>> {
    let url = recent_history_url(base, profile_id);
    let raw = client.get_bytes(&url, "getRecentMatchHistory").await?;
    let doc: RecentHistoryResponse = serde_json::from_slice(&raw)?;
    normalize_recent(doc, profile_id)
}

// --- age-archive fallback ---------------------------------------------------------------------

/// Outcome of one archive lookup across a match's participants. The rate-limit + budget PACING of
/// repeated archive calls is an M6 orchestration concern — this building block does a single
/// match's participant loop and reports which terminal-or-retry bucket it landed in.
#[derive(Debug)]
pub enum ArchiveFetch {
    /// Raw `.aoe2record` bytes + the participant profile id that served them.
    Ok(Bytes, ProfileId),
    /// Every tried participant returned 404 — the archive has no copy (terminal).
    NotFound,
    /// A 429 survived the retry middleware — retry the whole match on a later run.
    RateLimited { retry_after: Option<u64> },
    /// 200 arrived but the ZIP was structurally unusable — deterministic, so terminal.
    BadPayload(String),
}

/// Try each participant until one yields the replay. 404 → try the next participant; a surviving
/// 429 → [`ArchiveFetch::RateLimited`]; a structurally bad ZIP → [`ArchiveFetch::BadPayload`]
/// (deterministic); other non-2xx / network failures bubble as `Err` (retryable). Paced by
/// `client`'s limiter — the caller owns the harsher archive spacing + per-run budget (M6).
#[tracing::instrument(skip(client, profile_ids), fields(match_id = %match_id))]
pub async fn download_archive_replay(
    client: &FetchClient,
    match_id: MatchId,
    profile_ids: &[ProfileId],
) -> Result<ArchiveFetch> {
    download_archive_replay_at(client, config::ARCHIVE_BASE, match_id, profile_ids).await
}

/// [`download_archive_replay`] against an explicit `base` — the seam wiremock tests use to point
/// at a mock server instead of the live age-archive endpoint.
pub(crate) async fn download_archive_replay_at(
    client: &FetchClient,
    base: &str,
    match_id: MatchId,
    profile_ids: &[ProfileId],
) -> Result<ArchiveFetch> {
    for &pid in profile_ids.iter().take(config::ARCHIVE_MAX_PIDS_PER_MATCH) {
        let url = archive_url(base, match_id, pid);
        let resp = client.get_response(&url).await?;
        match resp.status().as_u16() {
            200 => {
                let body = resp.bytes().await?;
                return match unzip_single_member(&body) {
                    Ok(record) => Ok(ArchiveFetch::Ok(record, pid)),
                    // A structurally bad ZIP won't improve on retry.
                    Err(Error::BadArchive(msg)) => Ok(ArchiveFetch::BadPayload(msg)),
                    Err(other) => Err(other),
                };
            }
            404 => continue, // this participant has no copy — try the next one
            429 => {
                return Ok(ArchiveFetch::RateLimited {
                    retry_after: parse_retry_after(resp.headers()),
                })
            }
            other => {
                return Err(Error::Status {
                    status: other,
                    endpoint: "GetMatchReplay",
                })
            }
        }
    }
    Ok(ArchiveFetch::NotFound)
}

/// Single-member ZIP extraction via the `zip` crate (the archive always ships exactly one
/// `.aoe2record`, produced by .NET) — stored and deflated members both work, since `ZipArchive`
/// reads sizes/offsets off the archive's central directory rather than trusting the (possibly
/// zeroed, for a non-seekable writer) local file header. Anything structurally unusable — not a
/// zip, no central directory, a corrupt member — fails loud as [`Error::BadArchive`] rather than
/// guessing at raw bytes.
pub(crate) fn unzip_single_member(zip: &[u8]) -> Result<Bytes> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip))
        .map_err(|e| Error::BadArchive(format!("not a readable zip archive: {e}")))?;
    if archive.is_empty() {
        return Err(Error::BadArchive("zip archive has no entries".to_owned()));
    }
    let mut member = archive
        .by_index(0)
        .map_err(|e| Error::BadArchive(format!("reading the zip's single member failed: {e}")))?;
    let mut out = Vec::new();
    member
        .read_to_end(&mut out)
        .map_err(|e| Error::BadArchive(format!("inflating the zip member failed: {e}")))?;
    Ok(Bytes::from(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_builders_encode_relic_params() {
        assert_eq!(
            replay_files_url(config::API_BASE, &[MatchId(1), MatchId(2), MatchId(3)]),
            "https://aoe-api.worldsedgelink.com/community/leaderboard/getReplayFiles?matchIDs=%5B1%2C2%2C3%5D&title=age2"
        );
        assert_eq!(
            recent_history_url(config::API_BASE, ProfileId(199325)),
            "https://aoe-api.worldsedgelink.com/community/leaderboard/getRecentMatchHistory?title=age2&profile_ids=%5B199325%5D"
        );
        assert_eq!(
            archive_url(config::ARCHIVE_BASE, MatchId(500), ProfileId(42)),
            "https://api.ageofempires.com/api/GameStats/AgeII/GetMatchReplay/?gameId=500&profileId=42&matchId=500"
        );
    }

    #[test]
    fn parse_replay_files_groups_by_match_and_picks_best() {
        // Two files for match 10 (sizes 100/250), one aged-out entry with no matchhistory_id.
        let raw = br#"{"replayFiles":[
            {"profile_id":1,"matchhistory_id":10,"size":100,"url":"u1"},
            {"profile_id":2,"matchhistory_id":10,"size":250,"url":"u2"},
            {"profile_id":3,"matchhistory_id":11,"size":-1,"url":"u3"},
            {"profile_id":4,"size":999,"url":"orphan"}
        ]}"#;
        let per_match = parse_replay_files(raw).unwrap();
        assert_eq!(per_match.len(), 2);

        // best_file = largest usable.
        let best = best_file(&per_match[&MatchId(10)]).unwrap();
        assert_eq!(best.size, Some(250));
        assert_eq!(best.url.as_deref(), Some("u2"));

        // match 11 has only a size -1 file -> no usable best.
        assert!(best_file(&per_match[&MatchId(11)]).is_none());
    }

    // Trimmed REAL getRecentMatchHistory response (probed 2026-07-02, profile 199325 "VIT | Hera"):
    // two completed AUTOMATCH matches (out of order) with REAL but NON-ranked-RM matchtype_ids (2,
    // 18 — empire wars / some other automatch mode, per relic-map.mjs's "rest = other modes"), one
    // "." (non-AUTOMATCH, matchtype_id 0) that must drop on the description gate, and one AUTOMATCH
    // with a synthetic completiontime=0 that the defensive filter must drop. NONE of these four
    // real rows have a ranked-RM matchtype_id (6/7/8/9) — see the next test for that path.
    const FIXTURE: &str = r#"{
      "matchHistoryStats": [
        {"id": 54100641, "completiontime": 1607456129, "description": "AUTOMATCH", "matchtype_id": 2,
         "matchhistorymember": [
           {"profile_id": 199325, "oldrating": 2062, "newrating": 2026, "outcome": 0},
           {"profile_id": 2653793, "oldrating": 2125, "newrating": 2134, "outcome": 1}]},
        {"id": 489464526, "completiontime": 1782951012, "description": ".", "matchtype_id": 0,
         "matchhistorymember": [
           {"profile_id": 199325, "oldrating": 1714, "newrating": 1729, "outcome": 1},
           {"profile_id": 271202, "oldrating": 1686, "newrating": 1671, "outcome": 0}]},
        {"id": 74166094, "completiontime": 0, "description": "AUTOMATCH", "matchtype_id": 18,
         "matchhistorymember": [
           {"profile_id": 3309375, "oldrating": 1002, "newrating": 970, "outcome": 0},
           {"profile_id": 199325, "oldrating": 1000, "newrating": 1048, "outcome": 1}]},
        {"id": 83138685, "completiontime": 1618070120, "description": "AUTOMATCH", "matchtype_id": 2,
         "matchhistorymember": [
           {"profile_id": 214031, "oldrating": 1854, "newrating": 1846, "outcome": 0},
           {"profile_id": 199325, "oldrating": 2026, "newrating": 2052, "outcome": 1}]}
      ]
    }"#;

    #[test]
    fn normalize_recent_drops_non_automatch_incomplete_and_non_ranked_matchtype_rows() {
        let doc: RecentHistoryResponse = serde_json::from_str(FIXTURE).unwrap();
        let seeds = normalize_recent(doc, ProfileId(199325)).unwrap();

        // Every real probed row is either non-AUTOMATCH, incomplete, or a non-ranked-RM
        // matchtype_id — none classify as ranked RM, so this yields NOTHING. This is the routine
        // case (most automatch traffic on a profile isn't ranked RM), not an error.
        assert!(
            seeds.is_empty(),
            "no row in the real fixture has a ranked-RM matchtype_id (6/7/8/9)"
        );
    }

    // SYNTHETIC (not from the 2026-07-02 probe): two AUTOMATCH rows with matchtype_id 6, added
    // solely to exercise the ranked-RM happy path (rating fallback, played_at conversion, newest
    // -first sort) — the real fixture above has no such row. Fabricated test SHAPE, not a
    // fabricated AoE2 FACT: the matchtype_id=6 vocab itself is sourced (see
    // `RelicMatchType::from_matchtype_id`'s doc), only these two ids/timestamps are made up.
    const SYNTHETIC_RANKED_FIXTURE: &str = r#"{
      "matchHistoryStats": [
        {"id": 1, "completiontime": 1607456129, "description": "AUTOMATCH", "matchtype_id": 6,
         "matchhistorymember": [
           {"profile_id": 199325, "oldrating": 2062, "newrating": 2026},
           {"profile_id": 2653793, "oldrating": 2125, "newrating": 2134}]},
        {"id": 2, "completiontime": 1618070120, "description": "AUTOMATCH", "matchtype_id": 6,
         "matchhistorymember": [
           {"profile_id": 214031, "oldrating": 1854, "newrating": 1846},
           {"profile_id": 199325, "oldrating": 2026, "newrating": 2052}]}
      ]
    }"#;

    #[test]
    fn normalize_recent_builds_seeds_for_ranked_rm_matches_newest_first() {
        let doc: RecentHistoryResponse = serde_json::from_str(SYNTHETIC_RANKED_FIXTURE).unwrap();
        let seeds = normalize_recent(doc, ProfileId(199325)).unwrap();

        assert_eq!(seeds.len(), 2);
        // Newest played_at first (match 2 @1618070120 before match 1 @1607456129).
        assert_eq!(seeds[0].match_id, MatchId(2));
        assert_eq!(seeds[1].match_id, MatchId(1));
        assert_eq!(seeds[0].match_type, RelicMatchType::SoloRmRanked);
        // newrating preferred for "me" (profile 199325).
        assert_eq!(seeds[0].new_rating, Some(2052));
        assert_eq!(seeds[1].new_rating, Some(2026));
        assert_eq!(
            seeds[0].played_at,
            DateTime::from_timestamp(1618070120, 0).unwrap()
        );
    }

    /// Build a one-member zip in memory via the `zip` crate's own writer (never hand-rolled bytes)
    /// — mirrors what the archive endpoint ships: a single `AgeIIDE_Replay_<id>.aoe2record` member.
    fn make_zip(payload: &[u8], method: zip::CompressionMethod) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default().compression_method(method);
        writer
            .start_file("AgeIIDE_Replay_1.aoe2record", options)
            .unwrap();
        std::io::Write::write_all(&mut writer, payload).unwrap();
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn unzip_extracts_deflated_and_stored_members() {
        let payload = b"fake aoe2record bytes: not a real replay, just round-trip data";
        for method in [zip::CompressionMethod::Deflated, zip::CompressionMethod::Stored] {
            let z = make_zip(payload, method);
            let out = unzip_single_member(&z).unwrap();
            assert_eq!(out.as_ref(), payload, "method={method:?}");
        }
    }

    #[test]
    fn unzip_fails_loud_on_garbage_empty_and_truncated_input() {
        // Not a zip at all, and empty input.
        assert!(unzip_single_member(b"not a zip at all").is_err());
        assert!(unzip_single_member(b"").is_err());
        // A real zip with its tail (central directory + EOCD) cut off — a genuinely incomplete
        // download must fail loud rather than guess at whatever bytes happened to arrive.
        let full = make_zip(b"payload", zip::CompressionMethod::Stored);
        let truncated = &full[..full.len() / 2];
        assert!(unzip_single_member(truncated).is_err());
    }

    // --- wiremock-backed HTTP-layer tests -----------------------------------------------------
    //
    // These exercise the FULL path through `FetchClient` (governor + semaphore + the stock
    // `RetryTransientMiddleware`) against a local mock server — never the live Relic API. The
    // `_at`/`with_limits_and_retries` seams above/in `client.rs` exist FOR these tests: production
    // callers only ever see the public `get_replay_files`/`discover_recent`/`download_archive_replay`,
    // which always point at the real Relic constants.

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn get_replay_files_then_best_file_then_download_replay_round_trip() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/getReplayFiles"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "replayFiles": [
                    {"profile_id": 1, "matchhistory_id": 10, "size": 100, "url": "u1"},
                    {"profile_id": 2, "matchhistory_id": 10, "size": 250, "url": format!("{}/blob", server.uri())},
                ]
            })))
            .mount(&server)
            .await;

        // The signed blob itself: gzip-compressed raw ".aoe2record" bytes.
        let raw_record = b"pretend .aoe2record bytes";
        let mut gz = Vec::new();
        {
            use std::io::Write as _;
            let mut enc = flate2::write::GzEncoder::new(&mut gz, flate2::Compression::default());
            enc.write_all(raw_record).unwrap();
            enc.finish().unwrap();
        }
        Mock::given(method("GET"))
            .and(path("/blob"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(gz))
            .mount(&server)
            .await;

        let client = FetchClient::with_limits_and_retries(1000, 4, 0).unwrap();
        let per_match = get_replay_files_at(&client, &server.uri(), &[MatchId(10)])
            .await
            .unwrap();
        let best = best_file(&per_match[&MatchId(10)]).unwrap();
        assert_eq!(best.size, Some(250));

        let downloaded = download_replay(&client, MatchId(10), best.url.as_ref().unwrap())
            .await
            .unwrap();
        assert_eq!(downloaded.as_ref(), raw_record);
    }

    #[tokio::test]
    async fn discover_recent_round_trips_through_http_and_normalizes() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/getRecentMatchHistory"))
            .respond_with(ResponseTemplate::new(200).set_body_string(SYNTHETIC_RANKED_FIXTURE))
            .mount(&server)
            .await;

        let client = FetchClient::with_limits_and_retries(1000, 4, 0).unwrap();
        let seeds = discover_recent_at(&client, &server.uri(), ProfileId(199325))
            .await
            .unwrap();

        assert_eq!(seeds.len(), 2);
        assert_eq!(seeds[0].match_id, MatchId(2), "newest played_at first");
        assert_eq!(seeds[0].match_type, RelicMatchType::SoloRmRanked);
    }

    #[tokio::test]
    async fn get_bytes_retries_a_transient_500_via_the_stock_retry_middleware() {
        let server = MockServer::start().await;
        // First call 500s, second succeeds — proves `RetryTransientMiddleware` (not a hand-rolled
        // retry loop) actually retries a transient failure.
        Mock::given(method("GET"))
            .and(path("/getReplayFiles"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/getReplayFiles"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "replayFiles": [] })),
            )
            .mount(&server)
            .await;

        let client = FetchClient::with_limits_and_retries(1000, 4, 2).unwrap();
        let per_match = get_replay_files_at(&client, &server.uri(), &[MatchId(1)])
            .await
            .unwrap();
        assert!(per_match.is_empty());
    }

    #[tokio::test]
    async fn a_persistent_429_surfaces_as_rate_limited_with_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/getReplayFiles"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "42"))
            .mount(&server)
            .await;

        // Zero retries: the middleware's own backoff never has a chance to clear the 429, so the
        // client must surface it as `Error::RateLimited` rather than treat it as a hard failure.
        let client = FetchClient::with_limits_and_retries(1000, 4, 0).unwrap();
        let err = get_replay_files_at(&client, &server.uri(), &[MatchId(1)])
            .await
            .unwrap_err();
        assert!(
            matches!(err, Error::RateLimited { retry_after: Some(42) }),
            "expected RateLimited{{retry_after: Some(42)}}, got {err:?}"
        );
    }
}
