//! `pipeline import-shards` — migrates the OLD parsed-replay shard corpus
//! (`data-cache/replays/shards/{meta,players,events,ages}.ndjson.gz`, produced by the retired
//! `scripts/data-pipeline/replay-rs` extractor) into the live Postgres pipeline as
//! `matches.source = 'replay'`, re-deriving the FULL enrichment (opening_kind/apm/timings/
//! units/techs) through the SAME [`crate::to_batch`] + `ingest::ingest_batch` path a live crawl
//! uses — see the crate-level doc's "composition root" note. This is the ONE place a historical
//! shard row + the DuckDB `games` crawl-seed compose into a [`replay::ParsedReplay`] +
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
//!   (`source <> 'aoestats'`, the same ~1.57M replay matches) carries `rating` — the Relic API's
//!   OWN post-game elo for that player, captured at crawl time, keyed by `(match_id, profile_id)`.
//!   The staged `seed` table fills [`replay::ReplayPlayer::elo`] from it — `None` (never
//!   fabricated) when the pair has no `games` row.
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
//! [`decompress_shard`] treats that error as "stop, keep everything decoded so far" (mirrors
//! `zcat ... 2>/dev/null`) rather than failing the whole shard.
//!
//! ## Scaling strategy — DuckDB staging + range-scan chunks (the whole-corpus design)
//! The four shards are NOT sorted or co-indexed by `match_id` (`store.rs`'s rayon workers append
//! whichever match finishes first), so a match's four tables are scattered across each file. At
//! ~194k matches / ~1B `events` rows, holding every target match's rows in RAM (the old
//! filter-while-scanning approach) is impossible. Instead [`build_staging`]:
//!  1. Decompresses each shard ONCE (tolerantly) to a plain `.ndjson` file (`meta`/`players`/
//!     `ages` are small; `events` is ~110 GB uncompressed — the VM has the headroom, checked).
//!  2. Loads each into a DuckDB staging database (`read_json` with EXPLICIT column types — no
//!     inference surprises on all-null columns), plus a `seed` table copied from the read-only
//!     `games` snapshot (`source <> 'aoestats'`, the games-JOIN that REPLACES the old
//!     5000-id-capped literal-`IN` seed query — no id cap anymore).
//!
//! Then [`import_shards`] pulls the sorted DISTINCT `match_id`s from the staged `meta` (small —
//! ~194k ints) and iterates them in contiguous CHUNKS. Per chunk it `COPY`s each staged table's
//! rows for that chunk's `match_id` RANGE (`BETWEEN lo AND hi`) out to small `.ndjson` files,
//! groups them in RAM (bounded to one chunk — a few GB), reconstructs each match, and ingests in
//! modest sub-batches. The staged `events` table is unsorted, so each chunk's range `COPY` scans
//! its `match_id` column — but that's ~97 cheap column scans total (chunk size ~2000), not the
//! 388 full-file decompress passes the naive "re-scan per chunk" alternative would cost.
//!
//! ## Resumability
//! `ingest::ingest_batch` is idempotent (`ON CONFLICT (match_id) DO NOTHING`, children gated on
//! genuinely-new match ids) — re-running is always safe. On top of that, staging is skipped when
//! its `.done` markers exist (`--rebuild-staging` forces a rebuild), and a cursor file records how
//! many distinct matches have been fully processed so a restart resumes at the next unfinished
//! chunk instead of re-COPYing completed ones. A crash mid-chunk simply re-does that one chunk
//! (idempotency absorbs the partial).
//!
//! ## Load safety
//! [`ImportShardsConfig::batch_size`] bounds how many matches share one `ingest_batch`
//! transaction (never one unbatched multi-million-row write — the DB wedged once on that shape).
//! Each batch's wall-clock `ingest_batch` time is logged; a batch slower than
//! [`ImportShardsConfig::slow_batch`] triggers a [`ImportShardsConfig::pause`] sleep before the
//! next one, as a self-contained backoff signal that the DB may be under load. This binary has no
//! standing credential to read the Postgres host's own OS load, so the operator cross-checks CT101
//! `/proc/loadavg` between chunks during a run (the per-chunk progress log makes that easy).
//!
//! A batch that fails outright (e.g. a genuine FK violation) is NOT split and retried
//! match-by-match — the whole batch's matches are counted
//! [`ImportShardsSummary::skipped_ingest_err`] and logged with their ids; since `ingest_batch` is
//! idempotent, re-running the importer retries them (after a smaller `--batch-size` if isolating
//! a bad match is needed).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
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

/// Tunables for one `import-shards` run.
pub struct ImportShardsConfig {
    /// Directory containing `{meta,players,events,ages}.ndjson.gz`.
    pub shards_dir: PathBuf,
    /// Path to the read-only DuckDB snapshot carrying `games` (the crawl seed).
    pub snapshot_path: PathBuf,
    /// Path to the `duckdb` CLI binary (not always on `PATH` — e.g. `~/bin/duckdb`).
    pub duckdb_bin: PathBuf,
    /// Scratch directory for the decompressed shards, the staging DuckDB, the per-chunk temp
    /// files, and the resume cursor.
    pub work_dir: PathBuf,
    /// Stop after this many distinct `match_id`s (sorted), 0 = all readable matches.
    pub limit: usize,
    /// Distinct matches per DuckDB range `COPY` round-trip (bounds per-chunk RAM).
    pub chunk_size: usize,
    /// Matches per `ingest_batch` transaction.
    pub batch_size: usize,
    /// A batch taking longer than this is treated as a load-safety signal (see the module doc).
    pub slow_batch: Duration,
    /// How long to pause after a slow batch.
    pub pause: Duration,
    /// Force a full staging rebuild (decompress + reload) even if the `.done` markers exist.
    pub rebuild_staging: bool,
}

