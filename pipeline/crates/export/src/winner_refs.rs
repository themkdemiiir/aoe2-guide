//! Assembles [`WinnerRefsDoc`] from `pipeline/dbt`'s `winner_refs_openings`/`winner_refs_ecotech`/
//! `winner_refs_meds` view rows. Pure (no `Client`, no IO) — same fetch/assembly split as
//! `winner_comps.rs`/`civ_cube.rs`/`eco_benchmark.rs`.
//!
//! The analyzer's coaching panel shows a "You | Opponent | Reference" matrix; this document is the
//! "Reference" column's data source — winners' openings, eco-tech pacing, and event-timing medians,
//! sliced by elo (and, for `openings`/`medsByMap`, by civ/map too). It replaces
//! `scripts/data-pipeline/build-winner-refs.mjs` + `build-winner-refs.sql` + `build-winner-meds.sql`
//! (three CSVs nested by the old `.mjs`) with one Postgres pipeline pass over three dbt views.
//!
//! **This document has THREE independently-scoped parts — read this before trusting any coverage
//! number, and see each view's own SQL doc for the full reasoning:**
//! - **`openings` is FULL-corpus.** `winner_refs_openings.sql` reads `match_players.opening_kind`,
//!   populated for BOTH `aoestats`- and `replay`-sourced matches (same posture
//!   `civ_meta_openings.sql` already established) — expect broad civ x elo coverage, growing as the
//!   aoestats archive's `opening_kind` backfill (currently ~15M of ~108M eligible rows) continues.
//! - **`ecoTechByCastle` is REPLAY-SOURCE ONLY, and THIN.** `winner_refs_ecotech.sql` needs
//!   `match_player_techs` (per-tech research CLICK times), a table the aoestats archive path never
//!   populates at all — see that migration's doc. The view scopes its OWN denominator to
//!   `source = 'replay'` too (a deliberate design choice, NOT a copy of `winner_comps.sql`'s
//!   posture — see that view's doc for why), so its `winners_n` honestly reports the (currently
//!   ~194k-match-and-growing) replay corpus's size, not the full archive's.
//! - **`medsByBucket`/`medsByMap` are REPLAY-SOURCE ONLY, and THIN.** `winner_refs_meds.sql` needs
//!   `replay_events` (the raw per-event log), exclusively replay-parsed — same corpus as
//!   `ecoTechByCastle`, same caveat.
//!
//! In other words: this is the SAME shape of deviation `winner_comps.rs`'s own doc documents for
//! `winner-comps.json` (a full-corpus generator replaced by a mix of full- and replay-only-sourced
//! views) — expect `ecoTechByCastle`/`medsByBucket`/`medsByMap` cells to be sparser than the
//! committed file's (which drew from a full-corpus `match_ages.parquet` research-JSON extract that
//! has no equivalent table in this Postgres schema — see `winner_refs_ecotech.sql`'s doc point 3
//! for the `civ_meta_openings.sql`-style detail), while `openings` should be comparably broad.
//! **Don't chase byte-parity with the committed (aoestats-archive) file** — only the SHAPE is
//! asserted equal (`tests/shape_parity_winner_refs.rs`), the same posture every other exporter in
//! this crate takes (see `model.rs`'s own module doc).

use std::collections::BTreeMap;

use crate::model::{
    EcoTechByCastleCell, WinnerMedsCell, WinnerMedsEloMap, WinnerOpening, WinnerOpeningsCell,
    WinnerOpeningsEloMap, WinnerRefsDoc,
};
use crate::query::{WinnerEcotechRow, WinnerMedsRow, WinnerOpeningRow};

const SOURCE: &str = "aoestats.io archive + live replay crawl, unified in the PostgreSQL pipeline \
                       via the dbt winner_refs_openings/winner_refs_ecotech/winner_refs_meds \
                       views. openings = winners' opening_kind distribution per civ x elo (FULL \
                       corpus, share >=8%, >=100 winners); ecoTechByCastle = winners' \
                       eco-upgrade-researched-before-Castle-Age shares per elo (REPLAY-SOURCE \
                       ONLY); medsByBucket/medsByMap = winners' first-military-training + \
                       eco-tech-research CLICK-time medians per elo and per map x elo \
                       (REPLAY-SOURCE ONLY).";

/// `YYYY-MM-DD` — same convention as `winner_comps.rs`'s/`civ_cube.rs`'s own `today()`.
fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

/// Fold the three views' rows into one [`WinnerRefsDoc`]. Each view is independently keyed (see
/// this module's doc for the three parts' differing corpus scope), so the three loops below never
/// interact — a thin/empty `ecotech`/`meds` slice still leaves `openings` fully populated, and vice
/// versa (never an all-or-nothing document).
pub fn build_winner_refs(
    openings: &[WinnerOpeningRow],
    ecotech: &[WinnerEcotechRow],
    meds: &[WinnerMedsRow],
) -> WinnerRefsDoc {
    WinnerRefsDoc {
        source: SOURCE.to_string(),
        generated: today(),
        openings: build_openings(openings),
        eco_tech_by_castle: build_ecotech(ecotech),
        meds_by_bucket: build_meds_by_bucket(meds),
        meds_by_map: build_meds_by_map(meds),
    }
}

