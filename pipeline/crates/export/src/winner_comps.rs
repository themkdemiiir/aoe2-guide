//! Assembles [`WinnerCompsDoc`] from `pipeline/dbt`'s `winner_comps` view rows. Pure (no `Client`,
//! no IO) — same fetch/assembly split as `eco_benchmark.rs`/`benchmark.rs`/`civ_meta.rs`.
//!
//! WINNERS-only unit-composition benchmark: for each `(civ, elo_bucket)` cell, the top 6 military
//! units (by producer share) among winners of ranked 1v1 RM matches — feeds `winner-comps.json`,
//! the analyzer's "winners with your civ at your elo typically produce ..." panel.
//!
//! **Corpus/scope deviation from the old `build-winner-comps.mjs`/`.sql` this replaces (documented
//! here and in `winner_comps.sql`'s own doc — a deliberate schema-driven change, not an
//! oversight):**
//! - **REPLAY-SOURCE ONLY.** The old DuckDB script read aoestats' `match_ages.parquet` per-age
//!   `units` JSON (the ~30M-match archive). This Postgres schema's `match_ages` carries no
//!   per-unit breakdown at all (only `villagers`/`military`/`n_buildings`/`n_research` summary
//!   counts) — per-unit data only exists in `match_player_units`, which
//!   `m20260706_000012_create_match_player_units.rs`'s own doc names as the intended "Phase E
//!   winner-comps exporter" source, and which is populated ONLY from parsed replays (the
//!   ~194k-and-growing replay corpus, not the full aoestats archive).
//! - **WHOLE-MATCH totals, not "through Castle Age."** The old generator summed only the
//!   dark+feudal+castle per-age windows. `match_player_units.trained` has no age dimension at all
//!   (see that migration's doc) — it is a whole-match total. A player's late-Imperial siege spam
//!   now counts toward `pct`/`med` exactly like an early Castle-Age skirmisher rush would have.
//! - **Unit-name granularity may differ.** `unit` is the `units` dimension's `name` (lower-cased
//!   in `winner_comps.sql`), sourced from aoe2techtree ids — it may enumerate upgrade-tier ids the
//!   old aoestats extract folded together under one label. Not reconciled here; see the task
//!   report for verification notes.

use std::collections::BTreeMap;

use crate::model::{WinnerCompUnit, WinnerCompsCell, WinnerCompsDoc, WinnerCompsEloMap};
use crate::query::WinnerCompsRow;

const SOURCE: &str = "aoestats.io + live replay crawl (REPLAY-SOURCED match_player_units only) — \
                       PostgreSQL pipeline via the dbt winner_comps view. Military units trained \
                       (whole-match totals, not age-scoped) by WINNERS of ranked 1v1 RM, per civ x \
                       elo bucket: pct = share of winners who produced the unit, med = median \
                       trained count among those producers. Thresholds: >=100 winners in the cell, \
                       unit produced by >=15% of them; top 6 units per cell by producer share.";

/// `YYYY-MM-DD` — same convention as `matchups.rs`'s own `today()`.
fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

/// Fold `winner_comps` rows into the nested `civs.<civSlug>.<eloBucket>` document. Rows arrive
/// already restricted to `unit_rank <= 6` and ordered by `(civ_slug, elo_bucket, unit_rank)` (see
/// `query::fetch_winner_comps`'s doc), so a straight per-row push reproduces the intended
/// producer-share-descending `units` list — no re-sort needed here. Every `(civ_slug, elo_bucket)`
/// grain is unique per unit in the view, so there is no last-writer-wins concern either.
pub fn build_winner_comps(rows: &[WinnerCompsRow]) -> WinnerCompsDoc {
    let mut civs: BTreeMap<String, WinnerCompsEloMap> = BTreeMap::new();
    for row in rows {
        let cell = civs
            .entry(row.civ_slug.clone())
            .or_default()
            .entry(row.elo_bucket.clone())
            .or_insert_with(|| WinnerCompsCell {
                winners: row.winners_n as u64,
                units: Vec::new(),
            });
        cell.units.push(WinnerCompUnit {
            unit: row.unit.clone(),
            pct: row.producer_pct,
            med: row.med_count,
        });
    }
    WinnerCompsDoc {
        source: SOURCE.to_string(),
        generated: today(),
        civs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        civ: &str,
        bucket: &str,
        unit: &str,
        winners: i64,
        producers: i64,
        pct: f64,
        med: f64,
    ) -> WinnerCompsRow {
        WinnerCompsRow {
            civ_slug: civ.to_string(),
            elo_bucket: bucket.to_string(),
            unit: unit.to_string(),
            winners_n: winners,
            producers,
            producer_pct: pct,
            med_count: med,
        }
    }

    #[test]
    fn nests_civ_bucket_and_preserves_row_order_as_the_units_list_order() {
        let doc = build_winner_comps(&[
            row("armenians", "1000-1199", "spearman", 476, 313, 65.8, 7.0),
            row("armenians", "1000-1199", "monk", 476, 244, 51.3, 2.0),
        ]);
        let cell = &doc.civs["armenians"]["1000-1199"];
        assert_eq!(cell.winners, 476);
        assert_eq!(cell.units.len(), 2);
        assert_eq!(cell.units[0].unit, "spearman");
        assert_eq!(cell.units[0].pct, 65.8);
        assert_eq!(cell.units[1].unit, "monk");
    }

    #[test]
    fn allows_a_half_integer_median() {
        // A true `percentile_cont` median over an even producer count is genuinely `x.5` — must
        // round-trip as a real, non-truncated f64, matching real cells in the committed file
        // (e.g. armenians/1400-1649/archer: med 22.5).
        let doc = build_winner_comps(&[row("bengalis", "2500+", "archer", 500, 270, 54.0, 23.5)]);
        assert_eq!(doc.civs["bengalis"]["2500+"].units[0].med, 23.5);
    }

    #[test]
    fn keeps_distinct_elo_buckets_of_the_same_civ_separate() {
        let doc = build_winner_comps(&[
            row("britons", "<1000", "archer", 200, 100, 50.0, 10.0),
            row("britons", "2500+", "archer", 300, 200, 66.7, 15.0),
        ]);
        assert_eq!(doc.civs["britons"].len(), 2);
        assert!(doc.civs["britons"].contains_key("<1000"));
        assert!(doc.civs["britons"].contains_key("2500+"));
    }

    #[test]
    fn keeps_distinct_civs_separate_at_the_same_bucket() {
        let doc = build_winner_comps(&[
            row("franks", "1800-1999", "knight", 400, 300, 75.0, 10.0),
            row("teutons", "1800-1999", "knight", 350, 200, 57.1, 8.0),
        ]);
        assert_eq!(doc.civs.len(), 2);
        assert_eq!(doc.civs["franks"]["1800-1999"].units[0].unit, "knight");
        assert_eq!(doc.civs["teutons"]["1800-1999"].units[0].unit, "knight");
    }

    #[test]
    fn source_and_generated_are_always_populated() {
        let doc = build_winner_comps(&[]);
        assert!(doc.source.contains("winner_comps"));
        assert_eq!(doc.generated.len(), 10, "YYYY-MM-DD");
        assert!(doc.civs.is_empty());
    }
}
