//! The write side: binary `COPY` into `TEMP … ON COMMIT DROP` staging tables holding the RAW
//! aoestats shape, then one set-based `INSERT … SELECT … JOIN … ON CONFLICT DO NOTHING` per
//! destination table — the same staging/COPY/idempotent-INSERT shape `ingest::ingest.rs` uses (see
//! that module's doc for the full rationale), duplicated rather than shared: aoestats' staging
//! tables hold *unresolved* TEXT slugs (`map`/`civ`/`leaderboard`/`game_id`), whereas `ingest`'s
//! staging tables already hold resolved integer `map_id`/`civ_id` — a genuinely different shape,
//! not a copy-paste that should be deduplicated.
//!
//! **Fail-loud vs. bulk-safe, by design, not by accident:**
//! - A non-numeric `game_id` (the `::bigint` casts below) and an unrecognized `leaderboard` (the
//!   `CASE`, no `ELSE` → NULL → a `matches.ladder` NOT NULL violation) are validated entirely by
//!   Postgres casts/constraints — either aborts the WHOLE transaction (nothing partial commits).
//!   The brief's verified schema guarantees `game_id` is always a numeric string and `leaderboard
//!   ∈ {random_map, team_random_map}`; a file that violates either assumption is a data-integrity
//!   event worth stopping the run for, not skipping one row of.
//! - An unmatched `map`/`civ` slug, and a NULL `game_id`/`profile_id` (a DIFFERENT thing than a
//!   non-numeric `game_id` above — NULL vs. malformed), are different in kind: dims are a living
//!   catalog (a legitimately new map shows up as "unknown" until the dims are extended), and a
//!   handful of rows genuinely missing their own identity is a REAL, OBSERVED property of the live
//!   archive — importing the real `p_2022-08-28.parquet` during this task's live smoke hit a row
//!   with a NULL `profile_id` (part of `match_players`' PK, so without this guard that one row
//!   would abort the entire 53k-row file, matches included, via the SAME single-transaction
//!   shape). Both cases are handled the same way: a plain `WHERE`/`JOIN` exclusion (never an
//!   attempted insert of an invalid identity), with a diagnostic pass BEFORE the real inserts so
//!   every excluded reason + its row count is `tracing::warn!`'d and returned in [`ImportStats`] —
//!   logged and counted, never silent, never fabricated.
//!
//! **Ages (Task M4b, [`import_ages`]):** a third, independent path over the SAME `p_*.parquet`
//! file's `replay_summary_raw` column — the aoestats per-age build summary, which is a
//! Python-repr blob (not JSON), routed through the one sanctioned Python step (`crate::py`) rather
//! than parsed in Rust. Same staging/COPY/JOIN shape as above, but with one twist: `match_ages` has
//! no primary key to `ON CONFLICT` against, so idempotency is instead a delete-then-insert scoped
//! to the batch's own `match_id`s — see [`import_ages`]'s doc for the exact mechanics.
//!
//! **Two more schema-drift fixes (same task as `parquet_read`'s arrow-type widening, but these
//! live here because they're SQL-layer, not arrow-type, issues — checked against the real archive,
//! not assumed):**
//!
//! - `leaderboard` -> `ladder`: [`INSERT_MATCHES_SQL`]'s `CASE` originally mapped only
//!   `random_map`/`team_random_map`. An archive-wide scan (all 172 `m_*.parquet` files, this task)
//!   found the STRING column itself is never NULL, but two more values appear starting ~2023-05:
//!   `co_random_map`/`co_team_random_map` (~4% of all rows archive-wide). Cross-tabbing against
//!   `num_players` for every occurrence of all four values confirms `co_random_map` is >99.99%
//!   2-player and `co_team_random_map` is >99.99% >2-player — the SAME shape split as the two
//!   already-mapped values — so they're folded into the same `1v1`/`team` buckets rather than left
//!   to fall through the `CASE` to NULL (which trips `matches.ladder`'s `NOT NULL` constraint and
//!   aborts the whole transaction, per this module's "fail-loud vs. bulk-safe" note above). A
//!   genuinely-unmapped FIFTH value would still do exactly that — the guard isn't removed, only
//!   widened to the archive's confirmed vocabulary. A defensive `num_players`-based fallback for an
//!   actually-NULL `leaderboard` is included too (2 players -> `1v1`, else `team`) even though this
//!   was never observed in any of the 172 files — cheap insurance against a future file, not a
//!   worked-around real bug.
//! - `duration` -> `matches.duration_ms`: exactly 2 of the 172 `m_*.parquet` files (`2022-10-02`,
//!   `2022-10-09`) contain a handful of rows whose `duration` is absurd — up to
//!   `5_574_815_100_000_000` ns (~64 DAYS), clearly corrupt telemetry from the source, not a real
//!   AoE2 match length — which overflows Postgres `integer` (`matches.duration_ms`'s column type,
//!   unchanged — this crate doesn't touch migration DDL) once divided to milliseconds, aborting the
//!   whole file's transaction with "integer out of range" for the sake of a handful of garbage
//!   values. [`INSERT_MATCHES_SQL`] now guards the `::int` cast with an `i32`-range check, writing
//!   `NULL` (the column is nullable) instead of aborting when it doesn't fit — the match itself
//!   (and every one of its fields the corrupt value doesn't touch) still imports. Counted and
//!   `tracing::warn!`'d via `matches_duration_out_of_range`, same "logged and counted, never
//!   silent, never fabricated" posture as the rest of this module.

