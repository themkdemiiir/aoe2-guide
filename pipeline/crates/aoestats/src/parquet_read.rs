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
//!
//! **Schema drift across the 3.5-year archive (172 `m_`/`p_` pairs, 2022-08 -> 2026-02), fixed by
//! this module (task: "aoestats parquet schema drift"):** two full archive-wide schema scans (every
//! file, this task — the second one triggered by a mid-task review flagging that this reader
//! downcasts columns in a fixed order and returns on the FIRST mismatch, so a failures log from a
//! partial run can only ever show the earliest-in-order drift per file, hiding anything later) found
//! four columns whose *physical arrow type* legitimately varies file-to-file, on top of the single
//! 2022-08 baseline this module originally assumed:
//!
//! - `profile_id` (`p_*`): `Float64` in the 36 oldest files, `Int64` in the 136 newer ones (never
//!   observed as `Int32` in this archive, but accepted too — see [`read_profile_id`] — since a
//!   pyarrow/pandas export could plausibly narrow to it for a file this archive doesn't happen to
//!   contain, and accepting it costs nothing). Always decoded to `i64` first, then represented as
//!   `f64` for the rest of the pipeline (unchanged wire/SQL shape) — lossless, since a Steam/Relic
//!   profile id is nowhere near `f64`'s 2^53 exact-integer ceiling.
//! - `started_timestamp` (`m_*`): `Timestamp(Microsecond, UTC)` in the 68 oldest files,
//!   `Timestamp(Nanosecond, UTC)` in the 104 newer ones. [`read_started_timestamp`] accepts every
//!   `TimeUnit` (Second/Millisecond/Microsecond/Nanosecond), decoding each to the same
//!   `DateTime<Utc>` — the physical integer is always a UTC epoch count regardless of unit (Arrow's
//!   own storage semantics for a tz-aware timestamp column), so there's no ambiguity to fabricate
//!   an answer for, unlike the `duration` column above.
//! - `opening`/`feudal_age_uptime`/`castle_age_uptime`/`imperial_age_uptime` (`p_*`): `Utf8`/
//!   `Float64` respectively through 2024-03-10, then Arrow's dedicated `Null` type starting
//!   2024-03-17 (`castle`/`imperial`) and 2024-03-24 (`opening`/`feudal`) onward (90/172 files) —
//!   **verified NOT a semantic change** (a mid-task review floated "opening became an integer
//!   code," which the second full scan disproves: every file's `opening`/uptime columns are either
//!   their original type or Arrow `Null`, never `Int32`/`Int64`). `Null` is pyarrow/pandas' encoding
//!   for a column that is ENTIRELY absent/NULL for that whole file — aoestats appears to have
//!   stopped populating these four fields archive-wide from 2024-03 on. [`read_opening`]/
//!   [`read_uptime`] accept `Null` as "every row is `None`" (the same value the original type would
//!   produce if every cell were NULL) rather than fabricating a placeholder — nothing is inferred,
//!   the fields are just genuinely absent from this point in the archive.
//!
//! `raw_match_type` (`m_*`) also drifts (`Float64` -> `Int64`, confirmed by the second scan), but
//! isn't in [`MATCH_COLUMNS`] — this module never reads it, so that drift is a non-issue here.
//!
//! Two more issues turned out NOT to be arrow-type drift at all, once checked against the real
//! files (never assume — verify): see `db`'s module doc for the `leaderboard`-value and
//! `duration`-magnitude fixes, which live in the SQL layer instead.
//!
//! Fail-loud is preserved for anything else: a column typed as neither `Float64`/`Int32`/`Int64`
//! (`profile_id`) nor one of the four `Timestamp` units (`started_timestamp`) nor `Utf8`/`Null`
//! (`opening`) nor `Float64`/`Null` (the three uptimes) still trips
//! [`AoestatsError::UnexpectedColumnType`] — this widens the accepted set to the archive's
//! confirmed historical variants, it does not remove the guard.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use arrow::array::{
    Array, BooleanArray, DurationNanosecondArray, Float64Array, Int32Array, Int64Array,
    RecordBatch, StringArray, TimestampMicrosecondArray, TimestampMillisecondArray,
    TimestampNanosecondArray, TimestampSecondArray,
};
use arrow::datatypes::{DataType, TimeUnit};
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
    ///
    /// Also passed through [`canonical_aoestats_map_slug`] AFTER `slug()` — a tiny, closed alias
    /// table for the two ranked-map spelling/article mismatches between aoestats and `maps.tsv`
    /// (task maps-gap, `.superpowers/sdd/task-mapsgap-brief.md`): aoestats' own spelling quirks,
    /// mapped onto the existing `maps.tsv` dim ids, not new map data.
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

