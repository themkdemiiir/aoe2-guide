//! Reads `m_*.parquet` (matches) / `p_*.parquet` (players) into typed row vectors, via the
//! `parquet`/`arrow` crates (library-first — no hand-rolled parquet decoding, no shelling out to
//! duckdb). Column layout verified against a real sample, `m_2022-08-28.parquet` /
//! `p_2022-08-28.parquet` (see `.superpowers/sdd/task-M4a-aoestats-report.md`):
//!
//! - `m_*`: `map`(Utf8), `started_timestamp`(Timestamp(Microsecond, UTC)), `duration`(**embedded
//!   as Arrow `Duration(Nanosecond)`** — see below), `game_id`(Utf8), `num_players`(Int64),
//!   `leaderboard`(Utf8), `patch`(Int64). (`avg_elo`/`team_0_elo`/`team_1_elo`/`replay_enhanced`/
//!   `mirror`/`raw_match_type`/`game_type`/`game_speed`/`starting_age`/`irl_duration` exist in the
//!   file but aren't projected — not part of the target mapping.)
//! - `p_*`: `game_id`(Utf8), `civ`(Utf8), `profile_id`(Float64), `winner`(Boolean),
//!   `new_rating`(Int64), `opening`(Utf8), `feudal_age_uptime`/`castle_age_uptime`/
//!   `imperial_age_uptime`(Float64), and — read separately by [`read_player_age_sources`] for the
//!   ages-import path (Task M4b), not by [`read_players`] above — `replay_summary_raw`(Utf8).
//!   (`team`/`old_rating`/`match_rating_diff` exist but aren't projected by either reader.)
//!
//! **The `duration` surprise:** the brief's verified schema calls `duration` a plain "bigint
//! SECONDS" column (true of its *physical* Parquet storage), but the file embeds a pandas/pyarrow
//! `ARROW:schema` metadata key declaring it a `timedelta64[ns]` — so `parquet`'s arrow reader
//! (which honors that embedded schema over the bare physical type) hands it back as an Arrow
//! `Duration(Nanosecond)` array, not `Int64`. Confirmed against the real sample: row 0's raw i64 is
//! `2971600000000` (ns) = `2971.6` s, a plausible AoE2 match length. [`duration_ms`] divides by
//! `1_000_000` accordingly. This is intentionally NOT made polymorphic (e.g. "try Int64, fall back
//! to Duration") — silently reinterpreting an ambiguous magnitude is exactly the kind of
//! fabrication the project's fail-loud rule forbids; a future file where this column is shaped
//! differently trips [`AoestatsError::UnexpectedColumnType`] instead.

use std::fs::File;
use std::path::Path;

use arrow::array::{
    Array, BooleanArray, DurationNanosecondArray, Float64Array, Int64Array, RecordBatch,
    StringArray, TimestampMicrosecondArray,
};
use chrono::{DateTime, Utc};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ProjectionMask;

use crate::error::{AoestatsError, Result};

/// One `m_*.parquet` row, after Rust-side type decoding but BEFORE map-slug resolution (that's a
/// SQL JOIN against `maps`, see [`crate::db`]). `map` is already run through
/// [`pipeline_core::slug::slug`] — see the field doc.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawMatchRow {
    pub game_id: Option<String>,
    /// The aoestats `map` string, canonicalized via [`pipeline_core::slug::slug`]. aoestats uses
    /// underscore-separated slugs (`"coastal_forest"`); this repo's `maps.slug` dim is generated
    /// from `maps.tsv` names via the SAME `slug()` function, which strips all non-alphanumerics
    /// (`"coastalforest"`, no underscore) — confirmed by querying the live dim. Applying the one
    /// canonical slug function here (rather than joining on the raw aoestats string) is the
    /// "reconcile only at the ingestion boundary" rule (`feedback_consistent_names`), not a
    /// fabricated workaround: single-word map names (`"arabia"`) are a no-op either way.
    pub map: Option<String>,
    /// Raw `leaderboard` string, unmodified — validated/mapped to `1v1`/`team` in `db.rs`'s SQL,
    /// which fails loud (a NOT NULL violation on `matches.ladder`) on anything else.
    pub leaderboard: Option<String>,
    pub started_timestamp: Option<DateTime<Utc>>,
    /// Nanoseconds (see the module doc) — [`Self::duration_ms`] converts.
    pub duration_ns: Option<i64>,
    pub num_players: Option<i64>,
    pub patch: Option<i64>,
}

