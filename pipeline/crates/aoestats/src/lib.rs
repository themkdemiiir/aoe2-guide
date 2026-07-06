//! `aoestats` — idempotent binary-`COPY` bulk loader: the aoestats parquet archive (matches +
//! players) into the AOE2 guide's live PostgreSQL schema, as `matches.source = 'aoestats'`.
//!
//! This is the historical-corpus counterpart to `ingest` (which loads replay-parsed batches).
//! Unlike `ingest`'s caller, which already resolves `map_id`/`civ_id` in Rust before COPY, this
//! crate stages the RAW aoestats `map`/`civ` slugs as TEXT and resolves them via a SQL `JOIN`
//! against the live `maps`/`civs` dims (see `db`'s module doc) — there is no Rust-side refdata
//! lookup here, so `pipeline-core` is pulled in with `default-features = false`.
//!
//! Ages (`replay_summary_raw`) are explicitly OUT of scope — that's task M4b.

pub mod db;
pub mod error;
pub mod parquet_read;

pub use db::{import_pair, ImportStats, UnknownSlugCounts};
pub use error::{AoestatsError, Result};
pub use parquet_read::{read_matches, read_players, RawMatchRow, RawPlayerRow};