/// Rows arrive ordered `(civ_slug, elo_bucket, opening_rank)` (see
/// `query::fetch_winner_openings`'s doc), so a straight per-row push reproduces the intended
/// share-descending `list` order — no re-sort needed, the same reasoning
/// `winner_comps.rs::build_winner_comps` uses for its own `units` list.
fn build_openings(rows: &[WinnerOpeningRow]) -> BTreeMap<String, WinnerOpeningsEloMap> {
    let mut civs: BTreeMap<String, WinnerOpeningsEloMap> = BTreeMap::new();
    for row in rows {
        let cell = civs
            .entry(row.civ_slug.clone())
            .or_default()
            .entry(row.elo_bucket.clone())
            .or_insert_with(|| WinnerOpeningsCell {
                winners: row.winners_n as u64,
                list: Vec::new(),
            });
        cell.list.push(WinnerOpening {
            opening: row.opening.clone(),
            pct: row.share_pct,
        });
    }
    civs
}

/// Every `elo_bucket` grain is unique in `winner_refs_ecotech` (a plain `GROUP BY elo_bucket`, no
/// GROUPING SETS rollup grains to collide with) — a plain `insert` is correct.
fn build_ecotech(rows: &[WinnerEcotechRow]) -> BTreeMap<String, EcoTechByCastleCell> {
    let mut by_bucket = BTreeMap::new();
    for row in rows {
        by_bucket.insert(
            row.elo_bucket.clone(),
            EcoTechByCastleCell {
                winners: row.winners_n as u64,
                wheelbarrow: row.wheelbarrow_pct,
                loom: row.loom_pct,
                dba: row.dba_pct,
                horse_collar: row.horse_collar_pct,
                gold_mining: row.gold_mining_pct,
            },
        );
    }
    by_bucket
}

/// Rounds an optional raw-millisecond `percentile_cont` result to the nearest millisecond —
/// `winner_refs_meds.sql` deliberately leaves its five `*_ms` columns unrounded (see that view's
/// doc), the same "SQL emits raw percentiles, Rust rounds for display" split `eco_benchmark.rs`'s
/// `round1` established for `benchmark_ecotech`'s seconds columns. `None` stays `None` — a cell
/// with no observed event for this column is a real "no signal" state (see
/// `model::WinnerMedsCell`'s doc), never coerced to a fabricated `0`.
fn round_ms(ms: Option<f64>) -> Option<i64> {
    ms.map(|v| v.round() as i64)
}

fn meds_cell(row: &WinnerMedsRow) -> WinnerMedsCell {
    WinnerMedsCell {
        winners: row.winners_n as u64,
        first_military_ms: round_ms(row.first_military_ms),
        loom_ms: round_ms(row.loom_ms),
        dba_ms: round_ms(row.dba_ms),
        wheelbarrow_ms: round_ms(row.wheelbarrow_ms),
        horse_collar_ms: round_ms(row.horse_collar_ms),
    }
}

/// The all-maps rollup grain (`winner_refs_meds.sql`'s `map_slug = 'all'` GROUPING SETS row) —
/// the analyzer's map-agnostic fallback when a player's own map has no `medsByMap` cell.
fn build_meds_by_bucket(rows: &[WinnerMedsRow]) -> BTreeMap<String, WinnerMedsCell> {
    rows.iter()
        .filter(|row| row.map_slug == "all")
        .map(|row| (row.elo_bucket.clone(), meds_cell(row)))
        .collect()
}