impl RawMatchRow {
    /// `duration_ns` converted to whole milliseconds (`matches.duration_ms`'s unit).
    pub fn duration_ms(&self) -> Option<i64> {
        self.duration_ns.map(|ns| ns / 1_000_000)
    }
}

/// One `p_*.parquet` row. `civ` is canonicalized the same way as [`RawMatchRow::map`] (a no-op for
/// aoestats civ slugs today — they're already single lowercase words — but consistent with the
/// one-canonical-slug-function rule rather than assuming it always will be).
#[derive(Debug, Clone, PartialEq)]
pub struct RawPlayerRow {
    pub game_id: Option<String>,
    pub civ: Option<String>,
    pub profile_id: Option<f64>,
    pub winner: Option<bool>,
    pub opening: Option<String>,
    pub feudal_age_uptime: Option<f64>,
    pub castle_age_uptime: Option<f64>,
    pub imperial_age_uptime: Option<f64>,
    pub new_rating: Option<i64>,
}

const MATCH_COLUMNS: &[&str] = &[
    "map",
    "started_timestamp",
    "duration",
    "game_id",
    "num_players",
    "leaderboard",
    "patch",
];

const PLAYER_COLUMNS: &[&str] = &[
    "game_id",
    "civ",
    "profile_id",
    "winner",
    "opening",
    "feudal_age_uptime",
    "castle_age_uptime",
    "imperial_age_uptime",
    "new_rating",
];

/// One `p_*.parquet` row projected for the ages-import path (Task M4b): the columns
/// `py::run_summaries` needs on its stdin. `civ` is canonicalized the same way as
/// [`RawPlayerRow::civ`]/[`RawMatchRow::map`] (see their field docs) since it feeds the same
/// `civs.slug` JOIN; `game_id`/`profile_id` are kept raw/unresolved for the same reason as
/// [`RawPlayerRow`] — resolution happens in `db.rs`'s SQL.
#[derive(Debug, Clone, PartialEq)]
pub struct RawAgeSourceRow {
    pub game_id: Option<String>,
    pub civ: Option<String>,
    pub profile_id: Option<f64>,
    pub winner: Option<bool>,
    /// Guaranteed non-empty and >50 chars by [`read_player_age_sources`]'s filter — never the raw
    /// column's NULL/short values, which are dropped before this struct is built.
    pub replay_summary_raw: String,
}

const PLAYER_AGE_SOURCE_COLUMNS: &[&str] =
    &["game_id", "civ", "profile_id", "winner", "replay_summary_raw"];

/// Opens `path`, projects it down to `columns` (skips everything else — notably
/// `replay_summary_raw`, which can be a sizeable JSON blob per row and is M4b's concern, not
/// this crate's), and returns every [`RecordBatch`] in the file.
fn read_projected_batches(
    path: &Path,
    columns: &'static [&'static str],
) -> Result<Vec<RecordBatch>> {
    let file = File::open(path).map_err(|source| AoestatsError::Open {
        path: path.to_owned(),
        source,
    })?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).map_err(|source| {
        AoestatsError::Parquet {
            path: path.to_owned(),
            source,
        }
    })?;

    let schema_descr = builder.parquet_schema();
    let mut indices = Vec::with_capacity(columns.len());
    for &name in columns {
        let idx = schema_descr
            .columns()
            .iter()
            .position(|col| col.name() == name)
            .ok_or(AoestatsError::MissingColumn {
                path: path.to_owned(),
                column: name,
            })?;
        indices.push(idx);
    }
    let mask = ProjectionMask::leaves(schema_descr, indices);

    let reader =
        builder
            .with_projection(mask)
            .build()
            .map_err(|source| AoestatsError::Parquet {
                path: path.to_owned(),
                source,
            })?;

    let mut batches = Vec::new();
    for batch in reader {
        let batch = batch.map_err(|source| AoestatsError::Parquet {
            path: path.to_owned(),
            source: source.into(),
        })?;
        batches.push(batch);
    }
    Ok(batches)
}

