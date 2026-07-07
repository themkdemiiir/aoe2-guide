//! `pipeline import-shards` — migrates the OLD parsed-replay shard corpus
//! (`data-cache/replays/shards/{meta,players,events,ages}.ndjson.gz`, produced by the retired
//! `scripts/data-pipeline/replay-rs` extractor) into the live Postgres pipeline as
//! `matches.source = 'replay'`, re-deriving the FULL enrichment (opening_kind/apm/timings/
//! units/techs) through the SAME [`crate::to_batch`] + `ingest::ingest_batch` path a live crawl
//! uses — see the crate-level doc's "composition root" note. This is the ONE place a historical
//! shard row + the DuckDB seed snapshot compose into a [`replay::ParsedReplay`] +
//! [`fetch::DiscoverySeed`] pair, mirroring how [`crate::crawl`] composes a freshly-downloaded
//! replay + a freshly-discovered seed.
//!
//! ## Why re-derive instead of trusting the old corpus's own (nonexistent) summary
//! The old shards carry only the FOUR raw tables (`meta`/`players`/`events`/`ages`); the rich
//! `opening`/`opening_kind`/`feudal_t`/`castle_t`/`imperial_t`/`apm`/`match_player_units`/
//! `match_player_techs` enrichment ([`replay::derive`]) did not exist when this corpus was
//! extracted. Reconstructing a [`replay::ParsedReplay`] from the four raw shard rows and running
//! it through the CURRENT [`crate::to_batch`] (which calls `replay::derive` internally) gets the
//! full enrichment for free, instead of re-parsing 1.57M `.aoe2record` files (most of which no
//! longer exist on disk — only the pre-extracted NDJSON survives).
//!
//! ## What the shards DON'T carry (and where it comes from instead)
//! - **`elo`**: the shard's `players` rows predate `aa33649` (`feat(replay-rs): extract post-game
//!   elo into PlayerRow`) — every historical row is missing it (see [`ShardPlayerRow`], which
//!   deliberately has no `elo` field at all). The DuckDB snapshot's `games` table
//!   (`source <> 'aoestats'`, the same 1,566,511 replay matches) carries `rating` — the Relic
//!   API's OWN post-game elo for that player, captured at crawl time, keyed by
//!   `(match_id, profile_id)`. [`load_seed`] fills [`replay::ReplayPlayer::elo`] from it — `None`
//!   (never fabricated) when the pair has no `games` row.
//! - **`played_at`/`ladder`**: a replay's own bytes never carry these
//!   ([`replay::ParsedReplay`]'s own doc) — the same `games` snapshot supplies them per match,
//!   building the [`fetch::DiscoverySeed`] `to_batch` needs. `new_rating` is left `None`: the
//!   post-game `elo` filled onto each player above is authoritative (`to_batch`'s own doc), so
//!   there is nothing honest to put in the seed's single-profile fallback slot.
//!
//! ## Tolerant gzip reads (a known, accepted corpus defect)
//! Every shard is a GZIP MULTI-MEMBER stream (`scripts/data-pipeline/replay-rs/src/store.rs`:
//! one member per `write_rows` flush, so a resumed extraction run just appends more members).
//! The corpus's LAST member is corrupt/partial (an interrupted extraction run) — `zcat`/
//! [`MultiGzDecoder`] decode every complete member fine and error only on that final one.
//! [`scan_ndjson_gz`] treats that error as "stop, keep everything decoded so far" (mirrors
//! `zcat ... 2>/dev/null`) rather than failing the whole shard.
//!
//! ## PHASE 1 vs PHASE 2 scan strategy
//! The four shards are NOT sorted or co-indexed by `match_id` (`store.rs`'s rayon workers append
//! whichever match finishes first). This binary's only mode — filter-while-scanning every shard
//! for a target `match_id` set built from `meta`'s first `--limit` rows — means every OTHER
//! shard is read IN FULL regardless of how small `--limit` is (the target ids could be anywhere
//! in the file). That's an accepted, explicit PHASE 1 cost (a one-time full scan of the ~11GB
//! compressed `events` shard) for "correctness on a sample" — minutes, not the bottleneck for a
//! few hundred matches.
//!
//! It does NOT scale to the PHASE 2 (all 1.57M matches) run: at `--limit 0` this binary still
//! just linearly filters, but with a target set that IS the whole file there is no longer a
//! "most of the file is irrelevant" saving, AND [`load_seed`]'s literal `IN (...)` DuckDB query
//! hard-caps at [`MAX_SEED_IDS`] ids (never silently truncates the sample). The design for that
//! run (not implemented here, left as a follow-up task): decompress each shard ONCE to a plain
//! (tolerantly-truncated) `.ndjson` file, load each into its own DuckDB TABLE (now indexable —
//! DuckDB's native reader can't handle the corrupt gzip tail, but it never sees it once the
//! decompression already happened in Rust), then drive the match loop off
//! `SELECT DISTINCT match_id FROM meta_tbl` with indexed `JOIN`s into `players_tbl`/`events_tbl`/
//! `ages_tbl`/`games`, instead of four full in-memory scans per run.
//!
//! ## Resumability / idempotency
//! `ingest::ingest_batch` is already idempotent (`ON CONFLICT (match_id) DO NOTHING`, children
//! gated on genuinely-new match ids) — re-running this binary over the same `--limit` is always
//! safe; matches already in `matches` cost one skipped `COPY` row and nothing else (counted in
//! [`ImportShardsSummary::already_present`]).
//!
//! ## Load safety
//! [`ImportShardsConfig::batch_size`] bounds how many matches share one `ingest_batch`
//! transaction (never one unbatched multi-million-row write — the DB wedged once on that shape).
//! Each batch's wall-clock `ingest_batch` time is logged; a batch slower than
//! [`ImportShardsConfig::slow_batch`] triggers a [`ImportShardsConfig::pause`] sleep before the
//! next one starts, as a self-contained backoff signal that the DB may be under load (this binary
//! has no standing credential to query the Postgres host's own OS load, unlike the desktop
//! operator, who can cross-check via `pct exec 101 -- cat /proc/loadavg` on `botanera` during a
//! run).
//!
//! A batch that fails outright (e.g. a genuine FK violation) is NOT split and retried
//! match-by-match — the whole batch's matches are counted
//! [`ImportShardsSummary::skipped_ingest_err`] and logged with their ids for investigation; since
//! `ingest_batch` is idempotent, simply re-running the importer retries them (after a smaller
//! `--batch-size` if isolating the bad match is needed).

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use flate2::read::MultiGzDecoder;
use serde::Deserialize;
use tokio_postgres::Client;