/// Every per-map grain (`map_slug != 'all'`) — the analyzer's preferred, map-specific cell.
fn build_meds_by_map(rows: &[WinnerMedsRow]) -> BTreeMap<String, WinnerMedsEloMap> {
    let mut by_map: BTreeMap<String, WinnerMedsEloMap> = BTreeMap::new();
    for row in rows.iter().filter(|row| row.map_slug != "all") {
        by_map
            .entry(row.map_slug.clone())
            .or_default()
            .insert(row.elo_bucket.clone(), meds_cell(row));
    }
    by_map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opening_row(civ: &str, bucket: &str, opening: &str, winners: i64, pct: f64) -> WinnerOpeningRow {
        WinnerOpeningRow {
            civ_slug: civ.to_string(),
            elo_bucket: bucket.to_string(),
            opening: opening.to_string(),
            winners_n: winners,
            share_pct: pct,
        }
    }

    fn ecotech_row(bucket: &str, winners: i64) -> WinnerEcotechRow {
        WinnerEcotechRow {
            elo_bucket: bucket.to_string(),
            winners_n: winners,
            wheelbarrow_pct: 28.4,
            loom_pct: 90.1,
            dba_pct: 88.8,
            horse_collar_pct: 64.2,
            gold_mining_pct: 39.7,
        }
    }

    fn meds_row(map: &str, bucket: &str, winners: i64) -> WinnerMedsRow {
        WinnerMedsRow {
            map_slug: map.to_string(),
            elo_bucket: bucket.to_string(),
            winners_n: winners,
            first_military_ms: Some(695_188.0),
            loom_ms: Some(392_164.4),
            dba_ms: Some(723_827.6),
            wheelbarrow_ms: None,
            horse_collar_ms: Some(960_814.0),
        }
    }

    #[test]
    fn nests_openings_by_civ_and_bucket_preserving_row_order_as_list_order() {
        let doc = build_winner_refs(
            &[
                opening_row("armenians", "1000-1199", "fast_castle", 569, 20.9),
                opening_row("armenians", "1000-1199", "man_at_arms", 569, 19.9),
            ],
            &[],
            &[],
        );
        let cell = &doc.openings["armenians"]["1000-1199"];
        assert_eq!(cell.winners, 569);
        assert_eq!(cell.list.len(), 2);
        assert_eq!(cell.list[0].opening, "fast_castle");
        assert_eq!(cell.list[0].pct, 20.9);
        assert_eq!(cell.list[1].opening, "man_at_arms");
    }

    #[test]
    fn keeps_distinct_civs_and_buckets_separate_in_openings() {
        let doc = build_winner_refs(
            &[
                opening_row("britons", "<1000", "archers", 200, 50.0),
                opening_row("britons", "2500+", "archers", 300, 66.7),
                opening_row("franks", "<1000", "scouts", 150, 40.0),
            ],
            &[],
            &[],
        );
        assert_eq!(doc.openings.len(), 2);
        assert_eq!(doc.openings["britons"].len(), 2);
        assert_eq!(doc.openings["franks"].len(), 1);
    }

    #[test]
    fn ecotech_is_keyed_by_bucket_alone_with_no_civ_or_map_dimension() {
        let doc = build_winner_refs(&[], &[ecotech_row("1000-1199", 545_283)], &[]);
        let cell = &doc.eco_tech_by_castle["1000-1199"];
        assert_eq!(cell.winners, 545_283);
        assert_eq!(cell.wheelbarrow, 28.4);
        assert_eq!(cell.loom, 90.1);
        assert_eq!(cell.dba, 88.8);
        assert_eq!(cell.horse_collar, 64.2);
        assert_eq!(cell.gold_mining, 39.7);
    }

    #[test]
    fn meds_all_maps_rollup_goes_to_meds_by_bucket_not_meds_by_map() {
        let doc = build_winner_refs(&[], &[], &[meds_row("all", "1000-1199", 23_582)]);
        assert!(doc.meds_by_bucket.contains_key("1000-1199"));
        assert!(doc.meds_by_map.is_empty());
        let cell = &doc.meds_by_bucket["1000-1199"];
        assert_eq!(cell.winners, 23_582);
        assert_eq!(cell.first_military_ms, Some(695_188));
    }

    #[test]
    fn meds_per_map_rows_go_to_meds_by_map_keyed_by_map_then_bucket() {
        let doc = build_winner_refs(&[], &[], &[meds_row("arabia", "1000-1199", 2_117)]);
        assert!(doc.meds_by_bucket.is_empty());
        let cell = &doc.meds_by_map["arabia"]["1000-1199"];
        assert_eq!(cell.winners, 2_117);
    }

    #[test]
    fn round_ms_rounds_to_nearest_millisecond_and_preserves_none() {
        assert_eq!(round_ms(Some(392_164.4)), Some(392_164));
        assert_eq!(round_ms(Some(723_827.6)), Some(723_828));
        assert_eq!(round_ms(None), None);
    }

    #[test]
    fn a_meds_cell_with_no_wheelbarrow_signal_serializes_as_a_real_none_not_a_fabricated_zero() {
        let doc = build_winner_refs(&[], &[], &[meds_row("all", "1000-1199", 23_582)]);
        assert_eq!(doc.meds_by_bucket["1000-1199"].wheelbarrow_ms, None);
    }

    #[test]
    fn each_parts_emptiness_is_independent_of_the_others() {
        // A thin `ecotech`/`meds` sample must never suppress an otherwise-full `openings` — see
        // the module doc's "three independently-scoped parts" note.
        let doc = build_winner_refs(&[opening_row("goths", "1400-1649", "trash", 400, 55.0)], &[], &[]);
        assert!(!doc.openings.is_empty());
        assert!(doc.eco_tech_by_castle.is_empty());
        assert!(doc.meds_by_bucket.is_empty());
        assert!(doc.meds_by_map.is_empty());
    }

    #[test]
    fn source_and_generated_are_always_populated_even_with_no_rows() {
        let doc = build_winner_refs(&[], &[], &[]);
        assert!(doc.source.contains("winner_refs"));
        assert_eq!(doc.generated.len(), 10, "YYYY-MM-DD");
        assert!(doc.openings.is_empty());
        assert!(doc.eco_tech_by_castle.is_empty());
        assert!(doc.meds_by_bucket.is_empty());
        assert!(doc.meds_by_map.is_empty());
    }
}