use std::path::Path;

use tokio_postgres::binary_copy::BinaryCopyInWriter;
use tokio_postgres::types::{ToSql, Type};
use tokio_postgres::{Client, Transaction};

use crate::error::{AoestatsError, Result};
use crate::parquet_read::{
    read_matches, read_player_age_sources, read_players, RawMatchRow, RawPlayerRow,
};
use crate::py::{self, AgeOutputRow, AgeSourceRow};

/// One (slug, row count) pair for a map/civ value staged but not found in the dims — see the
/// module doc's "fail-loud vs. bulk-safe" note. `slug` is `None` when the source cell itself was
/// NULL (grouped together by the diagnostic `GROUP BY`).
pub type UnknownSlugCounts = Vec<(Option<String>, i64)>;

/// The outcome of one [`import_pair`] call.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportStats {
    /// Rows newly inserted into `matches` (genuinely-new `match_id`s this call staged).
    pub matches_inserted: u64,
    /// Rows newly inserted into `match_players`.
    pub players_inserted: u64,
    /// Distinct unmatched `map` slugs staged this call, with how many staged rows each affected.
    pub unknown_map_slugs: UnknownSlugCounts,
    /// Distinct unmatched `civ` slugs staged this call, with how many staged rows each affected.
    pub unknown_civ_slugs: UnknownSlugCounts,
    /// Staged match rows with a NULL `game_id` — excluded (never inserted with a fabricated id);
    /// see the module doc's "fail-loud vs. bulk-safe" note.
    pub matches_missing_game_id: u64,
    /// Staged player rows with a NULL `game_id` and/or NULL `profile_id` — excluded for the same
    /// reason (`profile_id` is part of `match_players`' primary key).
    pub players_missing_identity: u64,
    /// Staged match rows whose `duration` doesn't fit Postgres `integer` once converted to
    /// milliseconds (corrupt source telemetry, not a real AoE2 match length — see the module doc)
    /// — the match itself still imports, with `duration_ms` written `NULL` instead of aborting the
    /// whole transaction.
    pub matches_duration_out_of_range: u64,
}

