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
//! Postgres-backed corpus (the full aoestats.io ranked archive, all-time), so counts/rates differ
//! from whatever was committed before a given regen — that is the accepted, by-design trade-off.
//! Only the SHAPE is asserted equal.

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

// --- eco benchmark (winner-focused eco-upgrade timing) — typed mirror of `benchmark-eco.json` ----

/// One `(tech, elo_bucket, mode)` grain's WINNER-focused first-research click-time percentiles, in
/// SECONDS. The analyzer draws the p25–p75 band around the p50 median. `n` is the winner sample
/// size behind the cell (kept so the UI can honestly gate a thin slice).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EcoBand {
    pub p25_s: f64,
    pub p50_s: f64,
    pub p75_s: f64,
    pub n: u64,
}

/// `techs.<techId>.<mapSlug>.<eloBucket>`, the innermost (mode-keyed) map of [`EcoBenchmarkDoc`].
pub type EcoModeMap = BTreeMap<String, EcoBand>;

/// The whole `benchmark-eco.json` document: `techs.<techId>.<mapSlug>.<eloBucket>.<mode> = EcoBand`,
/// `"all"`-keyed for the map-rollup and elo-rollup grains (see `benchmark_ecotech.sql`). Map-keyed
/// so the analyzer compares a player only to winners at their elo AND on their map (eco pace is
/// map-driven — FC-on-Arena vs. fighting Arabia). `techId` is the decimal AoE2 tech id as a string
/// (`"22"` = Loom, ...) — the analyzer's `WATCHED_TECHS` is the id↔name authority, so this file
/// carries ids only, never names.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EcoBenchmarkDoc {
    #[serde(rename = "_source")]
    pub source: String,
    pub techs: BTreeMap<String, BTreeMap<String, BTreeMap<String, EcoModeMap>>>,
}

// --- winner comps (Phase E) — typed mirror of the committed `public/winner-comps.json` ---------

/// One unit entry of a `winner-comps.json` cell's `units` list — one of the (at most 6) military
/// units, ordered by producer share descending, that WINNERS of that `(civ, elo_bucket)` cell
/// actually trained. `med` is a true statistical median (`percentile_cont`), so it is NOT always
/// integral — an even producer count yields a `x.5` value (e.g. `23.5`), exactly like the
/// committed file's own real cells.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WinnerCompUnit {
    pub unit: String,
    pub pct: f64,
    pub med: f64,
}

/// One `(civ, elo_bucket)` cell: how many winners are behind it, plus its top-6 unit list (see
/// [`WinnerCompUnit`]). A cell exists at all only once it clears `winner_comps.sql`'s own
/// thresholds (>=100 winners, unit produced by >=15% of them) — a thin combination is simply
/// absent from `civs`, never a fabricated empty cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WinnerCompsCell {
    pub winners: u64,
    pub units: Vec<WinnerCompUnit>,
}

/// `civs.<civSlug>.<eloBucket>`, the inner map of [`WinnerCompsDoc`]. Real elo-bucket keys only
/// (`pipeline_core::elo::ELO_BUCKETS`) — unlike `benchmark.json`/`benchmark-eco.json`, this
/// document has no `"all"` elo rollup (the source view emits none — see its doc).
pub type WinnerCompsEloMap = BTreeMap<String, WinnerCompsCell>;

/// The whole `winner-comps.json` document: `civs.<civSlug>.<eloBucket> = WinnerCompsCell`. Fed by
/// `pipeline/dbt/models/winner_comps.sql` — see that view's doc for the full aggregation and its
/// documented scope deviation (replay-source only, whole-match unit totals) from the old
/// `build-winner-comps.mjs`/`.sql` generator this replaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WinnerCompsDoc {
    pub source: String,
    /// `YYYY-MM-DD`, same convention as [`CivMetaDoc::generated`].
    pub generated: String,
    pub civs: BTreeMap<String, WinnerCompsEloMap>,
}