/// `profile_id`'s three confirmed-historical physical representations (see the module doc) — one
/// borrow of whichever variant the file actually has, resolved once per batch/column rather than
/// re-matching on `data_type()` per row.
enum ProfileIdColumn<'a> {
    Int64(&'a Int64Array),
    Int32(&'a Int32Array),
    Float64(&'a Float64Array),
}

/// Matches `array`'s physical arrow type against the three confirmed `profile_id` variants (see
/// the module doc); anything else is genuinely-unexpected schema drift and fails loud.
fn read_profile_id<'a>(
    array: &'a Arc<dyn Array>,
    path: &Path,
    name: &'static str,
) -> Result<ProfileIdColumn<'a>> {
    if let Some(a) = array.as_any().downcast_ref::<Int64Array>() {
        return Ok(ProfileIdColumn::Int64(a));
    }
    if let Some(a) = array.as_any().downcast_ref::<Int32Array>() {
        return Ok(ProfileIdColumn::Int32(a));
    }
    if let Some(a) = array.as_any().downcast_ref::<Float64Array>() {
        return Ok(ProfileIdColumn::Float64(a));
    }
    Err(AoestatsError::UnexpectedColumnType {
        path: path.to_owned(),
        column: name,
        actual: format!("{:?}", array.data_type()),
    })
}

impl ProfileIdColumn<'_> {
    fn is_null(&self, i: usize) -> bool {
        match self {
            Self::Int64(a) => a.is_null(i),
            Self::Int32(a) => a.is_null(i),
            Self::Float64(a) => a.is_null(i),
        }
    }

    /// Decodes row `i` to `i64` first regardless of physical type (per the module doc), then hands
    /// back `f64` — the type the rest of this crate's `profile_id` plumbing (staging column,
    /// `AgeSourceRow`/`AgeOutputRow`'s python JSON wire shape) already expects. Lossless: a
    /// Steam/Relic profile id is nowhere near `f64`'s 2^53 exact-integer ceiling.
    fn value_as_f64(&self, i: usize) -> f64 {
        match self {
            Self::Int64(a) => a.value(i) as f64,
            Self::Int32(a) => a.value(i) as f64,
            Self::Float64(a) => a.value(i),
        }
    }
}

/// `started_timestamp`'s four confirmed-historical `TimeUnit`s (see the module doc) — only
/// `Microsecond`/`Nanosecond` have been observed in the real archive; `Second`/`Millisecond` are
/// accepted too since they're the only other members of arrow's `TimeUnit` enum and cost nothing
/// extra to handle uniformly.
enum TimestampColumn<'a> {
    Second(&'a TimestampSecondArray),
    Millisecond(&'a TimestampMillisecondArray),
    Microsecond(&'a TimestampMicrosecondArray),
    Nanosecond(&'a TimestampNanosecondArray),
}

/// Matches `array`'s `DataType` against `Timestamp(<any TimeUnit>, _)` (the timezone string itself
/// is irrelevant to decoding — see the module doc: a tz-aware Arrow timestamp's physical integer is
/// always a UTC epoch count no matter what the declared tz is). Anything that isn't a `Timestamp`
/// at all is genuinely-unexpected schema drift and fails loud.
fn read_started_timestamp<'a>(
    array: &'a Arc<dyn Array>,
    path: &Path,
    name: &'static str,
) -> Result<TimestampColumn<'a>> {
    match array.data_type() {
        DataType::Timestamp(TimeUnit::Second, _) => {
            Ok(TimestampColumn::Second(downcast(array, path, name)?))
        }
        DataType::Timestamp(TimeUnit::Millisecond, _) => {
            Ok(TimestampColumn::Millisecond(downcast(array, path, name)?))
        }
        DataType::Timestamp(TimeUnit::Microsecond, _) => {
            Ok(TimestampColumn::Microsecond(downcast(array, path, name)?))
        }
        DataType::Timestamp(TimeUnit::Nanosecond, _) => {
            Ok(TimestampColumn::Nanosecond(downcast(array, path, name)?))
        }
        other => Err(AoestatsError::UnexpectedColumnType {
            path: path.to_owned(),
            column: name,
            actual: format!("{other:?}"),
        }),
    }
}

impl TimestampColumn<'_> {
    fn is_null(&self, i: usize) -> bool {
        match self {
            Self::Second(a) => a.is_null(i),
            Self::Millisecond(a) => a.is_null(i),
            Self::Microsecond(a) => a.is_null(i),
            Self::Nanosecond(a) => a.is_null(i),
        }
    }

    /// Decodes row `i` to a UTC instant. `Nanosecond` is infallible (chrono's
    /// `from_timestamp_nanos` — an `i64` count of nanoseconds since epoch always fits chrono's much
    /// wider representable range); `Second`/`Millisecond`/`Microsecond` can in principle overflow
    /// that range, so — matching the pre-existing microsecond-only behavior this replaces — an
    /// out-of-range value fails loud (`Err`) rather than silently defaulting, since a `None` return
    /// here must stay reserved for "the cell was NULL" (checked separately via `is_null`).
    fn value_utc(&self, i: usize, path: &Path, name: &'static str) -> Result<DateTime<Utc>> {
        let out_of_range = |raw: i64| AoestatsError::UnexpectedColumnType {
            path: path.to_owned(),
            column: name,
            actual: format!("timestamp value {raw} is out of chrono's representable range"),
        };
        match self {
            Self::Second(a) => {
                let raw = a.value(i);
                DateTime::from_timestamp(raw, 0).ok_or_else(|| out_of_range(raw))
            }
            Self::Millisecond(a) => {
                let raw = a.value(i);
                DateTime::from_timestamp_millis(raw).ok_or_else(|| out_of_range(raw))
            }
            Self::Microsecond(a) => {
                let raw = a.value(i);
                DateTime::from_timestamp_micros(raw).ok_or_else(|| out_of_range(raw))
            }
            Self::Nanosecond(a) => Ok(DateTime::from_timestamp_nanos(a.value(i))),
        }
    }
}