/// The outcome of one [`import_ages`] call (Task M4b).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AgesImportStats {
    /// Player-rows read from `p_*.parquet` with a real `replay_summary_raw` blob (the brief's `IS
    /// NOT NULL AND length > 50` filter) — i.e. how many rows were sent to the python subprocess.
    pub source_rows: u64,
    /// Per-player-per-age rows the python subprocess emitted (up to 4x `source_rows`, one per
    /// AOE2 age reached).
    pub staged_rows: u64,
    /// `match_ages` rows deleted before re-inserting this batch's `match_id`s — the
    /// delete-then-insert idempotency guard (see [`import_ages`]'s doc). Zero on a genuinely-new
    /// batch; equal to `ages_inserted` on an unchanged re-import.
    pub ages_deleted: u64,
    /// Rows newly inserted into `match_ages`.
    pub ages_inserted: u64,
    /// Distinct unmatched `civ` slugs staged this call, with how many staged age-rows each
    /// affected.
    pub unknown_civ_slugs: UnknownSlugCounts,
    /// Staged age-rows with a NULL `game_id`, NULL `profile_id`, and/or NULL `uptime` — excluded
    /// (never inserted with a fabricated value); see [`import_ages`]'s doc.
    pub rows_missing_identity: u64,
}

const CREATE_STAGING_TABLES_SQL: &str = r#"
CREATE TEMP TABLE stg_aoestats_matches (
    game_id TEXT,
    map TEXT,
    leaderboard TEXT,
    started_timestamp TIMESTAMPTZ,
    -- Already converted from the source `duration` column's nanoseconds to milliseconds in Rust
    -- (`RawMatchRow::duration_ms`) — see `parquet_read`'s module doc for why that conversion can't
    -- be a bare SQL `* 1000` on the raw column.
    duration_ms BIGINT,
    num_players BIGINT,
    patch BIGINT
) ON COMMIT DROP;

CREATE TEMP TABLE stg_aoestats_players (
    game_id TEXT,
    civ TEXT,
    profile_id DOUBLE PRECISION,
    winner BOOLEAN,
    opening TEXT,
    feudal_age_uptime DOUBLE PRECISION,
    castle_age_uptime DOUBLE PRECISION,
    imperial_age_uptime DOUBLE PRECISION,
    new_rating BIGINT
) ON COMMIT DROP;
"#;

const MATCHES_COPY_SQL: &str = "COPY stg_aoestats_matches \
     (game_id, map, leaderboard, started_timestamp, duration_ms, num_players, patch) \
     FROM STDIN BINARY";
const MATCHES_COPY_TYPES: [Type; 7] = [
    Type::TEXT,
    Type::TEXT,
    Type::TEXT,
    Type::TIMESTAMPTZ,
    Type::INT8,
    Type::INT8,
    Type::INT8,
];

const PLAYERS_COPY_SQL: &str = "COPY stg_aoestats_players \
     (game_id, civ, profile_id, winner, opening, feudal_age_uptime, castle_age_uptime, \
      imperial_age_uptime, new_rating) \
     FROM STDIN BINARY";
const PLAYERS_COPY_TYPES: [Type; 9] = [
    Type::TEXT,
    Type::TEXT,
    Type::FLOAT8,
    Type::BOOL,
    Type::TEXT,
    Type::FLOAT8,
    Type::FLOAT8,
    Type::FLOAT8,
    Type::INT8,
];