// --- civ cube (dict-indexed civ x elo x map x build cube) — typed mirror of the committed
// `public/civ-cube.json` + `src/data/civ-cube-dims.json` ----------------------------------------
//
// See `pipeline/dbt/models/civ_cube.sql` and `pipeline/crates/export/src/civ_cube.rs` for the
// aggregation + dict-indexing this pair of documents is built from.

/// One entry of the `months` (patch/build) axis — `{patch, label}` only, unlike [`PatchEntry`]:
/// the cube's own axis carries no global match-count (that lives on `civ-meta.json`'s `patches`,
/// the exact list this one is filtered FROM — see `civ_cube.rs`'s doc).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CivCubeMonth {
    pub patch: String,
    pub label: String,
}

/// `[civIndex, eloIndex, mapIndex, monthIndex, games, wins]` — one packed cube cell. A `(u32, u32,
/// u32, u32, u64, u64)` tuple serializes as the committed file's own 6-element JSON array (serde
/// tuple serialization = declaration order), the same packing convention [`EloWinRateGames`] uses
/// for its own 2-element array.
pub type CivCubeRowPacked = (u32, u32, u32, u32, u64, u64);

/// The whole `public/civ-cube.json` document: dictionary-indexed `civ x elo_bucket x map x build`
/// cells kept as a flat array of small-int-indexed rows (not a nested map) to stay small over the
/// wire — see `civ_cube.rs`'s doc for exactly how `rows` is packed against `civs`/`elos`/`maps`/
/// `months`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CivCubeDoc {
    /// `YYYY-MM-DD`.
    pub generated: String,
    pub source: String,
    /// Sorted, DATA-driven (only civs with a qualifying cell after the patch-axis filter) — unlike
    /// `civ-meta.json`'s `civs` map, this is NOT "every known civ gets an entry" (see
    /// `civ_cube.rs`'s doc).
    pub civs: Vec<String>,
    /// The fixed nine-bucket vocabulary (`pipeline_core::elo::ELO_BUCKETS`), unconditional — never
    /// filtered by what's actually present in `rows`.
    pub elos: Vec<String>,
    /// Sorted by total games descending (most-played first), ties broken alphabetically for
    /// determinism.
    pub maps: Vec<String>,
    #[serde(rename = "mapNames")]
    pub map_names: Vec<String>,
    pub months: Vec<CivCubeMonth>,
    pub rows: Vec<CivCubeRowPacked>,
}

/// `src/data/civ-cube-dims.json`: the tiny dropdown-only sibling of [`CivCubeDoc`] — deliberately
/// NO `civs` (the `/civs` page already has its own civ list) and NO `rows`; imported at build time
/// so the page's filter dropdowns exactly match the cube without fetching the full file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CivCubeDimsDoc {
    pub generated: String,
    pub elos: Vec<String>,
    pub maps: Vec<String>,
    #[serde(rename = "mapNames")]
    pub map_names: Vec<String>,
    pub months: Vec<CivCubeMonth>,
}

// --- winner refs (analyzer coaching-panel reference matrix) — typed mirror of the committed
// `public/winner-refs.json` ------------------------------------------------------------------
//
// See `pipeline/dbt/models/winner_refs_openings.sql`/`winner_refs_ecotech.sql`/
// `winner_refs_meds.sql` and `pipeline/crates/export/src/winner_refs.rs` for the three
// aggregations this document assembles. THREE independently-scoped parts, not one corpus:
// `openings` is FULL-corpus (both `aoestats`- and `replay`-sourced matches, same posture
// `civ_meta_openings.sql` already established); `ecoTechByCastle`/`medsByBucket`/`medsByMap` are
// REPLAY-SOURCE ONLY (`match_player_techs`/`replay_events` respectively — neither table is ever
// populated from the aoestats archive path) — see `winner_refs.rs`'s module doc for the full
// per-part coverage note.

/// One opening entry of a `winner-refs.json` `openings` cell's `list` — the closed `opening_kind`
/// vocabulary (see `winner_refs_openings.sql`'s doc), NOT the old free-text `opening` column's
/// `"unknown"` pseudo-strategy the legacy aoestats-only generator used to rank (a documented scope
/// change — see that view's doc point 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WinnerOpening {
    pub opening: String,
    pub pct: f64,
}

