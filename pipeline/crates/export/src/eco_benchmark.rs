//! Assembles [`EcoBenchmarkDoc`] from `pipeline/dbt`'s `benchmark_ecotech` view rows. Pure (no
//! `Client`, no IO) — same fetch/assembly split as `benchmark.rs`/`civ_meta.rs`/`matchups.rs`.
//!
//! WINNER-focused (the view filters `won = true`): each cell is the p25/p50/p75 of a watched eco
//! upgrade's first-research CLICK time among the players who WON, by `tech_id x elo_bucket x mode`,
//! with `"all"`-keyed elo/mode rollups. The WASM analyzer overlays this onto a player's own
//! eco-upgrade timings to show the p25–p75 winner band at their elo.
//!
//! Rows arrive in milliseconds; stored as SECONDS rounded to 1 decimal, matching `benchmark.json`'s
//! `*_s` convention (see `benchmark.rs`'s `round1`). Unlike `build_benchmark`'s two-view overlay,
//! this is a single view -> a single nested map, so the assembly is a straight insert.

use std::collections::BTreeMap;

use crate::model::{EcoBand, EcoBenchmarkDoc, EcoModeMap};
use crate::query::BenchmarkEcoRow;

const SOURCE: &str = "aoestats.io ranked archive + live replay crawl — PostgreSQL pipeline via the \
                       dbt benchmark_ecotech view. p25_s/p50_s/p75_s = 25th/50th/75th percentile of \
                       each watched eco upgrade's first-research CLICK time (seconds) among WINNERS, \
                       by tech_id x map_slug x elo_bucket x mode, with 'all'-keyed map/elo rollups.";

/// Rounds to 1 decimal place — same display precision as `benchmark.rs`'s `round1`.
fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Fold `benchmark_ecotech` rows into the nested `techs.<techId>.<mapSlug>.<eloBucket>.<mode>`
/// document. Every `(tech_id, map_slug, elo_bucket, mode)` grain is unique in the view (GROUPING
/// SETS never emit a duplicate grain), so a plain `insert` is correct — no last-writer-wins.
pub fn build_eco_benchmark(rows: &[BenchmarkEcoRow]) -> EcoBenchmarkDoc {
    let mut techs: BTreeMap<String, BTreeMap<String, BTreeMap<String, EcoModeMap>>> =
        BTreeMap::new();
    for row in rows {
        techs
            .entry(row.tech_id.to_string())
            .or_default()
            .entry(row.map_slug.clone())
            .or_default()
            .entry(row.elo_bucket.clone())
            .or_default()
            .insert(
                row.mode.clone(),
                EcoBand {
                    p25_s: round1(row.p25_ms / 1000.0),
                    p50_s: round1(row.p50_ms / 1000.0),
                    p75_s: round1(row.p75_ms / 1000.0),
                    n: row.n as u64,
                },
            );
    }
    EcoBenchmarkDoc { source: SOURCE.to_string(), techs }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(tech_id: i32, map: &str, bucket: &str, mode: &str, p50_ms: f64) -> BenchmarkEcoRow {
        BenchmarkEcoRow {
            tech_id,
            map_slug: map.to_string(),
            elo_bucket: bucket.to_string(),
            mode: mode.to_string(),
            p25_ms: p50_ms - 30_000.0,
            p50_ms,
            p75_ms: p50_ms + 60_000.0,
            n: 1234,
        }
    }

    #[test]
    fn nests_tech_map_bucket_mode_in_seconds() {
        let doc = build_eco_benchmark(&[row(22, "arabia", "1000-1199", "1v1", 408_000.0)]);
        let cell = &doc.techs["22"]["arabia"]["1000-1199"]["1v1"];
        assert_eq!(cell.p50_s, 408.0);
        assert_eq!(cell.p25_s, 378.0);
        assert_eq!(cell.p75_s, 468.0);
        assert_eq!(cell.n, 1234);
    }

    #[test]
    fn keeps_map_and_elo_rollup_grains_as_all_keys() {
        let doc = build_eco_benchmark(&[
            row(213, "arena", "all", "1v1", 900_000.0), // same-map all-elo rollup
            row(213, "all", "1200-1399", "1v1", 1_300_000.0), // all-map same-elo rollup
            row(213, "all", "all", "1v1", 1_290_000.0), // all-map all-elo rollup
        ]);
        assert!(doc.techs["213"]["arena"].contains_key("all")); // map-specific elo rollup
        assert!(doc.techs["213"]["all"].contains_key("1200-1399")); // all-map elo cell
        assert!(doc.techs["213"]["all"].contains_key("all")); // all-map all-elo cell
    }

    #[test]
    fn round1_matches_benchmark_precision() {
        // 831549 ms -> 831.549 s -> 831.5
        assert_eq!(round1(831_549.0 / 1000.0), 831.5);
    }
}