fn column<'a>(
    batch: &'a RecordBatch,
    path: &Path,
    name: &'static str,
) -> Result<&'a std::sync::Arc<dyn Array>> {
    batch
        .column_by_name(name)
        .ok_or(AoestatsError::MissingColumn {
            path: path.to_owned(),
            column: name,
        })
}

fn downcast<'a, T: 'static>(
    array: &'a std::sync::Arc<dyn Array>,
    path: &Path,
    name: &'static str,
) -> Result<&'a T> {
    array
        .as_any()
        .downcast_ref::<T>()
        .ok_or_else(|| AoestatsError::UnexpectedColumnType {
            path: path.to_owned(),
            column: name,
            actual: format!("{:?}", array.data_type()),
        })
}

/// Reads every row of `m_*.parquet` at `path` into [`RawMatchRow`]s.
pub fn read_matches(path: &Path) -> Result<Vec<RawMatchRow>> {
    let batches = read_projected_batches(path, MATCH_COLUMNS)?;
    let mut rows = Vec::new();
    for batch in &batches {
        let map = downcast::<StringArray>(column(batch, path, "map")?, path, "map")?;
        let started_timestamp = downcast::<TimestampMicrosecondArray>(
            column(batch, path, "started_timestamp")?,
            path,
            "started_timestamp",
        )?;
        let duration = downcast::<DurationNanosecondArray>(
            column(batch, path, "duration")?,
            path,
            "duration",
        )?;
        let game_id = downcast::<StringArray>(column(batch, path, "game_id")?, path, "game_id")?;
        let num_players =
            downcast::<Int64Array>(column(batch, path, "num_players")?, path, "num_players")?;
        let leaderboard =
            downcast::<StringArray>(column(batch, path, "leaderboard")?, path, "leaderboard")?;
        let patch = downcast::<Int64Array>(column(batch, path, "patch")?, path, "patch")?;

        for i in 0..batch.num_rows() {
            // Not folded into the generic `opt()` combinator like the other columns: an
            // out-of-chrono-range micros value must fail loud (`Err`), never silently default to
            // the Unix epoch — a `None` here means "the cell was NULL", not "we couldn't parse
            // it," and those two must stay distinguishable.
            let started_timestamp_value = if started_timestamp.is_null(i) {
                None
            } else {
                let micros = started_timestamp.value(i);
                Some(DateTime::from_timestamp_micros(micros).ok_or_else(|| {
                    AoestatsError::UnexpectedColumnType {
                        path: path.to_owned(),
                        column: "started_timestamp",
                        actual: format!("micros={micros} is out of chrono's representable range"),
                    }
                })?)
            };

            rows.push(RawMatchRow {
                game_id: opt(game_id, i, |a, i| a.value(i).to_owned()),
                map: opt(map, i, |a, i| pipeline_core::slug::slug(a.value(i))),
                leaderboard: opt(leaderboard, i, |a, i| a.value(i).to_owned()),
                started_timestamp: started_timestamp_value,
                duration_ns: opt(duration, i, |a, i| a.value(i)),
                num_players: opt(num_players, i, |a, i| a.value(i)),
                patch: opt(patch, i, |a, i| a.value(i)),
            });
        }
    }
    Ok(rows)
}