use fetch::{DiscoverySeed, RelicMatchType};
use ingest::{ingest_batch, ReplayBatch};
use pipeline_core::{Age, GameCivId, MatchId, ProfileId};
use replay::{ParsedReplay, ReplayAge, ReplayEvent, ReplayPlayer};

/// Cap on how many literal ids [`load_seed`] will inline into one DuckDB `IN (...)` query — see
/// the module doc's "PHASE 1 vs PHASE 2" note: this whole seed-loading strategy is PHASE-1-only.
const MAX_SEED_IDS: usize = 5_000;

/// Tunables for one `import-shards` run.
pub struct ImportShardsConfig {
    /// Directory containing `{meta,players,events,ages}.ndjson.gz`.
    pub shards_dir: PathBuf,
    /// Path to the read-only DuckDB snapshot carrying `games`.
    pub duckdb_path: PathBuf,
    /// Path to the `duckdb` CLI binary (not always on `PATH` — e.g. `~/bin/duckdb`).
    pub duckdb_bin: PathBuf,
    /// Stop after this many `meta` rows, in file order. 0 = every match_id in `meta` — see the
    /// module doc for why that PHASE 2 mode isn't recommended without the staging rework it
    /// describes (and why [`load_seed`] will refuse it past [`MAX_SEED_IDS`] anyway).
    pub limit: usize,
    /// Matches per `ingest_batch` transaction.
    pub batch_size: usize,
    /// A batch taking longer than this is treated as a load-safety signal (see the module doc).
    pub slow_batch: Duration,
    /// How long to pause after a slow batch.
    pub pause: Duration,
}

/// Outcome counts for one `import-shards` run — see the module doc for what each skip reason
/// means and why nothing here is ever fabricated to paper over a gap.
#[derive(Debug, Default, Clone)]
pub struct ImportShardsSummary {
    pub meta_rows_read: u64,
    pub meta_truncated: bool,
    pub target_matches: u64,
    pub players_truncated: bool,
    pub ages_truncated: bool,
    pub events_truncated: bool,
    /// Genuinely new `matches` rows written (summed `IngestStats::matches_inserted`).
    pub imported: u64,
    /// Matches already present before this run (idempotent re-ingest — see the module doc).
    pub already_present: u64,
    /// This match's shard had zero `players` rows.
    pub skipped_no_players: u64,
    /// No `games` row for this match_id (no `played_at`/`ladder` to build a seed from).
    pub skipped_no_seed: u64,
    /// The `games` row's `ladder` was neither `"1v1"` nor `"team"` (unreachable with today's
    /// data — see [`relic_match_type_from_ladder`] — kept fail-loud, never guessed).
    pub skipped_unknown_ladder: u64,
    /// A shard row's numeric field didn't fit the target Rust type (never silently narrowed).
    pub skipped_bad_numeric: u64,
    /// An `ages` row's `age` string wasn't one of the four closed [`Age`] values.
    pub skipped_bad_age: u64,
    /// [`crate::to_batch`] itself rejected the match (mismatched id, missing map_id, unmapped
    /// ladder, or a unit-composition overflow — see [`crate::Error`]).
    pub skipped_to_batch_err: u64,
    /// The match's `ingest_batch` transaction failed outright (see the module doc's "Load
    /// safety" section for why this is not automatically retried match-by-match).
    pub skipped_ingest_err: u64,
}