/// One `(civ, elo_bucket)` opening cell: how many winners are behind it, plus its top-5 openings
/// by share (see [`WinnerOpening`]) — the same "rank in the view, cap in Rust" split
/// [`WinnerCompsCell`] uses for its own top-6 unit list.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WinnerOpeningsCell {
    pub winners: u64,
    pub list: Vec<WinnerOpening>,
}

/// `openings.<civSlug>.<eloBucket>`, the inner map of [`WinnerRefsDoc::openings`].
pub type WinnerOpeningsEloMap = BTreeMap<String, WinnerOpeningsCell>;

/// One `elo_bucket` cell of `ecoTechByCastle`: how many winners reached Castle Age (the
/// denominator), plus the share of THEM who researched each watched eco upgrade BEFORE completing
/// Castle Age. All five percentages are always populated once a cell exists (never `null` — see
/// `winner_refs_ecotech.sql`'s doc: the view's own `HAVING count(*) >= 50` guarantees a non-zero
/// denominator for every emitted row).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EcoTechByCastleCell {
    pub winners: u64,
    pub wheelbarrow: f64,
    pub loom: f64,
    pub dba: f64,
    #[serde(rename = "horseCollar")]
    pub horse_collar: f64,
    #[serde(rename = "goldMining")]
    pub gold_mining: f64,
}

/// One elo/map cell of `medsByBucket`/`medsByMap`: winner first-military-training + eco-tech
/// research-CLICK-time MEDIANS, in milliseconds. Each `*_ms` field is `Option<i64>` (JSON `null`
/// when absent), NOT a bare `i64` — `winner_refs_meds.sql`'s `percentile_cont` is null whenever
/// none of that cell's winners triggered the corresponding event at all, a real "no signal" state
/// this document must be able to represent honestly rather than fabricate a value for (see
/// `winner_refs.rs`'s doc). The committed file's real cells happen to have no nulls today (the
/// current sample clears every column), but the type does not assume that stays true forever.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WinnerMedsCell {
    pub winners: u64,
    #[serde(rename = "firstMilitaryMs")]
    pub first_military_ms: Option<i64>,
    #[serde(rename = "loomMs")]
    pub loom_ms: Option<i64>,
    #[serde(rename = "dbaMs")]
    pub dba_ms: Option<i64>,
    #[serde(rename = "wheelbarrowMs")]
    pub wheelbarrow_ms: Option<i64>,
    #[serde(rename = "horseCollarMs")]
    pub horse_collar_ms: Option<i64>,
}

/// `medsByMap.<mapSlug>.<eloBucket>`, the inner map of [`WinnerRefsDoc::meds_by_map`].
pub type WinnerMedsEloMap = BTreeMap<String, WinnerMedsCell>;

/// The whole `winner-refs.json` document — the analyzer's coaching-panel "You | Opponent |
/// Reference" matrix, fetched lazily. `openings` is keyed `<civSlug>.<eloBucket>` (the committed
/// file's own top-level key, kept verbatim — unlike [`WinnerCompsDoc`], there is no wrapping
/// `civs` field here); `ecoTechByCastle`/`medsByBucket` are keyed by `<eloBucket>` alone (no
/// civ/map split); `medsByMap` adds one more level, `<mapSlug>.<eloBucket>`. See
/// `winner_refs.rs`'s module doc for which of these four are full-corpus vs. replay-source-only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WinnerRefsDoc {
    pub source: String,
    /// `YYYY-MM-DD`, same convention as [`WinnerCompsDoc::generated`].
    pub generated: String,
    pub openings: BTreeMap<String, WinnerOpeningsEloMap>,
    #[serde(rename = "ecoTechByCastle")]
    pub eco_tech_by_castle: BTreeMap<String, EcoTechByCastleCell>,
    #[serde(rename = "medsByBucket")]
    pub meds_by_bucket: BTreeMap<String, WinnerMedsCell>,
    #[serde(rename = "medsByMap")]
    pub meds_by_map: BTreeMap<String, WinnerMedsEloMap>,
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