/// Reads every row of `p_*.parquet` at `path` into [`RawPlayerRow`]s.
pub fn read_players(path: &Path) -> Result<Vec<RawPlayerRow>> {
    let batches = read_projected_batches(path, PLAYER_COLUMNS)?;
    let mut rows = Vec::new();
    for batch in &batches {
        let game_id = downcast::<StringArray>(column(batch, path, "game_id")?, path, "game_id")?;
        let civ = downcast::<StringArray>(column(batch, path, "civ")?, path, "civ")?;
        let profile_id =
            downcast::<Float64Array>(column(batch, path, "profile_id")?, path, "profile_id")?;
        let winner = downcast::<BooleanArray>(column(batch, path, "winner")?, path, "winner")?;
        let opening = downcast::<StringArray>(column(batch, path, "opening")?, path, "opening")?;
        let feudal = downcast::<Float64Array>(
            column(batch, path, "feudal_age_uptime")?,
            path,
            "feudal_age_uptime",
        )?;
        let castle = downcast::<Float64Array>(
            column(batch, path, "castle_age_uptime")?,
            path,
            "castle_age_uptime",
        )?;
        let imperial = downcast::<Float64Array>(
            column(batch, path, "imperial_age_uptime")?,
            path,
            "imperial_age_uptime",
        )?;
        let new_rating =
            downcast::<Int64Array>(column(batch, path, "new_rating")?, path, "new_rating")?;

        for i in 0..batch.num_rows() {
            rows.push(RawPlayerRow {
                game_id: opt(game_id, i, |a, i| a.value(i).to_owned()),
                civ: opt(civ, i, |a, i| pipeline_core::slug::slug(a.value(i))),
                profile_id: opt(profile_id, i, |a, i| a.value(i)),
                winner: opt(winner, i, |a, i| a.value(i)),
                opening: opt(opening, i, |a, i| a.value(i).to_owned()),
                feudal_age_uptime: opt(feudal, i, |a, i| a.value(i)),
                castle_age_uptime: opt(castle, i, |a, i| a.value(i)),
                imperial_age_uptime: opt(imperial, i, |a, i| a.value(i)),
                new_rating: opt(new_rating, i, |a, i| a.value(i)),
            });
        }
    }
    Ok(rows)
}

/// Reads `p_*.parquet` at `path`, projecting down to the 5 columns the ages-import path needs
/// (see [`RawAgeSourceRow`]), and keeps only rows passing the task brief's `replay_summary_raw IS
/// NOT NULL AND length(replay_summary_raw) > 50` filter — applied here in Rust, not as a DuckDB
/// `WHERE`, since this crate reads parquet directly via `arrow`/`parquet` (see the module doc's
/// LIBRARY-FIRST rationale); there is no SQL layer between the file and this function to push the
/// predicate into.
pub fn read_player_age_sources(path: &Path) -> Result<Vec<RawAgeSourceRow>> {
    let batches = read_projected_batches(path, PLAYER_AGE_SOURCE_COLUMNS)?;
    let mut rows = Vec::new();
    for batch in &batches {
        let game_id = downcast::<StringArray>(column(batch, path, "game_id")?, path, "game_id")?;
        let civ = downcast::<StringArray>(column(batch, path, "civ")?, path, "civ")?;
        let profile_id =
            downcast::<Float64Array>(column(batch, path, "profile_id")?, path, "profile_id")?;
        let winner = downcast::<BooleanArray>(column(batch, path, "winner")?, path, "winner")?;
        let replay_summary_raw = downcast::<StringArray>(
            column(batch, path, "replay_summary_raw")?,
            path,
            "replay_summary_raw",
        )?;

        for i in 0..batch.num_rows() {
            if replay_summary_raw.is_null(i) {
                continue;
            }
            let raw = replay_summary_raw.value(i);
            if raw.len() <= 50 {
                continue;
            }
            rows.push(RawAgeSourceRow {
                game_id: opt(game_id, i, |a, i| a.value(i).to_owned()),
                civ: opt(civ, i, |a, i| pipeline_core::slug::slug(a.value(i))),
                profile_id: opt(profile_id, i, |a, i| a.value(i)),
                winner: opt(winner, i, |a, i| a.value(i)),
                replay_summary_raw: raw.to_owned(),
            });
        }
    }
    Ok(rows)
}

/// `None` if the array is null at `i`, else `Some(f(array, i))` — the one place null-checking is
/// spelled out, shared by every column above.
fn opt<A: Array, T>(array: &A, i: usize, f: impl FnOnce(&A, usize) -> T) -> Option<T> {
    if array.is_null(i) {
        None
    } else {
        Some(f(array, i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real sample's row 0: `duration` = `2_971_600_000_000` ns = `2_971.6` s — a plausible
    /// AoE2 match length (see the module doc). `duration_ms` must divide cleanly to `2_971_600`.
    #[test]
    fn duration_ms_converts_nanoseconds_from_the_real_sample() {
        let row = RawMatchRow {
            duration_ns: Some(2_971_600_000_000),
            ..Default::default()
        };
        assert_eq!(row.duration_ms(), Some(2_971_600));
    }

    #[test]
    fn duration_ms_is_none_when_the_source_cell_was_null() {
        let row = RawMatchRow {
            duration_ns: None,
            ..Default::default()
        };
        assert_eq!(row.duration_ms(), None);
    }
}