/// Outcome counts for one `import-shards` run — see the module doc for what each skip reason
/// means and why nothing here is ever fabricated to paper over a gap.
#[derive(Debug, Default, Clone)]
pub struct ImportShardsSummary {
    pub distinct_matches: u64,
    pub processed_matches: u64,
    pub chunks: u64,
    /// Genuinely new `matches` rows written (summed `IngestStats::matches_inserted`).
    pub imported: u64,
    /// Matches already present before this run (idempotent re-ingest / resume — see the module doc).
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
    /// `match_player_units` rows dropped because their `unit_id` isn't in the live `units` dim
    /// (would FK-violate) — see [`filter_to_known_dims`]. The match + its KNOWN units still import.
    pub dropped_unit_rows: u64,
    /// `match_player_techs` rows dropped because their `tech_id` isn't in the live `techs` dim.
    pub dropped_tech_rows: u64,
    /// The DISTINCT out-of-dim `unit_id`s seen (reported so the operator can decide whether to
    /// extend the `units` dim — see [`filter_to_known_dims`]'s doc). Never fabricated into a row.
    pub unknown_unit_ids: BTreeSet<i32>,
    /// The DISTINCT out-of-dim `tech_id`s seen (expected empty — `replay::derive` only emits the
    /// fixed watched-tech set, all of which are standard dim entries).
    pub unknown_tech_ids: BTreeSet<i32>,
}

// --- shard row shapes. `meta` is read as a small whole-file Vec (needs its own `match_id`
// field); the other three are grouped by the `match_id` [`leading_match_id`] extracts from each
// line's first key, so they deliberately DON'T re-declare a `match_id` field — serde ignores the
// JSON `match_id` key with no matching struct field (no `deny_unknown_fields` in this crate). ---

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

/// One `seed` (staged `games`) row, as DuckDB's `COPY ... (FORMAT JSON)` renders it.
#[derive(Debug, Deserialize)]
struct SeedRowRaw {
    match_id: i64,
    profile_id: i64,
    played_at: String,
    ladder: String,
    rating: Option<i64>,
}

/// A bare distinct-id row from the DuckDB distinct-ids query.
#[derive(Debug, Deserialize)]
struct DistinctIdRow {
    match_id: i64,
}

/// Per-table loaded row counts from the post-load staging census (see [`build_staging`]).
#[derive(Debug, Deserialize)]
struct TableCount {
    meta: i64,
    players: i64,
    events: i64,
    ages: i64,
    seed: i64,
}

/// The match-level facts one match needs from its seed rows (played_at/ladder), distinct from the
/// per-player elo lookup built alongside it.
struct SeedInfo {
    ladder: String,
    played_at: DateTime<Utc>,
}

/// One chunk's indexed seed: the per-player elo lookup, the per-match played_at/ladder, and a
/// count of rows whose `played_at` couldn't be parsed (their match gets no seed entry).
#[derive(Default)]
struct SeedIndex {
    elo_by_key: HashMap<(i64, i64), i32>,
    match_seed: HashMap<i64, SeedInfo>,
    bad_timestamps: u64,
}

// --- one shard file's decompression -----------------------------------------------------------

#[derive(Debug, Default, Clone, Copy)]
struct DecompressStats {
    lines: u64,
    truncated: bool,
}

/// Streams `src` (a multi-member gzip shard, tolerant of a corrupt trailing member — see the
/// module doc) into `dst` as one complete NDJSON line per successfully decoded line. A decode
/// error mid-stream stops the read and is logged, never propagated — that is the intended,
/// accepted behavior for this corpus. Writes a `<dst>.done` marker on success so a re-run skips
/// the (slow, for `events`) decompression.
fn decompress_shard(src: &Path, dst: &Path, rebuild: bool) -> Result<DecompressStats> {
    let marker = done_marker(dst);
    if !rebuild && marker.exists() {
        let lines = std::fs::read_to_string(&marker)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        tracing::info!(dst = %dst.display(), lines, "shard already decompressed — skipping");
        return Ok(DecompressStats { lines, truncated: false });
    }

    let file =
        std::fs::File::open(src).with_context(|| format!("failed to open {}", src.display()))?;
    let mut reader = BufReader::new(MultiGzDecoder::new(file));
    let out = std::fs::File::create(dst)
        .with_context(|| format!("failed to create {}", dst.display()))?;
    let mut writer = std::io::BufWriter::new(out);

    let mut stats = DecompressStats::default();
    let mut buf = String::new();
    loop {
        buf.clear();
        match reader.read_line(&mut buf) {
            Ok(0) => break,
            Ok(_) => {
                let line = buf.trim_end_matches(['\n', '\r']);
                if !line.is_empty() {
                    writer
                        .write_all(line.as_bytes())
                        .and_then(|()| writer.write_all(b"\n"))
                        .with_context(|| format!("failed writing to {}", dst.display()))?;
                    stats.lines += 1;
                }
            }
            Err(err) => {
                tracing::warn!(
                    src = %src.display(),
                    lines = stats.lines,
                    error = %err,
                    "gzip stream ended early (tolerant read) — keeping the valid prefix"
                );
                stats.truncated = true;
                break;
            }
        }
    }
    writer
        .flush()
        .with_context(|| format!("failed to flush {}", dst.display()))?;
    std::fs::write(&marker, stats.lines.to_string())
        .with_context(|| format!("failed to write marker {}", marker.display()))?;
    tracing::info!(
        dst = %dst.display(),
        lines = stats.lines,
        truncated = stats.truncated,
        "shard decompressed"
    );
    Ok(stats)
}

