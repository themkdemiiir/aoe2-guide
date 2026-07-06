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

use std::path::Path;

use tokio_postgres::binary_copy::BinaryCopyInWriter;
use tokio_postgres::types::{ToSql, Type};
use tokio_postgres::{Client, Transaction};

use crate::error::{AoestatsError, Result};
use crate::parquet_read::{read_matches, read_players, RawMatchRow, RawPlayerRow};

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
const INSERT_MATCHES_SQL: &str = r#"
INSERT INTO matches (match_id, source, ladder, map_id, build, patch, played_at, duration_ms, n_players)
SELECT
    s.game_id::bigint,
    'aoestats'::source_kind,
    (CASE s.leaderboard
        WHEN 'random_map' THEN '1v1'
        WHEN 'team_random_map' THEN 'team'
     END)::ladder_kind,
    mp.map_id,
    s.patch::int,
    NULL::text,
    s.started_timestamp,
    s.duration_ms::int,
    s.num_players::smallint
FROM stg_aoestats_matches s
JOIN maps mp ON mp.slug = s.map
WHERE s.game_id IS NOT NULL
ON CONFLICT (match_id) DO NOTHING
"#;

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
    };
    tracing::info!(
        matches_inserted = stats.matches_inserted,
        players_inserted = stats.players_inserted,
        unknown_map_slugs = stats.unknown_map_slugs.len(),
        unknown_civ_slugs = stats.unknown_civ_slugs.len(),
        matches_missing_game_id = stats.matches_missing_game_id,
        players_missing_identity = stats.players_missing_identity,
        "aoestats import committed"
    );
    Ok(stats)
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
