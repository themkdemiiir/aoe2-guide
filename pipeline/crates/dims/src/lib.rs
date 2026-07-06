//! `dims` — idempotently loads the AOE2 guide's reference dimension tables (`maps`, `civs`,
//! `civs_relic`, `patch_index`) from the committed refdata `pipeline_core` (`refdata` feature)
//! already parses. See [`load_dims`]'s doc for the exact algorithm.
//!
//! Every `matches` row FK-references `maps`/`civs` (`match_players.civ_id` -> `civs`,
//! `matches.map_id` -> `maps`), so this loader is a prerequisite for ANY ingest (replay or
//! aoestats) against a fresh schema — the live schema's dimension tables start out empty.
//! `patch_index` has no incoming FK (`matches.build`/`patch` are plain columns), so it is loaded
//! for completeness but is non-blocking for ingest.

mod load;

pub use load::{load_dims, DimsStats};