/// `s.game_id::bigint` fails loud on a non-numeric `game_id` (see the module doc). `patch`
/// (aoestats' numeric patch/build id) becomes `matches.build`; `matches.patch` (the distinct
/// free-text patch label column) has no aoestats source, so it's written `NULL` — a deliberate,
/// documented choice per the brief's "your call, document" note, not a silent default (there's no
/// plausible non-NULL value to invent).
///
/// The `ladder` `CASE` and the `duration_ms` range guard are this task's fixes — see the module
/// doc's "Two more schema-drift fixes" section for why each one is shaped the way it is.
const INSERT_MATCHES_SQL: &str = r#"
INSERT INTO matches (match_id, source, ladder, map_id, build, patch, played_at, duration_ms, n_players)
SELECT
    s.game_id::bigint,
    'aoestats'::source_kind,
    (CASE
        WHEN s.leaderboard IN ('random_map', 'co_random_map') THEN '1v1'
        WHEN s.leaderboard IN ('team_random_map', 'co_team_random_map') THEN 'team'
        WHEN s.leaderboard IS NULL AND s.num_players = 2 THEN '1v1'
        WHEN s.leaderboard IS NULL AND s.num_players IS NOT NULL THEN 'team'
     END)::ladder_kind,
    mp.map_id,
    s.patch::int,
    NULL::text,
    s.started_timestamp,
    (CASE
        WHEN s.duration_ms BETWEEN -2147483648 AND 2147483647 THEN s.duration_ms::int
        ELSE NULL
     END),
    s.num_players::smallint
FROM stg_aoestats_matches s
JOIN maps mp ON mp.slug = s.map
WHERE s.game_id IS NOT NULL
ON CONFLICT (match_id) DO NOTHING
"#;

/// Diagnostic companion to [`INSERT_MATCHES_SQL`]'s `duration_ms` range guard — counted and
/// `tracing::warn!`'d the same way `UNKNOWN_MAP_SLUGS_SQL` et al. are (see the module doc).
const COUNT_MATCHES_DURATION_OUT_OF_RANGE_SQL: &str = "SELECT count(*) FROM stg_aoestats_matches \
     WHERE duration_ms IS NOT NULL AND duration_ms NOT BETWEEN -2147483648 AND 2147483647";

/// Gated on `JOIN matches m` (the real table, not a captured "just inserted" set — see the module
/// doc): a player row imports only once its parent match genuinely exists, whether that match came
/// from this same transaction's `INSERT` above or a prior run.
const INSERT_PLAYERS_SQL: &str = r#"
INSERT INTO match_players (match_id, profile_id, civ_id, elo, won, opening, feudal_t, castle_t, imperial_t)
SELECT
    s.game_id::bigint,
    s.profile_id::bigint,
    c.civ_id,
    s.new_rating::int,
    s.winner,
    s.opening,
    s.feudal_age_uptime::real,
    s.castle_age_uptime::real,
    s.imperial_age_uptime::real
FROM stg_aoestats_players s
JOIN civs c ON c.slug = s.civ
JOIN matches m ON m.match_id = s.game_id::bigint
WHERE s.game_id IS NOT NULL AND s.profile_id IS NOT NULL
ON CONFLICT (match_id, profile_id) DO NOTHING
"#;

const UNKNOWN_MAP_SLUGS_SQL: &str = r#"
SELECT s.map, count(*)
FROM stg_aoestats_matches s
LEFT JOIN maps mp ON mp.slug = s.map
WHERE mp.map_id IS NULL
GROUP BY s.map
"#;

const UNKNOWN_CIV_SLUGS_SQL: &str = r#"
SELECT s.civ, count(*)
FROM stg_aoestats_players s
LEFT JOIN civs c ON c.slug = s.civ
WHERE c.civ_id IS NULL
GROUP BY s.civ
"#;

const COUNT_MATCHES_MISSING_GAME_ID_SQL: &str =
    "SELECT count(*) FROM stg_aoestats_matches WHERE game_id IS NULL";

const COUNT_PLAYERS_MISSING_IDENTITY_SQL: &str =
    "SELECT count(*) FROM stg_aoestats_players WHERE game_id IS NULL OR profile_id IS NULL";

// --- Task M4b: ages import (`import_ages`). `uptime` is staged RAW (seconds, as emitted by
// `aoestats_summaries.py`) rather than pre-converted in Rust — the `* 1000 -> uptime_ms`
// conversion happens once, in `INSERT_AGES_SQL`, per the task brief. ---