// --- shard row shapes (mirror `scripts/data-pipeline/replay-rs/src/extract.rs`'s historical
// output — see the module doc for why `ShardPlayerRow` has no `elo` field). Every JSON line's
// `match_id` is its FIRST key ([`leading_match_id`] depends on that order to skip full JSON
// parses for out-of-sample rows), but `ShardPlayerRow`/`ShardEventRow`/`ShardAgeRow` deliberately
// don't re-declare a `match_id` FIELD: [`scan_filtered`] already keys its output map on the value
// `leading_match_id` extracted, so a second copy on the struct would just be dead weight — serde
// silently ignores JSON object keys with no matching struct field (no `deny_unknown_fields`
// anywhere in this crate), so this is safe. `ShardMetaRow` is the one exception: it's read
// whole (not grouped by an externally-supplied key), so its own `match_id` field is load-bearing.

#[derive(Debug, Deserialize)]
struct ShardMetaRow {
    match_id: i64,
    build: Option<i64>,
    map_id: Option<i64>,
    duration_ms: i64,
    n_players: i64,
}

#[derive(Debug, Deserialize)]
struct ShardPlayerRow {
    player_number: i64,
    profile_id: i64,
    civ_id: i64,
    name: String,
    team: i64,
    color: i64,
    won: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ShardEventRow {
    profile_id: Option<i64>,
    player_number: i64,
    t_ms: i64,
    kind: String,
    target_id: Option<i64>,
    amount: Option<i64>,
    detail: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ShardAgeRow {
    profile_id: i64,
    civ_id: i64,
    won: Option<bool>,
    age: String,
    uptime_ms: i64,
    villagers: i64,
    military: i64,
    n_buildings: i64,
    n_research: i64,
}

/// One `games` row, as `duckdb -json` renders it (see [`load_seed`]).
#[derive(Debug, Deserialize)]
struct SeedRowRaw {
    match_id: i64,
    profile_id: i64,
    played_at: String,
    ladder: String,
    rating: Option<i64>,
}

/// The match-level facts [`load_seed`] resolves per `match_id` (player-level `rating` is kept
/// separately — see [`load_seed`]'s `elo_by_key` return value).
struct SeedInfo {
    ladder: String,
    played_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, Copy)]
struct ScanStats {
    lines_ok: u64,
    truncated: bool,
}

/// Streams `path` (tolerant of a corrupt trailing gzip member — see the module doc), calling
/// `on_line` for every successfully gzip-decoded, non-empty line. A decode error mid-stream stops
/// the read and is logged, never propagated as a hard failure — see the module doc's "Tolerant
/// gzip reads" section for why that is the intended, accepted behavior for this corpus.
fn scan_ndjson_gz(path: &Path, mut on_line: impl FnMut(&str)) -> Result<ScanStats> {
    let file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = BufReader::new(MultiGzDecoder::new(file));
    let mut stats = ScanStats::default();
    let mut buf = String::new();
    loop {
        buf.clear();
        match reader.read_line(&mut buf) {
            Ok(0) => break,
            Ok(_) => {
                let line = buf.trim_end_matches(['\n', '\r']);
                if !line.is_empty() {
                    on_line(line);
                    stats.lines_ok += 1;
                }
            }
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    lines_ok = stats.lines_ok,
                    error = %err,
                    "gzip stream ended early (tolerant read) — keeping the valid prefix"
                );
                stats.truncated = true;
                break;
            }
        }
    }
    Ok(stats)
}

/// Leading `"match_id":<int>` extracted without a full JSON parse. Every shard row type
/// serializes `match_id` as its first field (see the module doc), so this cheap prefix check lets
/// [`scan_filtered`] skip full deserialization for the overwhelming majority of `events`/`ages`/
/// `players` rows that belong to a match OUTSIDE the current target set — full `serde_json`
/// parsing only ever runs on rows that are actually kept.
fn leading_match_id(line: &str) -> Option<i64> {
    let rest = line.strip_prefix("{\"match_id\":")?;
    let end = rest.find(',')?;
    rest.get(..end)?.parse().ok()
}