fn done_marker(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".done");
    PathBuf::from(s)
}

// --- DuckDB helpers ----------------------------------------------------------------------------

/// Runs a DuckDB SQL script (written to a temp file, `duckdb <db> -f <script>`) that produces no
/// stdout — DDL, `COPY ... TO`, `ATTACH`. `spawn_blocking` since the CLI call is synchronous.
async fn run_duckdb_script(cfg: &ImportShardsConfig, db: &Path, sql: String) -> Result<()> {
    let script = cfg.work_dir.join("_script.sql");
    std::fs::write(&script, &sql)
        .with_context(|| format!("failed to write duckdb script {}", script.display()))?;
    let bin = cfg.duckdb_bin.clone();
    let db = db.to_path_buf();
    let script2 = script.clone();
    let output = tokio::task::spawn_blocking(move || {
        Command::new(&bin).arg(&db).arg("-f").arg(&script2).output()
    })
    .await
    .context("duckdb subprocess task panicked")?
    .context("failed to spawn the duckdb CLI")?;
    if !output.status.success() {
        bail!(
            "duckdb script failed (status {}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// Runs a DuckDB query with `-json` and deserializes the JSON-array stdout into `Vec<T>` — for
/// the small distinct-ids result only (never a bulk data path; those go through file `COPY`).
async fn query_duckdb_json<T>(cfg: &ImportShardsConfig, db: &Path, sql: String) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de>,
{
    let bin = cfg.duckdb_bin.clone();
    let db = db.to_path_buf();
    let output = tokio::task::spawn_blocking(move || {
        Command::new(&bin)
            .arg(&db)
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
            "duckdb query failed (status {}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout).context("failed to parse duckdb -json output")
}

/// The staging DuckDB path.
fn staging_db(cfg: &ImportShardsConfig) -> PathBuf {
    cfg.work_dir.join("staging.duckdb")
}

/// PRAGMAs prefixed to every load/COPY script: a memory ceiling well under the VM's 48 GB (so a
/// 1B-row load/scan can't OOM the box) and a temp dir on the roomy scratch volume for any spill.
fn duckdb_pragmas(cfg: &ImportShardsConfig) -> String {
    let tmp = cfg.work_dir.join("duckdb_tmp");
    format!(
        "PRAGMA memory_limit='24GB';\nPRAGMA temp_directory='{}';\n",
        tmp.display()
    )
}

/// Decompresses all four shards, then loads them (plus the `games` seed) into the staging DuckDB.
/// Idempotent: skips entirely when `staging.duckdb.done` exists unless
/// [`ImportShardsConfig::rebuild_staging`].
async fn build_staging(cfg: &ImportShardsConfig) -> Result<()> {
    std::fs::create_dir_all(&cfg.work_dir)
        .with_context(|| format!("failed to create work dir {}", cfg.work_dir.display()))?;
    std::fs::create_dir_all(cfg.work_dir.join("duckdb_tmp")).ok();

    let db = staging_db(cfg);
    let staging_marker = done_marker(&db);
    if !cfg.rebuild_staging && staging_marker.exists() {
        tracing::info!(db = %db.display(), "staging DuckDB already built — skipping (use --rebuild-staging to force)");
        return Ok(());
    }
    // Reaching here means we ARE (re)building — remove any stale/partial staging DB (e.g. one left
    // by a load that failed before writing its `.done` marker) so `CREATE OR REPLACE` starts from
    // a clean file. The (slow) decompressed `.ndjson` shards have their OWN markers and are NOT
    // removed here — a failed DuckDB load never needs re-decompressing.
    std::fs::remove_file(&db).ok();
    std::fs::remove_file(&staging_marker).ok();

    // 1. Decompress each shard (tolerant). `events` is the slow one (~110 GB out); markers make a
    // re-run skip it.
    let meta_nd = cfg.work_dir.join("meta.ndjson");
    let players_nd = cfg.work_dir.join("players.ndjson");
    let events_nd = cfg.work_dir.join("events.ndjson");
    let ages_nd = cfg.work_dir.join("ages.ndjson");
    let meta_lines = decompress_shard(&cfg.shards_dir.join("meta.ndjson.gz"), &meta_nd, cfg.rebuild_staging)?.lines;
    let players_lines = decompress_shard(&cfg.shards_dir.join("players.ndjson.gz"), &players_nd, cfg.rebuild_staging)?.lines;
    let ages_lines = decompress_shard(&cfg.shards_dir.join("ages.ndjson.gz"), &ages_nd, cfg.rebuild_staging)?.lines;
    let events_lines = decompress_shard(&cfg.shards_dir.join("events.ndjson.gz"), &events_nd, cfg.rebuild_staging)?.lines;

    // 2. Load into DuckDB with EXPLICIT column types (no inference surprises on all-null columns).
    // `ignore_errors=true` tolerates the malformed JSON line(s) the gzip truncation boundary left
    // behind (the JSON-layer analog of `decompress_shard`'s gzip-layer tolerance — see the module
    // doc): DuckDB emits an all-NULL row for such a line instead of aborting the whole load. The
    // `WHERE match_id IS NOT NULL` then drops those all-NULL rows — a real shard row ALWAYS has a
    // non-null `match_id` (it's the extractor's key), so this filter removes exactly the corrupt
    // lines and nothing legitimate. The `seed` table comes from the clean `games` snapshot and
    // needs neither.
    let sql = format!(
        "{pragmas}\
ATTACH '{snapshot}' AS snap (READ_ONLY);\n\
CREATE OR REPLACE TABLE meta AS SELECT * FROM read_json('{meta}', format='newline_delimited', ignore_errors=true, columns={{match_id:'BIGINT', build:'BIGINT', map_id:'BIGINT', duration_ms:'BIGINT', n_players:'BIGINT'}}) WHERE match_id IS NOT NULL;\n\
CREATE OR REPLACE TABLE players AS SELECT * FROM read_json('{players}', format='newline_delimited', ignore_errors=true, columns={{match_id:'BIGINT', player_number:'BIGINT', profile_id:'BIGINT', civ_id:'BIGINT', name:'VARCHAR', team:'BIGINT', color:'BIGINT', won:'BOOLEAN'}}) WHERE match_id IS NOT NULL;\n\
CREATE OR REPLACE TABLE ages AS SELECT * FROM read_json('{ages}', format='newline_delimited', ignore_errors=true, columns={{match_id:'BIGINT', profile_id:'BIGINT', civ_id:'BIGINT', won:'BOOLEAN', age:'VARCHAR', uptime_ms:'BIGINT', villagers:'BIGINT', military:'BIGINT', n_buildings:'BIGINT', n_research:'BIGINT'}}) WHERE match_id IS NOT NULL;\n\
CREATE OR REPLACE TABLE events AS SELECT * FROM read_json('{events}', format='newline_delimited', ignore_errors=true, columns={{match_id:'BIGINT', profile_id:'BIGINT', player_number:'BIGINT', t_ms:'BIGINT', kind:'VARCHAR', target_id:'BIGINT', amount:'BIGINT', detail:'VARCHAR'}}) WHERE match_id IS NOT NULL;\n\
CREATE OR REPLACE TABLE seed AS SELECT match_id, profile_id, played_at, ladder, rating FROM snap.games WHERE source <> 'aoestats';\n\
DETACH snap;\n",
        pragmas = duckdb_pragmas(cfg),
        snapshot = cfg.snapshot_path.display(),
        meta = meta_nd.display(),
        players = players_nd.display(),
        ages = ages_nd.display(),
        events = events_nd.display(),
    );
    tracing::info!("loading staging DuckDB tables (this is the slow, one-time step for `events`)");
    let started = Instant::now();
    run_duckdb_script(cfg, &db, sql).await?;
    tracing::info!(elapsed_s = started.elapsed().as_secs(), "staging DuckDB loaded");

    // Loaded-vs-decompressed row counts, so a big skip (a red flag) is visible while a few
    // corrupt-boundary lines (expected) are not — see the module doc's "Tolerant" sections.
    let counts = query_duckdb_json::<TableCount>(
        cfg,
        &db,
        "SELECT (SELECT count(*) FROM meta) AS meta, (SELECT count(*) FROM players) AS players, (SELECT count(*) FROM events) AS events, (SELECT count(*) FROM ages) AS ages, (SELECT count(*) FROM seed) AS seed".to_owned(),
    )
    .await?;
    if let Some(c) = counts.first() {
        tracing::info!(
            meta_loaded = c.meta, meta_lines,
            players_loaded = c.players, players_lines,
            events_loaded = c.events, events_lines,
            ages_loaded = c.ages, ages_lines,
            seed_loaded = c.seed,
            "staging row counts (loaded vs decompressed lines — a large gap would flag corruption)"
        );
    }
    std::fs::write(&staging_marker, "ok")
        .with_context(|| format!("failed to write marker {}", staging_marker.display()))?;
    Ok(())
}

/// One chunk's five range `COPY`s → `.ndjson` files under the work dir, filtered to
/// `match_id BETWEEN lo AND hi`. Columns are listed with `match_id` FIRST so [`leading_match_id`]
/// can group the players/events/ages files without a full parse.
async fn copy_chunk(cfg: &ImportShardsConfig, lo: i64, hi: i64) -> Result<()> {
    let db = staging_db(cfg);
    let p = |name: &str| cfg.work_dir.join(format!("chunk_{name}.ndjson"));
    let sql = format!(
        "{pragmas}\
COPY (SELECT match_id, build, map_id, duration_ms, n_players FROM meta WHERE match_id BETWEEN {lo} AND {hi}) TO '{meta}' (FORMAT JSON);\n\
COPY (SELECT match_id, player_number, profile_id, civ_id, name, team, color, won FROM players WHERE match_id BETWEEN {lo} AND {hi}) TO '{players}' (FORMAT JSON);\n\
COPY (SELECT match_id, profile_id, player_number, t_ms, kind, target_id, amount, detail FROM events WHERE match_id BETWEEN {lo} AND {hi}) TO '{events}' (FORMAT JSON);\n\
COPY (SELECT match_id, profile_id, civ_id, won, age, uptime_ms, villagers, military, n_buildings, n_research FROM ages WHERE match_id BETWEEN {lo} AND {hi}) TO '{ages}' (FORMAT JSON);\n\
COPY (SELECT match_id, profile_id, played_at, ladder, rating FROM seed WHERE match_id BETWEEN {lo} AND {hi}) TO '{seed}' (FORMAT JSON);\n",
        pragmas = duckdb_pragmas(cfg),
        meta = p("meta").display(),
        players = p("players").display(),
        events = p("events").display(),
        ages = p("ages").display(),
        seed = p("seed").display(),
    );
    run_duckdb_script(cfg, &db, sql).await
}

/// Leading `"match_id":<int>` extracted without a full JSON parse — every chunk `COPY` lists
/// `match_id` first (see [`copy_chunk`]), so this cheap prefix read is the group key for the
/// players/events/ages files.
fn leading_match_id(line: &str) -> Option<i64> {
    let rest = line.strip_prefix("{\"match_id\":")?;
    let end = rest.find(['}', ','])?;
    rest.get(..end)?.parse().ok()
}

/// Reads a chunk `.ndjson` file (plain, not gzipped), deserializing each line as `T` and grouping
/// by the `match_id` [`leading_match_id`] extracts. A missing file (DuckDB writes none for an
/// empty result) is an empty map, never an error.
fn read_chunk_grouped<T>(path: &Path) -> Result<HashMap<i64, Vec<T>>>
where
    T: for<'de> Deserialize<'de>,
{
    let mut out: HashMap<i64, Vec<T>> = HashMap::new();
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(err) => return Err(err).with_context(|| format!("failed to open {}", path.display())),
    };
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line.with_context(|| format!("failed reading {}", path.display()))?;
        if line.is_empty() {
            continue;
        }
        let Some(mid) = leading_match_id(&line) else {
            continue;
        };
        let row: T = serde_json::from_str(&line)
            .with_context(|| format!("failed to parse a row of {}", path.display()))?;
        out.entry(mid).or_default().push(row);
    }
    Ok(out)
}

/// Reads a chunk `.ndjson` file as a flat `Vec<T>` (for `meta`/`seed` — no grouping needed).
fn read_chunk_vec<T>(path: &Path) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de>,
{
    let mut out = Vec::new();
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(err) => return Err(err).with_context(|| format!("failed to open {}", path.display())),
    };
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line.with_context(|| format!("failed reading {}", path.display()))?;
        if line.is_empty() {
            continue;
        }
        out.push(
            serde_json::from_str(&line)
                .with_context(|| format!("failed to parse a row of {}", path.display()))?,
        );
    }
    Ok(out)
}

// --- seed / ladder / reconstruction (shared with the unit tests) -------------------------------

/// Maps the DuckDB `games.ladder` string (already reduced to `"1v1"`/`"team"` by the crawl that
/// built the snapshot) directly onto the two constructable [`RelicMatchType`] variants — the SAME
/// two-value vocabulary `RelicMatchType::from_matchtype_id` classifies the live Relic
/// `matchtype_id` space into, entered from a different source string. `None` (never guessed) for
/// anything else; today's snapshot only ever contains these two (`SELECT DISTINCT ladder`), so
/// this is a defensive, expected-unreachable arm.
fn relic_match_type_from_ladder(ladder: &str) -> Option<RelicMatchType> {
    match ladder {
        "1v1" => Some(RelicMatchType::SoloRmRanked),
        "team" => Some(RelicMatchType::TeamRmRanked),
        _ => None,
    }
}

/// Builds the [`SeedIndex`] (per-player elo lookup + per-match played_at/ladder, first row wins)
/// from one chunk's `seed` rows.
fn index_seed(rows: Vec<SeedRowRaw>) -> SeedIndex {
    let mut idx = SeedIndex::default();
    for row in rows {
        if let Some(rating) = row.rating {
            if let Ok(rating) = i32::try_from(rating) {
                idx.elo_by_key.insert((row.match_id, row.profile_id), rating);
            }
        }
        if let std::collections::hash_map::Entry::Vacant(slot) = idx.match_seed.entry(row.match_id)
        {
            // `games.played_at` is a naive DuckDB `timestamp` ("%Y-%m-%d %H:%M:%S", no offset) —
            // treated as UTC, matching `fetch::api`'s own `DateTime::from_timestamp(epoch, 0)`
            // construction (the crawl derives both from the same Relic `completiontime`).
            match NaiveDateTime::parse_from_str(&row.played_at, "%Y-%m-%d %H:%M:%S") {
                Ok(naive) => {
                    slot.insert(SeedInfo {
                        ladder: row.ladder,
                        played_at: naive.and_utc(),
                    });
                }
                Err(_) => idx.bad_timestamps += 1,
            }
        }
    }
    idx
}

/// Why [`build_parsed_replay`] refused to build a [`ParsedReplay`] for one match — each variant
/// maps to exactly one [`ImportShardsSummary`] counter, never a silently-dropped generic failure.
#[derive(Debug)]
enum SkipReason {
    BadNumeric(&'static str),
    BadAge(String),
}

/// Rebuilds one match's [`ParsedReplay`] from its shard rows, filling each player's `elo` from
/// `elo_by_key` (the shards themselves carry none). Every numeric narrowing is checked
/// (`TryFrom`), never silently truncated — a real historical row never overflows these, so a
/// failure here is corruption worth counting and skipping, not fabricating around.
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

/// The live `units`/`techs` dimension id sets, loaded once from Postgres so [`filter_to_known_dims`]
/// can drop FK-unsafe child rows without a per-row round trip.
struct DimIds {
    units: HashSet<i32>,
    techs: HashSet<i32>,
}

/// Loads the `units`/`techs` dim id sets from the live schema (tiny — ~238 units, ~192 techs).
async fn load_dim_ids(client: &Client) -> Result<DimIds> {
    let units = client
        .query("SELECT unit_id FROM units", &[])
        .await
        .context("failed to load units dim ids")?
        .into_iter()
        .map(|r| r.get::<_, i32>(0))
        .collect();
    let techs = client
        .query("SELECT tech_id FROM techs", &[])
        .await
        .context("failed to load techs dim ids")?
        .into_iter()
        .map(|r| r.get::<_, i32>(0))
        .collect();
    Ok(DimIds { units, techs })
}

/// Drops `player_units`/`player_techs` rows whose id isn't in the live dim, recording the counts +
/// the DISTINCT unknown ids on `summary`. `match_player_units.unit_id` FK-references `units`, so a
/// `train` of a unit the dim doesn't list (verified real in this corpus: game unit ids **37** and
/// **1570**, absent from the 238-unit `aoe2techtree` dim) would abort the whole batch's
/// transaction — taking down every good match batched with it. Rather than fail those matches over
/// two unattributable unit ids, this drops ONLY the offending child rows (the match itself, its
/// KNOWN units, and all other enrichment still import) and reports the ids so the operator can
/// decide whether to extend the dim. This is the FK-safe analog of the "skip + count, never
/// fabricate" rule applied at row granularity — NOT a silent drop (every dropped id is surfaced in
/// [`ImportShardsSummary`]). Techs are filtered too for symmetry, though `replay::derive` only ever
/// emits the fixed watched-tech set (all standard dim entries), so `unknown_tech_ids` is expected
/// to stay empty.
fn filter_to_known_dims(batch: &mut ReplayBatch, dims: &DimIds, summary: &mut ImportShardsSummary) {
    let before_u = batch.player_units.len();
    batch.player_units.retain(|u| {
        let known = dims.units.contains(&u.unit_id.0);
        if !known {
            summary.unknown_unit_ids.insert(u.unit_id.0);
        }
        known
    });
    summary.dropped_unit_rows += (before_u - batch.player_units.len()) as u64;

    let before_t = batch.player_techs.len();
    batch.player_techs.retain(|t| {
        let known = dims.techs.contains(&t.tech_id.0);
        if !known {
            summary.unknown_tech_ids.insert(t.tech_id.0);
        }
        known
    });
    summary.dropped_tech_rows += (before_t - batch.player_techs.len()) as u64;
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
/// resetting `pending`/`pending_ids`. See the module doc's "Load safety" section for the
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
                events = pending.events.len(),
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

// --- resume cursor -----------------------------------------------------------------------------

fn cursor_path(cfg: &ImportShardsConfig) -> PathBuf {
    cfg.work_dir.join("cursor.txt")
}

/// Number of distinct matches already fully processed by a previous run (0 if none / unreadable).
fn read_cursor(cfg: &ImportShardsConfig) -> usize {
    std::fs::read_to_string(cursor_path(cfg))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn write_cursor(cfg: &ImportShardsConfig, processed: usize) {
    if let Err(err) = std::fs::write(cursor_path(cfg), processed.to_string()) {
        tracing::warn!(error = %err, "failed to persist resume cursor — restart re-does this chunk (idempotent)");
    }
}

// --- orchestration -----------------------------------------------------------------------------

/// Runs one full `import-shards` invocation — see the module doc for the complete design.
pub async fn import_shards(
    cfg: &ImportShardsConfig,
    client: &mut Client,
) -> Result<ImportShardsSummary> {
    let mut summary = ImportShardsSummary::default();

    build_staging(cfg).await?;

    let db = staging_db(cfg);
    // Small: ~194k ints. Sorted so chunks are contiguous `match_id` ranges (`BETWEEN lo AND hi`
    // then captures exactly a chunk's ids — the staged tables need no index/sort of their own).
    let ids: Vec<i64> = query_duckdb_json::<DistinctIdRow>(
        cfg,
        &db,
        "SELECT DISTINCT match_id FROM meta ORDER BY match_id".to_owned(),
    )
    .await?
    .into_iter()
    .map(|r| r.match_id)
    .collect();
    let ids: Vec<i64> = if cfg.limit == 0 {
        ids
    } else {
        ids.into_iter().take(cfg.limit).collect()
    };
    summary.distinct_matches = ids.len() as u64;
    if ids.is_empty() {
        return Ok(summary);
    }

    // Load the units/techs dim id sets once — `filter_to_known_dims` uses them to drop FK-unsafe
    // child rows (see its doc) instead of letting one out-of-dim unit_id abort a whole batch.
    let dims = load_dim_ids(client).await?;

    let start = read_cursor(cfg).min(ids.len());
    if start > 0 {
        tracing::info!(resume_from = start, total = ids.len(), "resuming from cursor");
    }

    let chunk_size = cfg.chunk_size.max(1);
    let mut processed = start;
    for chunk in ids[start..].chunks(chunk_size) {
        let lo = chunk[0];
        let hi = chunk[chunk.len() - 1];
        let chunk_started = Instant::now();

        copy_chunk(cfg, lo, hi).await?;

        let meta_rows: Vec<ShardMetaRow> = read_chunk_vec(&cfg.work_dir.join("chunk_meta.ndjson"))?;
        let mut players_by_match: HashMap<i64, Vec<ShardPlayerRow>> =
            read_chunk_grouped(&cfg.work_dir.join("chunk_players.ndjson"))?;
        let mut events_by_match: HashMap<i64, Vec<ShardEventRow>> =
            read_chunk_grouped(&cfg.work_dir.join("chunk_events.ndjson"))?;
        let mut ages_by_match: HashMap<i64, Vec<ShardAgeRow>> =
            read_chunk_grouped(&cfg.work_dir.join("chunk_ages.ndjson"))?;
        let seed_rows: Vec<SeedRowRaw> = read_chunk_vec(&cfg.work_dir.join("chunk_seed.ndjson"))?;
        let SeedIndex {
            elo_by_key,
            match_seed,
            bad_timestamps,
        } = index_seed(seed_rows);
        if bad_timestamps > 0 {
            tracing::warn!(bad_timestamps, "some games rows had an unparsable played_at — treated as absent seed");
        }

        let mut pending = ReplayBatch::default();
        let mut pending_ids: Vec<i64> = Vec::new();

        for meta in &meta_rows {
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
                tracing::warn!(match_id = meta.match_id, ladder = %seed_info.ladder, "unrecognized games.ladder — skipping");
                continue;
            };
            let events = events_by_match.remove(&meta.match_id).unwrap_or_default();
            let ages = ages_by_match.remove(&meta.match_id).unwrap_or_default();

            let parsed = match build_parsed_replay(meta, players, events, ages, &elo_by_key) {
                Ok(parsed) => parsed,
                Err(reason) => {
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
                    tracing::warn!(match_id = meta.match_id, reason = %detail, "skipping match — could not rebuild ParsedReplay");
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
                Ok(mut batch) => {
                    // Drop FK-unsafe unit/tech rows (out-of-dim ids) before batching — see
                    // `filter_to_known_dims`'s doc for why this is row-granular, counted, and not
                    // a silent drop.
                    filter_to_known_dims(&mut batch, &dims, &mut summary);
                    pending_ids.push(meta.match_id);
                    merge_into(&mut pending, batch);
                }
                Err(err) => {
                    summary.skipped_to_batch_err += 1;
                    tracing::warn!(match_id = meta.match_id, error = %err, "to_batch rejected this match");
                }
            }
            if pending_ids.len() >= cfg.batch_size.max(1) {
                flush(client, &mut pending, &mut pending_ids, cfg, &mut summary).await?;
            }
        }
        flush(client, &mut pending, &mut pending_ids, cfg, &mut summary).await?;

        processed += chunk.len();
        summary.chunks += 1;
        summary.processed_matches = processed as u64;
        write_cursor(cfg, processed);
        tracing::info!(
            chunk = summary.chunks,
            match_id_lo = lo,
            match_id_hi = hi,
            processed,
            total = ids.len(),
            imported = summary.imported,
            skipped_no_seed = summary.skipped_no_seed,
            chunk_elapsed_s = chunk_started.elapsed().as_secs(),
            "chunk complete"
        );
    }

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
        // A one-field object (DuckDB's distinct-id shape) closes with `}`, not `,`.
        assert_eq!(leading_match_id(r#"{"match_id":1}"#), Some(1));
        assert_eq!(leading_match_id(r#"{"match_id":2,"profile_id":null}"#), Some(2));
        assert_eq!(leading_match_id(r#"{"not_match_id":1}"#), None);
        assert_eq!(leading_match_id(""), None);
        assert_eq!(leading_match_id(r#"{"match_id":not_a_number}"#), None);
    }

    #[test]
    fn relic_match_type_from_ladder_maps_the_two_known_strings_and_fails_loud_otherwise() {
        assert_eq!(relic_match_type_from_ladder("1v1"), Some(RelicMatchType::SoloRmRanked));
        assert_eq!(relic_match_type_from_ladder("team"), Some(RelicMatchType::TeamRmRanked));
        assert_eq!(relic_match_type_from_ladder("ffa"), None);
        assert_eq!(relic_match_type_from_ladder(""), None);
    }

    #[test]
    fn index_seed_builds_elo_by_key_and_first_row_match_seed() {
        let rows = vec![
            SeedRowRaw { match_id: 1, profile_id: 5001, played_at: "2026-06-16 14:23:07".to_owned(), ladder: "1v1".to_owned(), rating: Some(1650) },
            SeedRowRaw { match_id: 1, profile_id: 5002, played_at: "2026-06-16 14:23:07".to_owned(), ladder: "1v1".to_owned(), rating: None },
            SeedRowRaw { match_id: 2, profile_id: 7001, played_at: "bad-timestamp".to_owned(), ladder: "team".to_owned(), rating: Some(900) },
        ];
        let idx = index_seed(rows);
        assert_eq!(idx.elo_by_key.get(&(1, 5001)), Some(&1650));
        assert_eq!(idx.elo_by_key.get(&(1, 5002)), None, "null rating -> no elo entry, never fabricated");
        assert_eq!(idx.elo_by_key.get(&(2, 7001)), Some(&900));
        assert_eq!(idx.match_seed.get(&1).map(|s| s.ladder.as_str()), Some("1v1"));
        assert_eq!(idx.bad_timestamps, 1, "the unparsable played_at is counted");
        assert!(!idx.match_seed.contains_key(&2), "a match whose only row had a bad timestamp has no seed");
    }

    fn sample_meta() -> ShardMetaRow {
        ShardMetaRow { match_id: 1001, build: Some(177_723), map_id: Some(9), duration_ms: 1_800_000, n_players: 2 }
    }
    fn sample_player(profile_id: i64, player_number: i64) -> ShardPlayerRow {
        ShardPlayerRow { player_number, profile_id, civ_id: 1, name: "Player".to_owned(), team: 1, color: 1, won: Some(true) }
    }

    #[test]
    fn build_parsed_replay_fills_elo_from_the_seed_map_never_leaves_a_stale_shard_value() {
        let meta = sample_meta();
        let players = vec![sample_player(5001, 1), sample_player(5002, 2)];
        let mut elo = HashMap::new();
        elo.insert((1001, 5001), 1650);
        let parsed = build_parsed_replay(&meta, players, vec![], vec![], &elo)
            .expect("valid shard rows must build a ParsedReplay");
        let p1 = parsed.players.iter().find(|p| p.profile_id == ProfileId(5001)).unwrap();
        assert_eq!(p1.elo, Some(1650), "elo comes from the seed map, not the shard");
        let p2 = parsed.players.iter().find(|p| p.profile_id == ProfileId(5002)).unwrap();
        assert_eq!(p2.elo, None, "no games row -> honest None, never guessed");
    }

    #[test]
    fn build_parsed_replay_converts_a_known_age_and_fails_loud_on_an_unknown_one() {
        let meta = sample_meta();
        let mk = |age: &str| ShardAgeRow {
            profile_id: 5001, civ_id: 1, won: Some(true), age: age.to_owned(),
            uptime_ms: 60_000, villagers: 20, military: 0, n_buildings: 5, n_research: 2,
        };
        let parsed = build_parsed_replay(&meta, vec![sample_player(5001, 1)], vec![], vec![mk("feudal")], &HashMap::new()).unwrap();
        assert_eq!(parsed.ages[0].age, Age::Feudal);
        let err = build_parsed_replay(&meta, vec![sample_player(5001, 1)], vec![], vec![mk("fuedal")], &HashMap::new()).unwrap_err();
        assert!(matches!(err, SkipReason::BadAge(ref s) if s == "fuedal"));
    }

    #[test]
    fn build_parsed_replay_fails_loud_on_a_numeric_field_that_does_not_fit() {
        let mut meta = sample_meta();
        meta.n_players = i64::from(i16::MAX) + 1;
        let err = build_parsed_replay(&meta, vec![sample_player(5001, 1)], vec![], vec![], &HashMap::new()).unwrap_err();
        assert!(matches!(err, SkipReason::BadNumeric("meta.n_players")));
    }

    #[test]
    fn filter_to_known_dims_drops_out_of_dim_ids_and_records_them() {
        use ingest::{NewMatchPlayerTech, NewMatchPlayerUnit};
        use pipeline_core::{GameUnitId, TechId};

        let mut batch = ReplayBatch {
            player_units: vec![
                // 83 (Villager) is in the dim; 1570 + 37 are the real out-of-dim ids this corpus
                // actually contains (see filter_to_known_dims's doc).
                NewMatchPlayerUnit { match_id: MatchId(1), profile_id: ProfileId(5001), unit_id: GameUnitId(83), trained: 5 },
                NewMatchPlayerUnit { match_id: MatchId(1), profile_id: ProfileId(5001), unit_id: GameUnitId(1570), trained: 1 },
                NewMatchPlayerUnit { match_id: MatchId(1), profile_id: ProfileId(5002), unit_id: GameUnitId(37), trained: 2 },
            ],
            player_techs: vec![
                NewMatchPlayerTech { match_id: MatchId(1), profile_id: ProfileId(5001), tech_id: TechId(22), t_ms: 10_000 },
                NewMatchPlayerTech { match_id: MatchId(1), profile_id: ProfileId(5001), tech_id: TechId(9999), t_ms: 20_000 },
            ],
            ..ReplayBatch::default()
        };
        let dims = DimIds {
            units: HashSet::from([83, 448]),
            techs: HashSet::from([22, 213]),
        };
        let mut summary = ImportShardsSummary::default();
        filter_to_known_dims(&mut batch, &dims, &mut summary);

        assert_eq!(batch.player_units.len(), 1, "only the in-dim unit (83) survives");
        assert_eq!(batch.player_units[0].unit_id, GameUnitId(83));
        assert_eq!(summary.dropped_unit_rows, 2);
        assert_eq!(summary.unknown_unit_ids, BTreeSet::from([37, 1570]));

        assert_eq!(batch.player_techs.len(), 1, "only the in-dim tech (22) survives");
        assert_eq!(summary.dropped_tech_rows, 1);
        assert_eq!(summary.unknown_tech_ids, BTreeSet::from([9999]));
    }
}
