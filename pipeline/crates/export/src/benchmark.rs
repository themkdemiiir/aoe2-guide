//! Assembles [`BenchmarkDoc`] from `pipeline/dbt`'s `benchmark_ageup`/`benchmark_vils` views'
//! rows. Pure (no `Client`, no IO) — same fetch/assembly split as `civ_meta.rs`/`matchups.rs`.
//!
//! The min-sample threshold (old `build-benchmark*.sql`'s `HAVING count(*) >= 50`) lives IN the
//! dbt models, not here — see `benchmark_ageup.sql`'s doc for why this one pair of models breaks
//! from `civ_meta`'s/`matchups`'s "thresholds only in Rust" convention.
//!
//! **Documented M5b sample scope:** `benchmark_ageup` clears its threshold for 22 grains today
//! (mostly per-civ rollups, one per-map cell) — real, useful output. `benchmark_vils` clears it
//! for ZERO grains: WINNERS who reached Castle with both early-age windows recorded number 1056
//! players total, spread thin enough across (civ, map, elo_bucket, mode) that no single grain
//! reaches 50. Every `vils_castle` field is therefore `None` in this M5b run — real SQL, real
//! threshold, not enough volume yet (the exact same class of caveat `matchups.rs`'s doc accepts
//! for `civ-matchups.json`). `vils_castle` renders as JSON `null` (never omitted) on every cell,
//! rather than the old `build-benchmark-vils.mjs`'s plain key omission — see `model.rs`'s
//! `BenchmarkCell` doc for why that deviation is harmless to shape parity.

use std::collections::BTreeMap;

use crate::model::{BenchmarkCell, BenchmarkDoc, BenchmarkModeMap};
use crate::query::{BenchmarkAgeupRow, BenchmarkVilsRow};

const SOURCE: &str = "PostgreSQL pipeline (matches.source='aoestats') via the dbt benchmark_ageup/\
                       benchmark_vils models — M5b sample, see the task report for scope. \
                       feudal_s/castle_s/imperial_s = median age-up completion seconds by \
                       civ x map x elo_bucket x mode, with 'all'-keyed rollups; vils_castle = \
                       median villagers trained through Castle Age among slice WINNERS.";

/// Rounds to 1 decimal place, matching the old `build-benchmark*.sql`'s own `round(median(...), 1)`
/// display precision (the underlying `f64` from `percentile_cont` carries full float precision —
/// see `benchmark_ageup.sql`'s doc for why no cast/round happens in SQL itself).
fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

type CivMap = BTreeMap<String, BTreeMap<String, BTreeMap<String, BenchmarkModeMap>>>;

fn cell_mut<'a>(
    civs: &'a mut CivMap,
    civ: &str,
    map: &str,
    bucket: &str,
    mode: &str,
) -> &'a mut BenchmarkCell {
    civs.entry(civ.to_string())
        .or_default()
        .entry(map.to_string())
        .or_default()
        .entry(bucket.to_string())
        .or_default()
        .entry(mode.to_string())
        .or_insert(BenchmarkCell {
            feudal_s: None,
            castle_s: None,
            imperial_s: None,
            vils_castle: None,
        })
}

/// Builds `benchmark.json` by first laying down every `benchmark_ageup` grain (the richer,
/// always-present source — 13.9M-match-scale in the old DuckDB source, `match_players` here), then
/// overlaying `benchmark_vils`'s `vils_castle` onto whichever of those SAME grains it also covers
/// (never inventing a cell `benchmark_ageup` didn't already produce — matches
/// `build-benchmark-vils.mjs`'s own "merge into existing cells" contract, see that script's doc).
pub fn build_benchmark(ageup: &[BenchmarkAgeupRow], vils: &[BenchmarkVilsRow]) -> BenchmarkDoc {
    let mut civs: CivMap = BTreeMap::new();

    for row in ageup {
        let cell = cell_mut(
            &mut civs,
            &row.civ_slug,
            &row.map_slug,
            &row.elo_bucket,
            &row.mode,
        );
        cell.feudal_s = Some(round1(row.feudal_median));
        cell.castle_s = Some(round1(row.castle_median));
        cell.imperial_s = Some(round1(row.imperial_median));
    }

    let mut merged = 0usize;
    let mut skipped_missing_cell = 0usize;
    for row in vils {
        let civ_entry = civs.get_mut(&row.civ_slug);
        let cell = civ_entry
            .and_then(|maps| maps.get_mut(&row.map_slug))
            .and_then(|buckets| buckets.get_mut(&row.elo_bucket))
            .and_then(|modes| modes.get_mut(&row.mode));
        match cell {
            Some(cell) => {
                cell.vils_castle = Some(round1(row.vils_median));
                merged += 1;
            }
            None => skipped_missing_cell += 1,
        }
    }
    tracing::debug!(
        merged,
        skipped_missing_cell,
        "benchmark_vils overlay merged"
    );

    BenchmarkDoc {
        source: SOURCE.to_string(),
        civs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ageup_row(civ: &str, map: &str, bucket: &str, mode: &str) -> BenchmarkAgeupRow {
        BenchmarkAgeupRow {
            civ_slug: civ.to_string(),
            map_slug: map.to_string(),
            elo_bucket: bucket.to_string(),
            mode: mode.to_string(),
            feudal_median: 600.0,
            castle_median: 1300.0,
            imperial_median: 2300.0,
        }
    }

    #[test]
    fn ageup_only_leaves_vils_castle_null() {
        let doc = build_benchmark(&[ageup_row("franks", "arabia", "all", "1v1")], &[]);
        let cell = &doc.civs["franks"]["arabia"]["all"]["1v1"];
        assert_eq!(cell.feudal_s, Some(600.0));
        assert!(cell.vils_castle.is_none());
    }

    #[test]
    fn vils_overlays_onto_a_matching_ageup_cell() {
        let ageup = vec![ageup_row("franks", "arabia", "all", "1v1")];
        let vils = vec![BenchmarkVilsRow {
            civ_slug: "franks".to_string(),
            map_slug: "arabia".to_string(),
            elo_bucket: "all".to_string(),
            mode: "1v1".to_string(),
            vils_median: 41.0,
        }];
        let doc = build_benchmark(&ageup, &vils);
        assert_eq!(
            doc.civs["franks"]["arabia"]["all"]["1v1"].vils_castle,
            Some(41.0)
        );
    }

    #[test]
    fn vils_for_a_grain_ageup_never_produced_is_dropped_not_invented() {
        let vils = vec![BenchmarkVilsRow {
            civ_slug: "franks".to_string(),
            map_slug: "arabia".to_string(),
            elo_bucket: "all".to_string(),
            mode: "1v1".to_string(),
            vils_median: 41.0,
        }];
        let doc = build_benchmark(&[], &vils);
        assert!(
            doc.civs.is_empty(),
            "no ageup cell exists to attach vils_castle to"
        );
    }

    #[test]
    fn round1_matches_expected_precision() {
        assert_eq!(round1(831.549), 831.5);
        assert_eq!(round1(552.0), 552.0);
    }
}
