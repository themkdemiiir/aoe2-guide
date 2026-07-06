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
#[serde(deny_unknown_fields)]
pub struct CivMetaDoc {
    pub source: String,
    pub ladders: Vec<String>,
    /// `YYYY-MM-DD`.
    pub generated: String,
    /// `YYYY-MM` of the newest `played_at` in the corpus (`query::fetch_source_date`) — `None`
    /// (JSON `null`) on a genuinely empty `matches` table, never a fabricated sentinel string
    /// (see that function's doc + the "no defaults, fail loud" rule).
    #[serde(rename = "sourceDate")]
    pub source_date: Option<String>,
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
#[serde(deny_unknown_fields)]
pub struct Appearances {
    #[serde(rename = "1v1")]
    pub ladder_1v1: u64,
    pub team: u64,
}

/// One entry of the top-level `patches` axis.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct CivEntry {
    #[serde(rename = "1v1")]
    pub ladder_1v1: Option<LadderStats>,
    pub team: Option<LadderStats>,
}

/// One (civ, ladder) stats block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct BucketStats {
    pub games: u64,
    #[serde(rename = "winRate")]
    pub win_rate: f64,
}

/// One entry of the top-3 `openings` list.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opening {
    pub opening: String,
    pub share: f64,
}

/// Median seconds-to-age-up, per age.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgeUp {
    pub feudal: i64,
    pub castle: i64,
    pub imperial: i64,
}

// --- matchups (task M5b) — typed mirrors of `civ-matchups*.json` -------------------------------
//
// Field order matches each committed file exactly, same "serde_json::to_string_pretty
// reproduces the committed shape" reasoning as `CivMetaDoc` above.

/// One opponent's games/winRate against a civ — the shared list-item shape of
/// `civ-matchups.json`/`civ-matchups-by-map.json`/`civ-matchups-team.json`'s `civs` entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchupOpponent {
    pub opp: String,
    pub games: u64,
    #[serde(rename = "winRate")]
    pub win_rate: f64,
}

/// `civ-matchups.json` AND `civ-matchups-team.json` (identical shape — only `ladder`/`minGames`/
/// `note`/`civs` content differ at runtime; see `matchups.rs::build_civ_matchups`/
/// `build_civ_matchups_team`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CivMatchupsDoc {
    pub source: String,
    pub generated: String,
    pub ladder: String,
    #[serde(rename = "minGames")]
    pub min_games: u64,
    pub note: String,
    /// Keyed by civ slug — a data-keyed map, not a fixed field set (see `shape::DYNAMIC_MAP_FIELDS`
    /// and its `collapse_dynamic` helper, which this doc's nested `Vec<MatchupOpponent>` entries
    /// specifically motivated — see that function's doc).
    pub civs: BTreeMap<String, Vec<MatchupOpponent>>,
}

/// `civ-matchups-by-map.json`: one more dynamic-keyed level than [`CivMatchupsDoc`] — each civ's
/// entry is itself keyed by map slug, not a fixed struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CivMatchupsByMapDoc {
    pub source: String,
    pub generated: String,
    pub ladder: String,
    #[serde(rename = "minGames")]
    pub min_games: u64,
    pub note: String,
    pub civs: BTreeMap<String, BTreeMap<String, Vec<MatchupOpponent>>>,
}

/// `civ-matchups-by-elo.json`'s `minGames: {bucket, all}` (both currently 30, kept as two named
/// fields — not one constant — because they gate two logically distinct things: a per-bucket cell
/// vs. the elo-agnostic `"all"` rollup cell; see `matchups.rs`'s threshold constants).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EloMinGames {
    pub bucket: u64,
    pub all: u64,
}

/// `[winRate, games]` — a `(f64, u64)` tuple serializes as the committed file's own 2-element JSON
/// array (serde tuple serialization order = declaration order).
pub type EloWinRateGames = (f64, u64);

/// `civs.<civSlug>.<oppSlug>.<eloBucket>`, the innermost map of [`CivMatchupsByEloDoc`].
pub type EloBucketMap = BTreeMap<String, EloWinRateGames>;

/// `civ-matchups-by-elo.json`: `civs.<slug>.<opp> = {<bucket>: [winRate, games], ..., all: [...]}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CivMatchupsByEloDoc {
    pub source: String,
    pub generated: String,
    pub ladder: String,
    #[serde(rename = "eloBuckets")]
    pub elo_buckets: Vec<String>,
    #[serde(rename = "minGames")]
    pub min_games: EloMinGames,
    pub note: String,
    pub civs: BTreeMap<String, BTreeMap<String, EloBucketMap>>,
}

// --- benchmark (task M5b) — typed mirror of `benchmark.json` ------------------------------------

/// One (civ, map, elo_bucket, mode) grain's medians — `vils_castle` is `None` (JSON `null`) on any
/// grain `benchmark_vils` doesn't cover (a real, documented state: the old
/// `build-benchmark-vils.mjs` merged it into a SUBSET of the age-benchmark's own cells, never
/// inventing a value for the rest — see that script's doc and `benchmark.rs`'s doc for why this
/// crate emits `null` rather than omitting the key, unlike the old generator's plain omission).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkCell {
    pub feudal_s: Option<f64>,
    pub castle_s: Option<f64>,
    pub imperial_s: Option<f64>,
    pub vils_castle: Option<f64>,
}

/// `civs.<civSlug>.<mapSlug>.<eloBucket>`, the innermost (mode-keyed) map of [`BenchmarkDoc`].
pub type BenchmarkModeMap = BTreeMap<String, BenchmarkCell>;

/// The whole `benchmark.json` document: `civs.<civSlug>.<mapSlug>.<eloBucket>.<mode> =
/// BenchmarkCell`, `"all"`-keyed at every one of the three inner levels for its rollup grains (see
/// `benchmark_ageup.sql`'s doc for exactly which grain combinations are real).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkDoc {
    #[serde(rename = "_source")]
    pub source: String,
    pub civs: BTreeMap<String, BTreeMap<String, BTreeMap<String, BenchmarkModeMap>>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A committed JSON file gaining a new top-level key this crate hasn't modeled yet must fail
    /// LOUD (a shape-parity test going red), not silently ignore the new key — that silent-ignore
    /// is exactly what plain `#[derive(Deserialize)]` does by default, which is why every
    /// fixed-schema struct above carries `#[serde(deny_unknown_fields)]`. Spot-checked on one
    /// small struct rather than every one of them (the shape-parity tests already exercise every
    /// struct against the real committed files).
    #[test]
    fn deny_unknown_fields_is_actually_wired_on_a_fixed_schema_struct() {
        let json = r#"{ "opening": "scouts", "share": 12.5, "unexpectedNewKey": true }"#;
        let err = serde_json::from_str::<Opening>(json).unwrap_err();
        assert!(
            err.to_string().contains("unknown field"),
            "expected an unknown-field error, got: {err}"
        );
    }

    #[test]
    fn the_known_shape_still_deserializes_fine() {
        let json = r#"{ "opening": "scouts", "share": 12.5 }"#;
        assert!(serde_json::from_str::<Opening>(json).is_ok());
    }
}