const CREATE_AGES_STAGING_TABLE_SQL: &str = r#"
CREATE TEMP TABLE stg_aoestats_ages (
    game_id TEXT,
    profile_id DOUBLE PRECISION,
    civ TEXT,
    won BOOLEAN,
    age TEXT,
    uptime DOUBLE PRECISION,
    villagers INTEGER,
    military INTEGER,
    n_buildings INTEGER,
    n_research INTEGER
) ON COMMIT DROP;
"#;

const AGES_COPY_SQL: &str = "COPY stg_aoestats_ages \
     (game_id, profile_id, civ, won, age, uptime, villagers, military, n_buildings, n_research) \
     FROM STDIN BINARY";
const AGES_COPY_TYPES: [Type; 10] = [
    Type::TEXT,
    Type::FLOAT8,
    Type::TEXT,
    Type::BOOL,
    Type::TEXT,
    Type::FLOAT8,
    Type::INT4,
    Type::INT4,
    Type::INT4,
    Type::INT4,
];

/// `match_ages` has no primary key (same posture as `replay_ages`), so re-inserting an
/// already-loaded file would duplicate every row. This deletes every EXISTING `match_ages` row
/// whose `match_id` appears anywhere in the batch just staged — run in the SAME transaction,
/// strictly before [`INSERT_AGES_SQL`] — so a re-import replaces a match's ages rather than
/// appending a second copy. Scoped to `game_id IS NOT NULL` purely to avoid a pointless `NULL` in
/// the `IN (...)` list; harmless either way since `match_id` itself is NOT NULL.
const DELETE_EXISTING_AGES_SQL: &str = r#"
DELETE FROM match_ages
WHERE match_id IN (
    SELECT DISTINCT game_id::bigint FROM stg_aoestats_ages WHERE game_id IS NOT NULL
)
"#;

/// `c.slug = s.civ` mirrors [`INSERT_PLAYERS_SQL`]'s civ JOIN. The `WHERE` guard is this crate's
/// own addition beyond the brief's literal SQL (documented, per the "your call, document" note):
/// `match_ages.match_id`/`profile_id`/`uptime_ms` are all NOT NULL, but `uptime` can legitimately
/// be NULL if a future python change ever omits the key (see `py::AgeOutputRow::uptime`'s doc) —
/// without this guard, one such row would abort the WHOLE batch's insert (same "fail-loud vs.
/// bulk-safe" reasoning as `import_pair`'s missing-identity guard, see this module's doc). `age`
/// is cast from the staging table's plain `TEXT` to the `age_kind` enum here — the only cast this
/// function needs, mirroring [`INSERT_MATCHES_SQL`]'s `source`/`ladder` casts above.
const INSERT_AGES_SQL: &str = r#"
INSERT INTO match_ages (match_id, profile_id, civ_id, won, age, uptime_ms, villagers, military, n_buildings, n_research)
SELECT
    s.game_id::bigint,
    s.profile_id::bigint,
    c.civ_id,
    s.won,
    s.age::age_kind,
    (s.uptime * 1000)::int AS uptime_ms,
    s.villagers,
    s.military,
    s.n_buildings,
    s.n_research
FROM stg_aoestats_ages s
JOIN civs c ON c.slug = s.civ
WHERE s.game_id IS NOT NULL AND s.profile_id IS NOT NULL AND s.uptime IS NOT NULL
"#;

const UNKNOWN_AGES_CIV_SLUGS_SQL: &str = r#"
SELECT s.civ, count(*)
FROM stg_aoestats_ages s
LEFT JOIN civs c ON c.slug = s.civ
WHERE c.civ_id IS NULL
GROUP BY s.civ
"#;

const COUNT_AGES_MISSING_IDENTITY_SQL: &str = "SELECT count(*) FROM stg_aoestats_ages \
     WHERE game_id IS NULL OR profile_id IS NULL OR uptime IS NULL";