/// Scans `path`, fully deserializing (as `T`) and grouping by `match_id` only the lines whose
/// [`leading_match_id`] is in `targets` — see the module doc's "PHASE 1 vs PHASE 2 scan strategy".
fn scan_filtered<T>(path: &Path, targets: &HashSet<i64>) -> Result<(HashMap<i64, Vec<T>>, bool)>
where
    T: for<'de> Deserialize<'de>,
{
    let mut out: HashMap<i64, Vec<T>> = HashMap::new();
    let mut parse_errors = 0u64;
    let stats = scan_ndjson_gz(path, |line| {
        let Some(mid) = leading_match_id(line) else {
            return;
        };
        if !targets.contains(&mid) {
            return;
        }
        match serde_json::from_str::<T>(line) {
            Ok(row) => out.entry(mid).or_default().push(row),
            Err(_) => parse_errors += 1,
        }
    })?;
    tracing::info!(
        path = %path.display(),
        lines_scanned = stats.lines_ok,
        truncated = stats.truncated,
        matches_found = out.len(),
        parse_errors,
        "shard scanned"
    );
    Ok((out, stats.truncated))
}

/// Maps the DuckDB `games.ladder` string (already reduced to `"1v1"`/`"team"` by the crawl that
/// built the snapshot) directly onto the two constructable [`RelicMatchType`] variants — the SAME
/// two-value vocabulary `RelicMatchType::from_matchtype_id` classifies the live Relic
/// `matchtype_id` space into, just entered from a different source string. `None` (never guessed)
/// for anything else; today's live snapshot only ever contains these two (verified via
/// `SELECT DISTINCT ladder FROM games`), so this is a defensive, expected-unreachable arm.
fn relic_match_type_from_ladder(ladder: &str) -> Option<RelicMatchType> {
    match ladder {
        "1v1" => Some(RelicMatchType::SoloRmRanked),
        "team" => Some(RelicMatchType::TeamRmRanked),
        _ => None,
    }
}