/// `opening`'s two confirmed-historical physical representations (see the module doc): `Utf8`
/// through 2024-03-10, Arrow's `Null` type (the whole column absent) from 2024-03-24 on.
enum OpeningColumn<'a> {
    Utf8(&'a StringArray),
    /// The column's arrow type is `Null` — every row is absent, not merely NULL-valued within a
    /// typed array (there is no underlying value to be null-checked against).
    AllAbsent,
}

fn read_opening<'a>(
    array: &'a Arc<dyn Array>,
    path: &Path,
    name: &'static str,
) -> Result<OpeningColumn<'a>> {
    match array.data_type() {
        DataType::Utf8 => Ok(OpeningColumn::Utf8(downcast(array, path, name)?)),
        DataType::Null => Ok(OpeningColumn::AllAbsent),
        other => Err(AoestatsError::UnexpectedColumnType {
            path: path.to_owned(),
            column: name,
            actual: format!("{other:?}"),
        }),
    }
}

impl OpeningColumn<'_> {
    fn value_opt(&self, i: usize) -> Option<String> {
        match self {
            Self::Utf8(a) => {
                if a.is_null(i) {
                    None
                } else {
                    Some(a.value(i).to_owned())
                }
            }
            Self::AllAbsent => None,
        }
    }
}

/// `feudal_age_uptime`/`castle_age_uptime`/`imperial_age_uptime`'s two confirmed-historical
/// physical representations (see the module doc) — same `Float64`-or-`Null` shape as
/// [`OpeningColumn`], shared across all three uptime columns since they drifted identically.
enum UptimeColumn<'a> {
    Float64(&'a Float64Array),
    /// The column's arrow type is `Null` — every row is absent (see [`OpeningColumn::AllAbsent`]'s
    /// doc for why that's a distinct case from "null-valued").
    AllAbsent,
}

fn read_uptime<'a>(
    array: &'a Arc<dyn Array>,
    path: &Path,
    name: &'static str,
) -> Result<UptimeColumn<'a>> {
    match array.data_type() {
        DataType::Float64 => Ok(UptimeColumn::Float64(downcast(array, path, name)?)),
        DataType::Null => Ok(UptimeColumn::AllAbsent),
        other => Err(AoestatsError::UnexpectedColumnType {
            path: path.to_owned(),
            column: name,
            actual: format!("{other:?}"),
        }),
    }
}

impl UptimeColumn<'_> {
    fn value_opt(&self, i: usize) -> Option<f64> {
        match self {
            Self::Float64(a) => {
                if a.is_null(i) {
                    None
                } else {
                    Some(a.value(i))
                }
            }
            Self::AllAbsent => None,
        }
    }
}