/// Reads `matches_path`/`players_path` and idempotently loads them into the live schema in ONE
/// transaction (see the module doc for the exact algorithm and its fail-loud/bulk-safe split).
/// Re-importing an identical pair leaves every table's row count unchanged (`ON CONFLICT DO
/// NOTHING` on both PKs).
pub async fn import_pair(
    client: &mut Client,
    matches_path: &Path,
    players_path: &Path,
) -> Result<ImportStats> {
    let raw_matches = read_matches(matches_path)?;
    let raw_players = read_players(players_path)?;

    let tx = client.transaction().await.map_err(AoestatsError::Db)?;

    tx.batch_execute(CREATE_STAGING_TABLES_SQL)
        .await
        .map_err(AoestatsError::Db)?;

    copy_matches(&tx, &raw_matches).await?;
    copy_players(&tx, &raw_players).await?;

    let unknown_map_slugs = fetch_unknown_slugs(&tx, UNKNOWN_MAP_SLUGS_SQL).await?;
    for (slug, count) in &unknown_map_slugs {
        tracing::warn!(
            slug = ?slug,
            rows = count,
            "aoestats import: unknown map slug — extend the maps dim to pick these up"
        );
    }

    let unknown_civ_slugs = fetch_unknown_slugs(&tx, UNKNOWN_CIV_SLUGS_SQL).await?;
    for (slug, count) in &unknown_civ_slugs {
        tracing::warn!(
            slug = ?slug,
            rows = count,
            "aoestats import: unknown civ slug — extend the civs dim to pick these up"
        );
    }

    let matches_missing_game_id = fetch_count(&tx, COUNT_MATCHES_MISSING_GAME_ID_SQL).await?;
    if matches_missing_game_id > 0 {
        tracing::warn!(
            rows = matches_missing_game_id,
            "aoestats import: match rows with a NULL game_id — excluded, not fabricated"
        );
    }
    let players_missing_identity = fetch_count(&tx, COUNT_PLAYERS_MISSING_IDENTITY_SQL).await?;
    if players_missing_identity > 0 {
        tracing::warn!(
            rows = players_missing_identity,
            "aoestats import: player rows with a NULL game_id/profile_id — excluded, not fabricated"
        );
    }
    let matches_duration_out_of_range =
        fetch_count(&tx, COUNT_MATCHES_DURATION_OUT_OF_RANGE_SQL).await?;
    if matches_duration_out_of_range > 0 {
        tracing::warn!(
            rows = matches_duration_out_of_range,
            "aoestats import: match rows with a duration outside Postgres integer range — \
             duration_ms written NULL instead of aborting the file, not fabricated"
        );
    }

    let matches_inserted = tx
        .execute(INSERT_MATCHES_SQL, &[])
        .await
        .map_err(AoestatsError::Db)?;
    let players_inserted = tx
        .execute(INSERT_PLAYERS_SQL, &[])
        .await
        .map_err(AoestatsError::Db)?;

    tx.commit().await.map_err(AoestatsError::Db)?;

    let stats = ImportStats {
        matches_inserted,
        players_inserted,
        unknown_map_slugs,
        unknown_civ_slugs,
        matches_missing_game_id: matches_missing_game_id as u64,
        players_missing_identity: players_missing_identity as u64,
        matches_duration_out_of_range: matches_duration_out_of_range as u64,
    };
    tracing::info!(
        matches_inserted = stats.matches_inserted,
        players_inserted = stats.players_inserted,
        unknown_map_slugs = stats.unknown_map_slugs.len(),
        unknown_civ_slugs = stats.unknown_civ_slugs.len(),
        matches_missing_game_id = stats.matches_missing_game_id,
        players_missing_identity = stats.players_missing_identity,
        matches_duration_out_of_range = stats.matches_duration_out_of_range,
        "aoestats import committed"
    );
    Ok(stats)
}

