//! Typed serde mirror of the committed `src/data/civ-meta.json` — field-for-field, in the SAME
//! declared order (serde_json serializes struct fields in declaration order), so
//! `serde_json::to_string_pretty` on [`CivMetaDoc`] reproduces the committed file's shape exactly.
//!
//! **This is the M5a parity proof, not just a DTO:** `tests/shape_parity.rs` deserializes the REAL
//! committed `src/data/civ-meta.json` into this exact type. If that succeeds, every key this
//! struct declares is present in the real file with a compatible type — the strongest, most
//! direct statement of "same shape" available (one type, exercised in both directions: serialized
//! from this crate's own DB-sourced rows, and deserialized from the legacy JS generator's output).
//!
//! No field uses `#[serde(skip_serializing_if = ...)]` anywhere in this module: every key the
//! committed file has must always be PRESENT in this crate's output (as `null` where there's no
//! data, never omitted) — shape parity means the key survives, not that empty data hides it.
//!
//! Deliberate scope note (documented in the task report): the values populated here come from the
//! new Postgres-backed corpus (currently one aoestats sample day — see the M5a brief's "parity
//! model"), so counts/rates differ from the committed file's — that is the accepted, by-design
//! trade-off. Only the SHAPE is asserted equal.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The whole `civ-meta.json` document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CivMetaDoc {
    pub source: String,
    pub ladders: Vec<String>,
    /// `YYYY-MM-DD`.
    pub generated: String,
    #[serde(rename = "sourceDate")]
    pub source_date: String,
    pub appearances: Appearances,
    pub patches: Vec<PatchEntry>,
    #[serde(rename = "eloBuckets")]
    pub elo_buckets: Vec<String>,
    #[serde(rename = "tierMethod")]
    pub tier_method: String,
    /// Keyed by civ slug (`"franks"`, `"britons"`, ...) — a data-keyed map, not a fixed field set;
    /// see `shape::DYNAMIC_MAP_FIELDS`.
    pub civs: BTreeMap<String, CivEntry>,
    #[serde(rename = "patchSource")]
    pub patch_source: String,
}

/// Per-ladder total appearance counts (`match_players` rows across every civ in that ladder).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Appearances {
    #[serde(rename = "1v1")]
    pub ladder_1v1: u64,
    pub team: u64,
}

/// One entry of the top-level `patches` axis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchEntry {
    /// The build id, as a decimal string (matches the committed file: `"179158"`, not `179158`).
    pub patch: String,
    pub label: String,
    pub matches: u64,
}

/// One civilization's stats, split by ladder. Either ladder is `None` (serializes as JSON `null`,
/// never an omitted key) when that civ has no usable sample in this ladder — a real, documented
/// state the committed file itself uses for post-freeze-archive DLC civs (see
/// `refresh-civ-current.mjs`'s module doc).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CivEntry {
    #[serde(rename = "1v1")]
    pub ladder_1v1: Option<LadderStats>,
    pub team: Option<LadderStats>,
}

/// One (civ, ladder) stats block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LadderStats {
    pub games: u64,
    #[serde(rename = "winRate")]
    pub win_rate: f64,
    /// `[low, high]`, both percent (0..100).
    pub ci95: [f64; 2],
    #[serde(rename = "playRate")]
    pub play_rate: f64,
    /// `None` -> JSON `null`: "no data" per `tierMethod`'s own documented contract, never
    /// fabricated.
    pub tier: Option<String>,
    #[serde(rename = "byPatch")]
    pub by_patch: BTreeMap<String, BucketStats>,
    #[serde(rename = "byElo")]
    pub by_elo: BTreeMap<String, BucketStats>,
    #[serde(rename = "byMap")]
    pub by_map: BTreeMap<String, BucketStats>,
    pub openings: Vec<Opening>,
    #[serde(rename = "ageUp")]
    pub age_up: Option<AgeUp>,
}

/// One slice's games/winRate — the shared shape of `byPatch`/`byElo`/`byMap` entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketStats {
    pub games: u64,
    #[serde(rename = "winRate")]
    pub win_rate: f64,
}

/// One entry of the top-3 `openings` list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opening {
    pub opening: String,
    pub share: f64,
}

/// Median seconds-to-age-up, per age.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgeUp {
    pub feudal: i64,
    pub castle: i64,
    pub imperial: i64,
}
