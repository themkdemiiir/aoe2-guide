//! `aoestats` — idempotent binary-`COPY` bulk loader: the aoestats parquet archive (matches +
//! players) into the AOE2 guide's live PostgreSQL schema, as `matches.source = 'aoestats'`.
//!
//! This is the historical-corpus counterpart to `ingest` (which loads replay-parsed batches).
//! Unlike `ingest`'s caller, which already resolves `map_id`/`civ_id` in Rust before COPY, this
//! crate stages the RAW aoestats `map`/`civ` slugs as TEXT and resolves them via a SQL `JOIN`
//! against the live `maps`/`civs` dims (see `db`'s module doc) — there is no Rust-side refdata
//! lookup here, so `pipeline-core` is pulled in with `default-features = false`.
//!
//! Ages (`replay_summary_raw`, Task M4b, [`import_ages`]) are a separate path over the SAME
//! `p_*.parquet` file, into a separate `match_ages` table — see `db`'s module doc. Parsing
//! `replay_summary_raw`'s Python-repr blob is the one place this whole pipeline shells out to
//! Python (`crate::py`, `pipeline/py/aoestats_summaries.py`) rather than staying pure Rust: no
//! crate parses Python-repr (single-quoted, `True`/`False`/`None`) cleanly, and re-deriving
//! `ast.literal_eval` in Rust is out of scope for what this is — a one-time historical-archive
//! import, not a hot path.

pub mod db;
pub mod error;
pub mod parquet_read;
pub mod py;

pub use db::{import_ages, import_pair, AgesImportStats, ImportStats, UnknownSlugCounts};
pub use error::{AoestatsError, Result};
pub use parquet_read::{
    read_matches, read_player_age_sources, read_players, RawAgeSourceRow, RawMatchRow,
    RawPlayerRow,
};
pub use py::{AgeOutputRow, AgeSourceRow};