/// Reads `players_path`'s `replay_summary_raw` blobs, runs them through the embedded
/// `aoestats_summaries.py` subprocess (`crate::py`, the one sanctioned Python step — see the crate
/// module doc), validates every emitted `age` string against [`pipeline_core::Age`] (fail-loud on
/// an unexpected value — a data-corruption signal, not a per-row skip), and idempotently loads the
/// per-age rows into `match_ages`.
///
/// **Idempotency:** `match_ages` has no primary key, so a naive re-run would duplicate every row.
/// This guards it with a delete-then-insert SCOPED TO THIS BATCH, in the same transaction as the
/// insert ([`DELETE_EXISTING_AGES_SQL`]): every existing `match_ages` row whose `match_id` appears
/// in this batch's staged `game_id`s is deleted first, so re-importing an unchanged file replaces
/// its ages with an identical set (net-zero row-count change — see the testcontainers test), and
/// re-importing a file whose underlying replay summaries changed replaces the old values rather
/// than appending duplicates alongside them.
///
/// Does NOT require `matches_path`/`import_pair` to have run first — `match_ages` has no FK to
/// `matches` (same posture as `replay_ages`), so ages can be imported before, after, or without
/// their parent match ever landing in `matches`.
pub async fn import_ages(client: &mut Client, players_path: &Path) -> Result<AgesImportStats> {
    let source_rows = read_player_age_sources(players_path)?;
    let source_count = source_rows.len() as u64;

    let py_input: Vec<AgeSourceRow> = source_rows
        .into_iter()
        .map(|row| AgeSourceRow {
            game_id: row.game_id,
            profile_id: row.profile_id,
            civ: row.civ,
            winner: row.winner,
            replay_summary_raw: row.replay_summary_raw,
        })
        .collect();

    // `run_summaries` shells out synchronously (spawn + blocking pipe IO) — run it off the async
    // runtime's worker threads via `spawn_blocking` rather than blocking one of them directly.
    let output_rows: Vec<AgeOutputRow> =
        tokio::task::spawn_blocking(move || py::run_summaries(py_input))
            .await
            .map_err(AoestatsError::BlockingTask)??;
    let staged_count = output_rows.len() as u64;

    // Fail loud on the FIRST unexpected `age` value, before touching the database at all — see
    // this fn's doc.
    for row in &output_rows {
        pipeline_core::Age::try_from(row.age.as_str()).map_err(AoestatsError::AgeValidation)?;
    }

    let tx = client.transaction().await.map_err(AoestatsError::Db)?;

    tx.batch_execute(CREATE_AGES_STAGING_TABLE_SQL)
        .await
        .map_err(AoestatsError::Db)?;

    copy_ages(&tx, &output_rows).await?;

    let unknown_civ_slugs = fetch_unknown_slugs(&tx, UNKNOWN_AGES_CIV_SLUGS_SQL).await?;
    for (slug, count) in &unknown_civ_slugs {
        tracing::warn!(
            slug = ?slug,
            rows = count,
            "aoestats ages import: unknown civ slug — extend the civs dim to pick these up"
        );
    }

    let rows_missing_identity = fetch_count(&tx, COUNT_AGES_MISSING_IDENTITY_SQL).await?;
    if rows_missing_identity > 0 {
        tracing::warn!(
            rows = rows_missing_identity,
            "aoestats ages import: rows with a NULL game_id/profile_id/uptime — excluded, not \
             fabricated"
        );
    }

    // Delete-then-insert, same transaction — see this fn's "Idempotency" doc.
    let ages_deleted = tx
        .execute(DELETE_EXISTING_AGES_SQL, &[])
        .await
        .map_err(AoestatsError::Db)?;
    let ages_inserted = tx
        .execute(INSERT_AGES_SQL, &[])
        .await
        .map_err(AoestatsError::Db)?;

    tx.commit().await.map_err(AoestatsError::Db)?;

    let stats = AgesImportStats {
        source_rows: source_count,
        staged_rows: staged_count,
        ages_deleted,
        ages_inserted,
        unknown_civ_slugs,
        rows_missing_identity: rows_missing_identity as u64,
    };
    tracing::info!(
        source_rows = stats.source_rows,
        staged_rows = stats.staged_rows,
        ages_deleted = stats.ages_deleted,
        ages_inserted = stats.ages_inserted,
        unknown_civ_slugs = stats.unknown_civ_slugs.len(),
        rows_missing_identity = stats.rows_missing_identity,
        "aoestats ages import committed"
    );
    Ok(stats)
}