/// Reconciles the two known aoestats/`maps.tsv` map-slug mismatches, applied AFTER
/// [`pipeline_core::slug::slug`] (task maps-gap — see
/// `.superpowers/sdd/task-mapsgap-brief.md`). `slug()` only strips non-alphanumerics and
/// lowercases; it has no way to fix an outright misspelling or a dropped article, so the ~129k
/// aoestats rows for these two ranked maps were silently excluded by [`crate::db`]'s `JOIN maps mp
/// ON mp.slug = s.map` until this table was added. Both entries are UNAMBIGUOUS reconciliations of
/// an aoestats spelling quirk onto an EXISTING `maps.tsv` dim id, not fabricated map data:
///
/// - `"scandanavia"` (aoestats misspells "Scandinavia" with an extra 'a') -> `"scandinavia"`
///   (`maps.tsv` id 25, "Scandinavia"). It cannot be Norse Lands (id 56, "Real-World Scandinavia",
///   which slugs to `"norselands"`) — id 25 is the only "Scandinavia" in the dim.
/// - `"passage"` (aoestats drops the leading article from "The Passage") -> `"thepassage"`
///   (`maps.tsv` id 185, the only map with "Passage" in its name).
///
/// Kept as a closed, explicitly-enumerated `match` on purpose: a future third aoestats/`maps.tsv`
/// mismatch must be a deliberate, reviewed addition to this table, never silently absorbed by a
/// catch-all pattern.
fn canonical_aoestats_map_slug(slug: &str) -> &str {
    match slug {
        "scandanavia" => "scandinavia",
        "passage" => "thepassage",
        other => other,
    }
}