/// Runs the `duckdb` CLI (`spawn_blocking` — this is a synchronous subprocess call, same
/// treatment the playbook gives rusqlite/CPU-bound work) to pull `played_at`/`ladder`/`rating`
/// for exactly `target_ids`, returning the match-level seed info (played_at/ladder, first row
/// wins per match_id — every player row of one match carries the same values) and the per-player
/// elo lookup (`(match_id, profile_id) -> rating`). See the module doc for why this whole
/// approach is PHASE-1-only (bounded by [`MAX_SEED_IDS`]).
async fn load_seed(
    cfg: &ImportShardsConfig,
    target_ids: &[i64],
) -> Result<(HashMap<i64, SeedInfo>, HashMap<(i64, i64), i32>)> {
    if target_ids.len() > MAX_SEED_IDS {
        bail!(
            "{} target matches exceeds the {MAX_SEED_IDS}-id literal-IN-list cap this PHASE-1-only \
             seed loader supports — see import_shards's module doc's PHASE 2 staging note",
            target_ids.len()
        );
    }
    let id_list = target_ids
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT match_id, profile_id, played_at, ladder, rating FROM games \
         WHERE source <> 'aoestats' AND match_id IN ({id_list})"
    );

    let duckdb_bin = cfg.duckdb_bin.clone();
    let duckdb_path = cfg.duckdb_path.clone();
    let output = tokio::task::spawn_blocking(move || {
        Command::new(&duckdb_bin)
            .arg(&duckdb_path)
            .arg("-readonly")
            .arg("-json")
            .arg("-c")
            .arg(&sql)
            .output()
    })
    .await
    .context("duckdb subprocess task panicked")?
    .context("failed to spawn the duckdb CLI")?;

    if !output.status.success() {
        bail!(
            "duckdb seed query failed (status {}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let rows: Vec<SeedRowRaw> = serde_json::from_slice(&output.stdout)
        .context("failed to parse duckdb -json seed output")?;

    let mut match_seed: HashMap<i64, SeedInfo> = HashMap::new();
    let mut elo_by_key: HashMap<(i64, i64), i32> = HashMap::new();
    let mut bad_timestamps = 0u64;

    for row in rows {
        if let Some(rating) = row.rating {
            if let Ok(rating) = i32::try_from(rating) {
                elo_by_key.insert((row.match_id, row.profile_id), rating);
            }
        }
        if let std::collections::hash_map::Entry::Vacant(slot) = match_seed.entry(row.match_id) {
            // `games.played_at` is a naive DuckDB `timestamp` (`"%Y-%m-%d %H:%M:%S"`, no offset)
            // — treated as UTC, matching `fetch::api`'s own `DateTime::from_timestamp(epoch, 0)`
            // construction (the crawl that populated this snapshot derives `played_at` from the
            // same Relic `completiontime` UNIX-epoch field either way).
            match NaiveDateTime::parse_from_str(&row.played_at, "%Y-%m-%d %H:%M:%S") {
                Ok(naive) => {
                    slot.insert(SeedInfo {
                        ladder: row.ladder,
                        played_at: naive.and_utc(),
                    });
                }
                Err(_) => bad_timestamps += 1,
            }
        }
    }
    if bad_timestamps > 0 {
        tracing::warn!(
            bad_timestamps,
            "some games rows had an unparsable played_at — their match's seed is treated as absent"
        );
    }

    tracing::info!(
        matches_seeded = match_seed.len(),
        elo_rows = elo_by_key.len(),
        "duckdb seed loaded"
    );
    Ok((match_seed, elo_by_key))
}

/// Why [`build_parsed_replay`] refused to build a [`ParsedReplay`] for one match — every variant
/// maps to exactly one [`ImportShardsSummary`] counter (see [`import_shards`]'s match arm), never
/// a silently-dropped generic failure.
#[derive(Debug)]
enum SkipReason {
    /// A shard row's numeric field didn't fit its target Rust type (`.0` = which field).
    BadNumeric(&'static str),
    /// An `ages` row's `age` string wasn't one of the four closed [`Age`] values.
    BadAge(String),
}

/// Rebuilds one match's [`ParsedReplay`] from its shard rows, filling each player's `elo` from
/// `elo_by_key` (see the module doc — the shards themselves carry none). Every numeric narrowing
/// is checked (`TryFrom`), never silently truncated — a real historical row is never expected to
/// overflow these (ids/timings/counts are all small in practice), so a failure here is corruption
/// worth counting and skipping, not fabricating around.
fn build_parsed_replay(
    meta: &ShardMetaRow,
    players: Vec<ShardPlayerRow>,
    events: Vec<ShardEventRow>,
    ages: Vec<ShardAgeRow>,
    elo_by_key: &HashMap<(i64, i64), i32>,
) -> std::result::Result<ParsedReplay, SkipReason> {
    let build = meta
        .build
        .map(i32::try_from)
        .transpose()
        .map_err(|_| SkipReason::BadNumeric("meta.build"))?;
    let map_id = meta
        .map_id
        .map(i32::try_from)
        .transpose()
        .map_err(|_| SkipReason::BadNumeric("meta.map_id"))?;
    let duration_ms =
        i32::try_from(meta.duration_ms).map_err(|_| SkipReason::BadNumeric("meta.duration_ms"))?;
    let n_players =
        i16::try_from(meta.n_players).map_err(|_| SkipReason::BadNumeric("meta.n_players"))?;

    let players: Vec<ReplayPlayer> = players
        .into_iter()
        .map(|p| {
            Ok(ReplayPlayer {
                player_number: i16::try_from(p.player_number)
                    .map_err(|_| SkipReason::BadNumeric("player.player_number"))?,
                profile_id: ProfileId(p.profile_id),
                civ_id: GameCivId(
                    i32::try_from(p.civ_id).map_err(|_| SkipReason::BadNumeric("player.civ_id"))?,
                ),
                name: p.name,
                team: i16::try_from(p.team).map_err(|_| SkipReason::BadNumeric("player.team"))?,
                color: i32::try_from(p.color)
                    .map_err(|_| SkipReason::BadNumeric("player.color"))?,
                won: p.won,
                // The shards predate `elo` entirely (see the module doc) — always sourced from
                // the DuckDB seed, `None` (never fabricated) when this pair has no `games` row.
                elo: elo_by_key.get(&(meta.match_id, p.profile_id)).copied(),
            })
        })
        .collect::<std::result::Result<Vec<_>, SkipReason>>()?;

    let events: Vec<ReplayEvent> = events
        .into_iter()
        .map(|e| {
            Ok(ReplayEvent {
                profile_id: e.profile_id.map(ProfileId),
                player_number: i16::try_from(e.player_number)
                    .map_err(|_| SkipReason::BadNumeric("event.player_number"))?,
                t_ms: i32::try_from(e.t_ms).map_err(|_| SkipReason::BadNumeric("event.t_ms"))?,
                kind: e.kind,
                target_id: e.target_id,
                amount: e.amount,
                detail: e.detail,
            })
        })
        .collect::<std::result::Result<Vec<_>, SkipReason>>()?;

    let ages: Vec<ReplayAge> = ages
        .into_iter()
        .map(|a| {
            Ok(ReplayAge {
                profile_id: ProfileId(a.profile_id),
                civ_id: GameCivId(
                    i32::try_from(a.civ_id).map_err(|_| SkipReason::BadNumeric("age.civ_id"))?,
                ),
                won: a.won,
                age: Age::try_from(a.age.as_str()).map_err(|_| SkipReason::BadAge(a.age.clone()))?,
                uptime_ms: i32::try_from(a.uptime_ms)
                    .map_err(|_| SkipReason::BadNumeric("age.uptime_ms"))?,
                villagers: i32::try_from(a.villagers)
                    .map_err(|_| SkipReason::BadNumeric("age.villagers"))?,
                military: i32::try_from(a.military)
                    .map_err(|_| SkipReason::BadNumeric("age.military"))?,
                n_buildings: i32::try_from(a.n_buildings)
                    .map_err(|_| SkipReason::BadNumeric("age.n_buildings"))?,
                n_research: i32::try_from(a.n_research)
                    .map_err(|_| SkipReason::BadNumeric("age.n_research"))?,
            })
        })
        .collect::<std::result::Result<Vec<_>, SkipReason>>()?;

    Ok(ParsedReplay {
        match_id: MatchId(meta.match_id),
        build,
        map_id,
        duration_ms,
        n_players,
        players,
        events,
        ages,
    })
}

fn merge_into(pending: &mut ReplayBatch, batch: ReplayBatch) {
    pending.matches.extend(batch.matches);
    pending.players.extend(batch.players);
    pending.events.extend(batch.events);
    pending.ages.extend(batch.ages);
    pending.player_units.extend(batch.player_units);
    pending.player_techs.extend(batch.player_techs);
}

/// Ingests `pending` (if non-empty) in one transaction, folding the outcome into `summary` and
/// resetting `pending`/`pending_ids` — see the module doc's "Load safety" section for the
/// slow-batch backoff and the deliberate no-retry-split-on-failure policy.
async fn flush(
    client: &mut Client,
    pending: &mut ReplayBatch,
    pending_ids: &mut Vec<i64>,
    cfg: &ImportShardsConfig,
    summary: &mut ImportShardsSummary,
) -> Result<()> {
    if pending.matches.is_empty() {
        return Ok(());
    }
    let started = Instant::now();
    let result = ingest_batch(client, pending).await;
    let elapsed = started.elapsed();
    match result {
        Ok(stats) => {
            summary.imported += stats.matches_inserted;
            summary.already_present += stats.matches_skipped;
            tracing::info!(
                matches = pending.matches.len(),
                inserted = stats.matches_inserted,
                already_present = stats.matches_skipped,
                elapsed_ms = elapsed.as_millis(),
                "batch ingested"
            );
        }
        Err(err) => {
            summary.skipped_ingest_err += pending.matches.len() as u64;
            tracing::error!(
                match_ids = ?pending_ids,
                error = %err,
                "batch ingest failed — every match in this batch counted as skipped; \
                 ingest_batch is idempotent, so simply re-running the importer retries them"
            );
        }
    }
    if elapsed > cfg.slow_batch {
        tracing::warn!(
            elapsed_ms = elapsed.as_millis(),
            pause_secs = cfg.pause.as_secs(),
            "batch was slow — pausing before the next one (load-safety backoff)"
        );
        tokio::time::sleep(cfg.pause).await;
    }
    *pending = ReplayBatch::default();
    pending_ids.clear();
    Ok(())
}

/// Runs one full `import-shards` invocation — see the module doc for the complete design.
pub async fn import_shards(
    cfg: &ImportShardsConfig,
    client: &mut Client,
) -> Result<ImportShardsSummary> {
    let mut summary = ImportShardsSummary::default();

    // 1. `meta` is small (~4.5MB compressed for the full corpus) — read it whole, in file order,
    // and take the first `limit` rows (0 = all) as this run's target match_ids.
    let meta_path = cfg.shards_dir.join("meta.ndjson.gz");
    let mut all_meta: Vec<ShardMetaRow> = Vec::new();
    let mut meta_parse_errors = 0u64;
    let meta_stats = scan_ndjson_gz(&meta_path, |line| {
        match serde_json::from_str::<ShardMetaRow>(line) {
            Ok(row) => all_meta.push(row),
            Err(_) => meta_parse_errors += 1,
        }
    })?;
    summary.meta_rows_read = all_meta.len() as u64;
    summary.meta_truncated = meta_stats.truncated;
    tracing::info!(
        rows = all_meta.len(),
        parse_errors = meta_parse_errors,
        truncated = meta_stats.truncated,
        "meta shard scanned"
    );

    let target_metas: Vec<ShardMetaRow> = if cfg.limit == 0 {
        all_meta
    } else {
        all_meta.into_iter().take(cfg.limit).collect()
    };
    summary.target_matches = target_metas.len() as u64;
    if target_metas.is_empty() {
        return Ok(summary);
    }
    let target_ids: HashSet<i64> = target_metas.iter().map(|m| m.match_id).collect();
    let target_id_vec: Vec<i64> = target_metas.iter().map(|m| m.match_id).collect();

    // 2. The seed (played_at/ladder/elo) — see `load_seed`'s doc.
    let (match_seed, elo_by_key) = load_seed(cfg, &target_id_vec).await?;

    // 3. Filter-scan the other three shards for exactly these match_ids — see the module doc's
    // "PHASE 1 vs PHASE 2 scan strategy" for why this reads each shard in full regardless.
    let (mut players_by_match, players_truncated) =
        scan_filtered::<ShardPlayerRow>(&cfg.shards_dir.join("players.ndjson.gz"), &target_ids)?;
    summary.players_truncated = players_truncated;
    let (mut ages_by_match, ages_truncated) =
        scan_filtered::<ShardAgeRow>(&cfg.shards_dir.join("ages.ndjson.gz"), &target_ids)?;
    summary.ages_truncated = ages_truncated;
    let (mut events_by_match, events_truncated) =
        scan_filtered::<ShardEventRow>(&cfg.shards_dir.join("events.ndjson.gz"), &target_ids)?;
    summary.events_truncated = events_truncated;

    // 4. Build + ingest, batched. `remove` (not `get().cloned()`) — each match_id is visited
    // exactly once (`target_metas` has no duplicates, built from `meta`'s own rows), so taking
    // ownership out of the map avoids requiring `Clone` on every shard row type.
    let mut pending = ReplayBatch::default();
    let mut pending_ids: Vec<i64> = Vec::new();

    for meta in &target_metas {
        let Some(players) = players_by_match.remove(&meta.match_id) else {
            summary.skipped_no_players += 1;
            continue;
        };
        if players.is_empty() {
            summary.skipped_no_players += 1;
            continue;
        }

        let Some(seed_info) = match_seed.get(&meta.match_id) else {
            summary.skipped_no_seed += 1;
            continue;
        };
        let Some(match_type) = relic_match_type_from_ladder(&seed_info.ladder) else {
            summary.skipped_unknown_ladder += 1;
            tracing::warn!(
                match_id = meta.match_id,
                ladder = %seed_info.ladder,
                "unrecognized games.ladder value — skipping"
            );
            continue;
        };

        let events = events_by_match.remove(&meta.match_id).unwrap_or_default();
        let ages = ages_by_match.remove(&meta.match_id).unwrap_or_default();

        let parsed = match build_parsed_replay(meta, players, events, ages, &elo_by_key) {
            Ok(parsed) => parsed,
            Err(reason) => {
                // The match arm both classifies (bumps the right summary counter) AND reads the
                // variant's payload for the log line — a real read, not `{:?}` (rustc's dead-code
                // pass doesn't credit derived-`Debug`-only reads, so `{:?}` alone would leave the
                // payload field flagged unused).
                let detail = match &reason {
                    SkipReason::BadNumeric(field) => {
                        summary.skipped_bad_numeric += 1;
                        format!("bad numeric field {field}")
                    }
                    SkipReason::BadAge(age) => {
                        summary.skipped_bad_age += 1;
                        format!("unknown age {age:?}")
                    }
                };
                tracing::warn!(
                    match_id = meta.match_id,
                    reason = %detail,
                    "skipping match — could not rebuild a valid ParsedReplay from its shard rows"
                );
                continue;
            }
        };

        let seed = DiscoverySeed {
            match_id: MatchId(meta.match_id),
            match_type,
            played_at: seed_info.played_at,
            new_rating: None,
        };

        match crate::to_batch(parsed, seed) {
            Ok(batch) => {
                pending_ids.push(meta.match_id);
                merge_into(&mut pending, batch);
            }
            Err(err) => {
                summary.skipped_to_batch_err += 1;
                tracing::warn!(match_id = meta.match_id, error = %err, "to_batch rejected this match");
            }
        }

        if pending_ids.len() >= cfg.batch_size {
            flush(client, &mut pending, &mut pending_ids, cfg, &mut summary).await?;
        }
    }
    flush(client, &mut pending, &mut pending_ids, cfg, &mut summary).await?;

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_match_id_reads_the_first_json_field_without_a_full_parse() {
        assert_eq!(
            leading_match_id(r#"{"match_id":483566418,"build":177723}"#),
            Some(483566418)
        );
        assert_eq!(
            leading_match_id(r#"{"match_id":1,"profile_id":null}"#),
            Some(1)
        );
        assert_eq!(leading_match_id(r#"{"not_match_id":1}"#), None);
        assert_eq!(leading_match_id(""), None);
        assert_eq!(
            leading_match_id(r#"{"match_id":not_a_number,"x":1}"#),
            None,
            "a non-numeric leading value must not panic — just fail to classify"
        );
    }

    #[test]
    fn relic_match_type_from_ladder_maps_the_two_known_strings_and_fails_loud_otherwise() {
        assert_eq!(
            relic_match_type_from_ladder("1v1"),
            Some(RelicMatchType::SoloRmRanked)
        );
        assert_eq!(
            relic_match_type_from_ladder("team"),
            Some(RelicMatchType::TeamRmRanked)
        );
        assert_eq!(
            relic_match_type_from_ladder("ffa"),
            None,
            "an unrecognized ladder string must never guess — see the module doc"
        );
        assert_eq!(relic_match_type_from_ladder(""), None);
    }

    fn sample_meta() -> ShardMetaRow {
        ShardMetaRow {
            match_id: 1001,
            build: Some(177_723),
            map_id: Some(9),
            duration_ms: 1_800_000,
            n_players: 2,
        }
    }

    fn sample_player(profile_id: i64, player_number: i64) -> ShardPlayerRow {
        ShardPlayerRow {
            player_number,
            profile_id,
            civ_id: 1,
            name: "Player".to_owned(),
            team: 1,
            color: 1,
            won: Some(true),
        }
    }

    #[test]
    fn build_parsed_replay_fills_elo_from_the_seed_map_never_leaves_a_stale_shard_value() {
        let meta = sample_meta();
        let players = vec![sample_player(5001, 1), sample_player(5002, 2)];
        let mut elo = HashMap::new();
        elo.insert((1001, 5001), 1650);
        // 5002 deliberately has no seed row -> must stay `None`, never fabricated.

        let parsed = build_parsed_replay(&meta, players, vec![], vec![], &elo)
            .expect("valid shard rows must build a ParsedReplay");

        assert_eq!(parsed.match_id, MatchId(1001));
        assert_eq!(parsed.map_id, Some(9));
        assert_eq!(parsed.players.len(), 2);
        let p1 = parsed
            .players
            .iter()
            .find(|p| p.profile_id == ProfileId(5001))
            .expect("profile 5001 must be present");
        assert_eq!(p1.elo, Some(1650), "elo comes from the seed map, not the shard");
        let p2 = parsed
            .players
            .iter()
            .find(|p| p.profile_id == ProfileId(5002))
            .expect("profile 5002 must be present");
        assert_eq!(
            p2.elo, None,
            "no games row for this (match_id, profile_id) -> honest None, never guessed"
        );
    }

    #[test]
    fn build_parsed_replay_converts_a_known_age_and_fails_loud_on_an_unknown_one() {
        let meta = sample_meta();
        let good_age = ShardAgeRow {
            profile_id: 5001,
            civ_id: 1,
            won: Some(true),
            age: "feudal".to_owned(),
            uptime_ms: 60_000,
            villagers: 20,
            military: 0,
            n_buildings: 5,
            n_research: 2,
        };
        let parsed = build_parsed_replay(
            &meta,
            vec![sample_player(5001, 1)],
            vec![],
            vec![good_age],
            &HashMap::new(),
        )
        .expect("valid shard rows must build a ParsedReplay");
        assert_eq!(parsed.ages.len(), 1);
        assert_eq!(parsed.ages[0].age, Age::Feudal);
        assert_eq!(parsed.ages[0].villagers, 20);

        let bad_age = ShardAgeRow {
            profile_id: 5001,
            civ_id: 1,
            won: Some(true),
            age: "fuedal".to_owned(),
            uptime_ms: 60_000,
            villagers: 20,
            military: 0,
            n_buildings: 5,
            n_research: 2,
        };
        let err = build_parsed_replay(
            &meta,
            vec![sample_player(5001, 1)],
            vec![],
            vec![bad_age],
            &HashMap::new(),
        )
        .unwrap_err();
        assert!(
            matches!(err, SkipReason::BadAge(ref s) if s == "fuedal"),
            "an unknown age label must be a fail-loud BadAge skip, never silently dropped/coerced"
        );
    }

    #[test]
    fn build_parsed_replay_fails_loud_on_a_numeric_field_that_does_not_fit() {
        let mut meta = sample_meta();
        // `n_players` (i16) can't hold this — a corrupt/impossible row, never silently truncated.
        meta.n_players = i64::from(i16::MAX) + 1;
        let err = build_parsed_replay(&meta, vec![sample_player(5001, 1)], vec![], vec![], &HashMap::new())
            .unwrap_err();
        assert!(matches!(err, SkipReason::BadNumeric("meta.n_players")));
    }
}