async fn copy_ages(tx: &Transaction<'_>, rows: &[AgeOutputRow]) -> Result<()> {
    let sink = tx.copy_in(AGES_COPY_SQL).await.map_err(AoestatsError::Db)?;
    let writer = BinaryCopyInWriter::new(sink, &AGES_COPY_TYPES);
    tokio::pin!(writer);
    for row in rows {
        let params: [&(dyn ToSql + Sync); 10] = [
            &row.game_id,
            &row.profile_id,
            &row.civ,
            &row.won,
            &row.age,
            &row.uptime,
            &row.villagers,
            &row.military,
            &row.n_buildings,
            &row.n_research,
        ];
        writer
            .as_mut()
            .write(&params)
            .await
            .map_err(AoestatsError::Db)?;
    }
    writer.as_mut().finish().await.map_err(AoestatsError::Db)?;
    Ok(())
}

async fn fetch_unknown_slugs(tx: &Transaction<'_>, sql: &str) -> Result<UnknownSlugCounts> {
    let rows = tx.query(sql, &[]).await.map_err(AoestatsError::Db)?;
    Ok(rows.iter().map(|row| (row.get(0), row.get(1))).collect())
}

async fn fetch_count(tx: &Transaction<'_>, sql: &str) -> Result<i64> {
    let row = tx.query_one(sql, &[]).await.map_err(AoestatsError::Db)?;
    Ok(row.get(0))
}

async fn copy_matches(tx: &Transaction<'_>, rows: &[RawMatchRow]) -> Result<()> {
    let sink = tx
        .copy_in(MATCHES_COPY_SQL)
        .await
        .map_err(AoestatsError::Db)?;
    let writer = BinaryCopyInWriter::new(sink, &MATCHES_COPY_TYPES);
    tokio::pin!(writer);
    for row in rows {
        let duration_ms = row.duration_ms();
        let params: [&(dyn ToSql + Sync); 7] = [
            &row.game_id,
            &row.map,
            &row.leaderboard,
            &row.started_timestamp,
            &duration_ms,
            &row.num_players,
            &row.patch,
        ];
        writer
            .as_mut()
            .write(&params)
            .await
            .map_err(AoestatsError::Db)?;
    }
    writer.as_mut().finish().await.map_err(AoestatsError::Db)?;
    Ok(())
}

async fn copy_players(tx: &Transaction<'_>, rows: &[RawPlayerRow]) -> Result<()> {
    let sink = tx
        .copy_in(PLAYERS_COPY_SQL)
        .await
        .map_err(AoestatsError::Db)?;
    let writer = BinaryCopyInWriter::new(sink, &PLAYERS_COPY_TYPES);
    tokio::pin!(writer);
    for row in rows {
        let params: [&(dyn ToSql + Sync); 9] = [
            &row.game_id,
            &row.civ,
            &row.profile_id,
            &row.winner,
            &row.opening,
            &row.feudal_age_uptime,
            &row.castle_age_uptime,
            &row.imperial_age_uptime,
            &row.new_rating,
        ];
        writer
            .as_mut()
            .write(&params)
            .await
            .map_err(AoestatsError::Db)?;
    }
    writer.as_mut().finish().await.map_err(AoestatsError::Db)?;
    Ok(())
}