/// Reads every row of `m_*.parquet` at `path` into [`RawMatchRow`]s.
pub fn read_matches(path: &Path) -> Result<Vec<RawMatchRow>> {
    let batches = read_projected_batches(path, MATCH_COLUMNS)?;
    let mut rows = Vec::new();
    for batch in &batches {
        let map = downcast::<StringArray>(column(batch, path, "map")?, path, "map")?;
        let started_timestamp = read_started_timestamp(
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
            // out-of-chrono-range value must fail loud (`Err`), never silently default to the
            // Unix epoch — a `None` here means "the cell was NULL", not "we couldn't parse it,"
            // and those two must stay distinguishable.
            let started_timestamp_value = if started_timestamp.is_null(i) {
                None
            } else {
                Some(started_timestamp.value_utc(i, path, "started_timestamp")?)
            };

            rows.push(RawMatchRow {
                game_id: opt(game_id, i, |a, i| a.value(i).to_owned()),
                map: opt(map, i, |a, i| {
                    canonical_aoestats_map_slug(&pipeline_core::slug::slug(a.value(i))).to_owned()
                }),
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
            read_profile_id(column(batch, path, "profile_id")?, path, "profile_id")?;
        let winner = downcast::<BooleanArray>(column(batch, path, "winner")?, path, "winner")?;
        let opening = read_opening(column(batch, path, "opening")?, path, "opening")?;
        let feudal = read_uptime(
            column(batch, path, "feudal_age_uptime")?,
            path,
            "feudal_age_uptime",
        )?;
        let castle = read_uptime(
            column(batch, path, "castle_age_uptime")?,
            path,
            "castle_age_uptime",
        )?;
        let imperial = read_uptime(
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
                profile_id: if profile_id.is_null(i) {
                    None
                } else {
                    Some(profile_id.value_as_f64(i))
                },
                winner: opt(winner, i, |a, i| a.value(i)),
                opening: opening.value_opt(i),
                feudal_age_uptime: feudal.value_opt(i),
                castle_age_uptime: castle.value_opt(i),
                imperial_age_uptime: imperial.value_opt(i),
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
            read_profile_id(column(batch, path, "profile_id")?, path, "profile_id")?;
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
                profile_id: if profile_id.is_null(i) {
                    None
                } else {
                    Some(profile_id.value_as_f64(i))
                },
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
    use arrow::array::{ArrayRef, NullArray};
    use arrow::datatypes::{Field, Schema};
    use chrono::TimeZone;
    use parquet::arrow::ArrowWriter;

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

    // --- Schema-drift coverage (this task): every historical arrow-type variant confirmed by a
    // full 172-file archive scan, plus proof the fail-loud guard survives the widening. ---

    /// Writes a one-row `p_*.parquet`-shaped fixture with `profile_id` typed/valued as given —
    /// every other column is a fixed, uninteresting happy-path value. Shared by every
    /// `profile_id`-variant test below.
    fn write_one_row_players_fixture(path: &Path, profile_id_field: Field, profile_id: ArrayRef) {
        let schema = Arc::new(Schema::new(vec![
            Field::new("game_id", DataType::Utf8, true),
            Field::new("civ", DataType::Utf8, true),
            profile_id_field,
            Field::new("winner", DataType::Boolean, true),
            Field::new("opening", DataType::Utf8, true),
            Field::new("feudal_age_uptime", DataType::Float64, true),
            Field::new("castle_age_uptime", DataType::Float64, true),
            Field::new("imperial_age_uptime", DataType::Float64, true),
            Field::new("new_rating", DataType::Int64, true),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![Some("1001")])),
                Arc::new(StringArray::from(vec![Some("britons")])),
                profile_id,
                Arc::new(BooleanArray::from(vec![Some(true)])),
                Arc::new(StringArray::from(vec![None::<&str>])),
                Arc::new(Float64Array::from(vec![None::<f64>])),
                Arc::new(Float64Array::from(vec![None::<f64>])),
                Arc::new(Float64Array::from(vec![None::<f64>])),
                Arc::new(Int64Array::from(vec![Some(1500i64)])),
            ],
        )
        .expect("build one-row players RecordBatch");

        let file = File::create(path).expect("create one-row players fixture file");
        let mut writer = ArrowWriter::try_new(file, schema, None).expect("build ArrowWriter");
        writer.write(&batch).expect("write one-row players batch");
        writer.close().expect("close ArrowWriter");
    }

    #[test]
    fn read_players_accepts_int32_profile_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("p_int32.parquet");
        write_one_row_players_fixture(
            &path,
            Field::new("profile_id", DataType::Int32, true),
            Arc::new(Int32Array::from(vec![Some(5001i32)])),
        );

        let rows = read_players(&path).expect("read_players must accept Int32 profile_id");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].profile_id, Some(5001.0));
    }

    #[test]
    fn read_players_accepts_int64_profile_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("p_int64.parquet");
        write_one_row_players_fixture(
            &path,
            Field::new("profile_id", DataType::Int64, true),
            Arc::new(Int64Array::from(vec![Some(5001i64)])),
        );

        let rows = read_players(&path).expect("read_players must accept Int64 profile_id");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].profile_id, Some(5001.0));
    }

    /// [`read_player_age_sources`] shares [`read_profile_id`] with [`read_players`], but is a
    /// separate code path (separate projection/loop) — this is the "all 136 ages failures were
    /// this" case from the task brief, so it gets its own direct coverage rather than relying on
    /// `read_players`'s test to stand in for it.
    #[test]
    fn read_player_age_sources_accepts_int64_profile_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("p_ages_int64.parquet");
        let schema = Arc::new(Schema::new(vec![
            Field::new("game_id", DataType::Utf8, true),
            Field::new("civ", DataType::Utf8, true),
            Field::new("profile_id", DataType::Int64, true),
            Field::new("winner", DataType::Boolean, true),
            Field::new("replay_summary_raw", DataType::Utf8, true),
        ]));
        let long_blob = "x".repeat(60);
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![Some("1001")])),
                Arc::new(StringArray::from(vec![Some("britons")])),
                Arc::new(Int64Array::from(vec![Some(5001i64)])),
                Arc::new(BooleanArray::from(vec![Some(true)])),
                Arc::new(StringArray::from(vec![Some(long_blob.as_str())])),
            ],
        )
        .expect("build RecordBatch");
        let file = File::create(&path).expect("create fixture file");
        let mut writer = ArrowWriter::try_new(file, schema, None).expect("build ArrowWriter");
        writer.write(&batch).expect("write batch");
        writer.close().expect("close ArrowWriter");

        let rows = read_player_age_sources(&path)
            .expect("read_player_age_sources must accept Int64 profile_id");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].profile_id, Some(5001.0));
    }

    #[test]
    fn read_players_rejects_a_genuinely_unexpected_profile_id_type() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("p_bad_profile_id.parquet");
        write_one_row_players_fixture(
            &path,
            Field::new("profile_id", DataType::Utf8, true),
            Arc::new(StringArray::from(vec![Some("5001")])),
        );

        let err = read_players(&path).expect_err(
            "a profile_id typed as neither Float64/Int32/Int64 must still fail loud — widening \
             the accepted set must not remove the guard",
        );
        assert!(matches!(err, AoestatsError::UnexpectedColumnType { .. }));
    }

    /// Writes a one-row `p_*.parquet`-shaped fixture with `opening`/`feudal_age_uptime`/
    /// `castle_age_uptime`/`imperial_age_uptime` typed/valued as given — `profile_id` is a fixed
    /// `Int64` (the shape every real file carrying this drift also has), matching the module doc's
    /// confirmed pairing. Shared by the `opening`/uptime-variant tests below.
    fn write_players_fixture_with_opening_and_uptime(
        path: &Path,
        opening_field: Field,
        opening: ArrayRef,
        feudal_field: Field,
        feudal: ArrayRef,
    ) {
        let schema = Arc::new(Schema::new(vec![
            Field::new("game_id", DataType::Utf8, true),
            Field::new("civ", DataType::Utf8, true),
            Field::new("profile_id", DataType::Int64, true),
            Field::new("winner", DataType::Boolean, true),
            opening_field,
            feudal_field,
            Field::new("castle_age_uptime", DataType::Float64, true),
            Field::new("imperial_age_uptime", DataType::Float64, true),
            Field::new("new_rating", DataType::Int64, true),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![Some("1001")])),
                Arc::new(StringArray::from(vec![Some("britons")])),
                Arc::new(Int64Array::from(vec![Some(5001i64)])),
                Arc::new(BooleanArray::from(vec![Some(true)])),
                opening,
                feudal,
                Arc::new(Float64Array::from(vec![Some(700.0)])),
                Arc::new(Float64Array::from(vec![Some(900.0)])),
                Arc::new(Int64Array::from(vec![Some(1500i64)])),
            ],
        )
        .expect("build RecordBatch");

        let file = File::create(path).expect("create fixture file");
        let mut writer = ArrowWriter::try_new(file, schema, None).expect("build ArrowWriter");
        writer.write(&batch).expect("write batch");
        writer.close().expect("close ArrowWriter");
    }

    /// The real 2024-03+ drift (see the module doc): `opening`/`feudal_age_uptime` typed as Arrow's
    /// `Null` (the whole column absent for the file), not a semantic change to an integer code —
    /// this test exists specifically because a mid-task review floated the (incorrect) integer-code
    /// theory, and this is the archive-derived disproof plus the regression guard.
    #[test]
    fn read_players_treats_null_typed_opening_and_feudal_uptime_as_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("p_null_opening_feudal.parquet");
        write_players_fixture_with_opening_and_uptime(
            &path,
            Field::new("opening", DataType::Null, true),
            Arc::new(NullArray::new(1)),
            Field::new("feudal_age_uptime", DataType::Null, true),
            Arc::new(NullArray::new(1)),
        );

        let rows = read_players(&path)
            .expect("read_players must accept a Null-typed opening/feudal_age_uptime column");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].opening, None, "Null-typed opening decodes to None");
        assert_eq!(
            rows[0].feudal_age_uptime, None,
            "Null-typed feudal_age_uptime decodes to None"
        );
        // castle/imperial are still their original Float64 type with real values in this fixture —
        // proves the fix is per-column, not "the whole row goes empty once any column goes Null".
        assert_eq!(rows[0].castle_age_uptime, Some(700.0));
        assert_eq!(rows[0].imperial_age_uptime, Some(900.0));
    }

    #[test]
    fn read_players_rejects_a_genuinely_unexpected_opening_type() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("p_bad_opening.parquet");
        write_players_fixture_with_opening_and_uptime(
            &path,
            Field::new("opening", DataType::Int64, true),
            Arc::new(Int64Array::from(vec![Some(7i64)])),
            Field::new("feudal_age_uptime", DataType::Float64, true),
            Arc::new(Float64Array::from(vec![Some(300.0)])),
        );

        let err = read_players(&path).expect_err(
            "an opening typed as Int64 (never observed in the real archive — only Utf8/Null) \
             must still fail loud, not be silently reinterpreted as a numeric code",
        );
        assert!(matches!(err, AoestatsError::UnexpectedColumnType { .. }));
    }

    #[test]
    fn read_players_rejects_a_genuinely_unexpected_uptime_type() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("p_bad_uptime.parquet");
        write_players_fixture_with_opening_and_uptime(
            &path,
            Field::new("opening", DataType::Utf8, true),
            Arc::new(StringArray::from(vec![Some("scouts")])),
            Field::new("feudal_age_uptime", DataType::Utf8, true),
            Arc::new(StringArray::from(vec![Some("300.5")])),
        );

        let err = read_players(&path).expect_err(
            "a feudal_age_uptime typed as Utf8 (never observed — only Float64/Null) must still \
             fail loud, widening the accepted set must not remove the guard",
        );
        assert!(matches!(err, AoestatsError::UnexpectedColumnType { .. }));
    }

    /// Writes a one-row `m_*.parquet`-shaped fixture with `started_timestamp` typed/valued as
    /// given — every other column is a fixed, uninteresting happy-path value (`leaderboard` is
    /// `None` in one variant below, so it's a parameter too). Shared by every
    /// `started_timestamp`-variant test.
    fn write_one_row_matches_fixture(
        path: &Path,
        ts_field: Field,
        ts_array: ArrayRef,
        leaderboard: Option<&str>,
    ) {
        let schema = Arc::new(Schema::new(vec![
            Field::new("map", DataType::Utf8, true),
            ts_field,
            Field::new("duration", DataType::Duration(TimeUnit::Nanosecond), true),
            Field::new("game_id", DataType::Utf8, true),
            Field::new("num_players", DataType::Int64, true),
            Field::new("leaderboard", DataType::Utf8, true),
            Field::new("patch", DataType::Int64, true),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![Some("arabia")])),
                ts_array,
                Arc::new(DurationNanosecondArray::from(vec![Some(
                    1_800_000_000_000i64,
                )])),
                Arc::new(StringArray::from(vec![Some("2001")])),
                Arc::new(Int64Array::from(vec![Some(2i64)])),
                Arc::new(StringArray::from(vec![leaderboard])),
                Arc::new(Int64Array::from(vec![Some(101i64)])),
            ],
        )
        .expect("build one-row matches RecordBatch");

        let file = File::create(path).expect("create one-row matches fixture file");
        let mut writer = ArrowWriter::try_new(file, schema, None).expect("build ArrowWriter");
        writer.write(&batch).expect("write one-row matches batch");
        writer.close().expect("close ArrowWriter");
    }

    #[test]
    fn read_matches_accepts_nanosecond_started_timestamp() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("m_ns.parquet");
        let ts = Utc
            .with_ymd_and_hms(2023, 12, 10, 3, 41, 40)
            .unwrap()
            .timestamp_nanos_opt()
            .expect("timestamp fits i64 nanos");
        write_one_row_matches_fixture(
            &path,
            Field::new(
                "started_timestamp",
                DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into())),
                true,
            ),
            Arc::new(TimestampNanosecondArray::from(vec![Some(ts)]).with_timezone("UTC")),
            Some("random_map"),
        );

        let rows =
            read_matches(&path).expect("read_matches must accept a Nanosecond started_timestamp");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].started_timestamp,
            Some(DateTime::from_timestamp_nanos(ts))
        );
    }

    #[test]
    fn read_matches_accepts_microsecond_started_timestamp() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("m_us.parquet");
        let ts = Utc
            .with_ymd_and_hms(2022, 8, 28, 3, 41, 40)
            .unwrap()
            .timestamp_micros();
        write_one_row_matches_fixture(
            &path,
            Field::new(
                "started_timestamp",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                true,
            ),
            Arc::new(TimestampMicrosecondArray::from(vec![Some(ts)]).with_timezone("UTC")),
            Some("random_map"),
        );

        let rows = read_matches(&path)
            .expect("read_matches must accept the original Microsecond started_timestamp");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].started_timestamp,
            Some(DateTime::from_timestamp_micros(ts).unwrap())
        );
    }

    /// `leaderboard` becoming NULL was this task's original hypothesis for the `ladder` schema
    /// drift (the real archive-wide scan found it's never actually NULL — see `db`'s module doc —
    /// but a NULL cell must still round-trip cleanly through the Rust reader regardless, since the
    /// `db` layer's defensive `num_players` fallback depends on it not erroring here).
    #[test]
    fn read_matches_tolerates_a_null_leaderboard() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("m_null_leaderboard.parquet");
        let ts = Utc
            .with_ymd_and_hms(2023, 1, 29, 0, 0, 0)
            .unwrap()
            .timestamp_micros();
        write_one_row_matches_fixture(
            &path,
            Field::new(
                "started_timestamp",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                true,
            ),
            Arc::new(TimestampMicrosecondArray::from(vec![Some(ts)]).with_timezone("UTC")),
            None,
        );

        let rows = read_matches(&path).expect("read_matches must not error on a NULL leaderboard");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].leaderboard, None);
    }

    #[test]
    fn read_matches_rejects_a_genuinely_unexpected_started_timestamp_type() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("m_bad_timestamp.parquet");
        write_one_row_matches_fixture(
            &path,
            Field::new("started_timestamp", DataType::Int64, true),
            Arc::new(Int64Array::from(vec![Some(123i64)])),
            Some("random_map"),
        );

        let err = read_matches(&path).expect_err(
            "a started_timestamp typed as a bare Int64 (not any Timestamp unit) must still fail \
             loud — widening the accepted set must not remove the guard",
        );
        assert!(matches!(err, AoestatsError::UnexpectedColumnType { .. }));
    }

    /// A genuinely-unknown SCHEMA (an expected column entirely absent, e.g. a hypothetical future
    /// rename) must keep failing loud too — the widening in this task only adds recognized
    /// historical variants, it doesn't relax the "every expected column must exist" contract.
    #[test]
    fn read_matches_rejects_a_file_missing_an_expected_column() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("m_missing_patch.parquet");
        // Every MATCH_COLUMNS entry except `patch`.
        let schema = Arc::new(Schema::new(vec![
            Field::new("map", DataType::Utf8, true),
            Field::new(
                "started_timestamp",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                true,
            ),
            Field::new("duration", DataType::Duration(TimeUnit::Nanosecond), true),
            Field::new("game_id", DataType::Utf8, true),
            Field::new("num_players", DataType::Int64, true),
            Field::new("leaderboard", DataType::Utf8, true),
        ]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![Some("arabia")])),
                Arc::new(TimestampMicrosecondArray::from(vec![Some(0i64)]).with_timezone("UTC")),
                Arc::new(DurationNanosecondArray::from(vec![Some(0i64)])),
                Arc::new(StringArray::from(vec![Some("2001")])),
                Arc::new(Int64Array::from(vec![Some(2i64)])),
                Arc::new(StringArray::from(vec![Some("random_map")])),
            ],
        )
        .expect("build RecordBatch");
        let file = File::create(&path).expect("create fixture file");
        let mut writer = ArrowWriter::try_new(file, schema, None).expect("build ArrowWriter");
        writer.write(&batch).expect("write batch");
        writer.close().expect("close ArrowWriter");

        let err = read_matches(&path)
            .expect_err("a file missing an entire expected column must still fail loud");
        assert!(matches!(err, AoestatsError::MissingColumn { .. }));
    }

    #[test]
    fn canonical_aoestats_map_slug_fixes_the_two_known_mismatches() {
        assert_eq!(canonical_aoestats_map_slug("scandanavia"), "scandinavia");
        assert_eq!(canonical_aoestats_map_slug("passage"), "thepassage");
    }

    #[test]
    fn canonical_aoestats_map_slug_passes_through_an_unrelated_slug_unchanged() {
        assert_eq!(canonical_aoestats_map_slug("arabia"), "arabia");
        assert_eq!(canonical_aoestats_map_slug("coastalforest"), "coastalforest");
    }

    #[test]
    fn read_matches_applies_the_map_alias_after_slug() {
        // Unlike `write_one_row_matches_fixture` (which hard-codes `map = "arabia"`), this builds
        // its own one-row batch with the raw, pre-slug aoestats value `"scandanavia"` — proving
        // `read_matches` itself (not just the standalone `canonical_aoestats_map_slug` fn above)
        // applies the alias end to end.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("m_scandanavia.parquet");
        let schema = Arc::new(Schema::new(vec![
            Field::new("map", DataType::Utf8, true),
            Field::new(
                "started_timestamp",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                true,
            ),
            Field::new("duration", DataType::Duration(TimeUnit::Nanosecond), true),
            Field::new("game_id", DataType::Utf8, true),
            Field::new("num_players", DataType::Int64, true),
            Field::new("leaderboard", DataType::Utf8, true),
            Field::new("patch", DataType::Int64, true),
        ]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![Some("scandanavia")])),
                Arc::new(TimestampMicrosecondArray::from(vec![Some(0i64)]).with_timezone("UTC")),
                Arc::new(DurationNanosecondArray::from(vec![Some(0i64)])),
                Arc::new(StringArray::from(vec![Some("2001")])),
                Arc::new(Int64Array::from(vec![Some(2i64)])),
                Arc::new(StringArray::from(vec![Some("random_map")])),
                Arc::new(Int64Array::from(vec![Some(101i64)])),
            ],
        )
        .expect("build one-row matches RecordBatch");
        let file = File::create(&path).expect("create fixture file");
        let mut writer = ArrowWriter::try_new(file, schema, None).expect("build ArrowWriter");
        writer.write(&batch).expect("write batch");
        writer.close().expect("close ArrowWriter");

        let rows = read_matches(&path).expect("read_matches on the fixture file");
        assert_eq!(rows[0].map.as_deref(), Some("scandinavia"));
    }
}
