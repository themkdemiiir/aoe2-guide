//! Assembles [`CivCubeDoc`] + [`CivCubeDimsDoc`] from `pipeline/dbt`'s `civ_cube` view rows. Pure
//! (no `Client`, no IO) — same fetch/assembly split as `winner_comps.rs`/`eco_benchmark.rs`.
//!
//! FULL-CORPUS joint civ stats cube (civ x elo_bucket x map x build) so `/civs` can offer
//! COMBINABLE filters (e.g. "current patch + 1200 elo + Arabia") with no server: the page fetches
//! `civ-cube.json` once and filters it client-side. `civ-cube-dims.json` is the tiny
//! dropdown-only sibling, imported at build time so the filter UI's option lists exactly match the
//! cube without pulling in the (much larger) full file.
//!
//! **Corpus/scope deviations from the old `build-civ-cube.mjs`/`build-cube-history.sql` this
//! replaces (documented here and in `civ_cube.sql`'s own doc — deliberate, schema-driven changes,
//! not oversights):**
//! - **One unified corpus, no archive/crawl merge.** The old generator stitched an archive-side
//!   DuckDB CSV (thresholded `>= 20`) onto a live crawl loop (unthresholded). This Postgres schema
//!   already unifies both sources in the same `matches`/`match_players` tables, so `civ_cube.sql`
//!   is one `GROUP BY` with a single uniform `>= 20` floor — tighter than the old crawl side ever
//!   was, looser than nothing: any crawl-only cell that used to appear with fewer than 20 games no
//!   longer clears the bar. A real, intentional change, not a bug.
//! - **No `__unknown__` map sentinel.** `matches.map_id` is a `NOT NULL` foreign key here — a
//!   match whose source map slug doesn't resolve is excluded at INSERT time (`aoestats::db`), so
//!   there is no unresolved-map case left for a sentinel to catch. `maps`/`mapNames` in the
//!   committed file's OLD shape end with `"__unknown__"`/`"Unknown"`; this exporter's output
//!   simply never has that trailing entry.
//! - **No guide-civs filter.** The old generator restricted to the site's 53 guide-covered civs
//!   (`loadGuideCivs()`). This view covers the full ~59-civ game roster the `civs` dim tracks
//!   (minus the `civ_id = 0` "random" sentinel) — same posture `civ_meta.sql` already established
//!   for every other Rust-ported view in this pipeline.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::model::{CivCubeDimsDoc, CivCubeDoc, CivCubeMonth, CivCubeRowPacked, PatchEntry};
use crate::query::CivCubeRow;

const SOURCE: &str = "aoestats.io ranked archive + live replay crawl, unified in the PostgreSQL \
                       matches/match_players tables (ranked RM 1v1 only) — PostgreSQL pipeline via \
                       the dbt civ_cube view. Full civ x elo_bucket x map x build cube, >=20 games \
                       per cell; the patch/build axis is the same kept-builds set civ-meta.json's \
                       own patches list uses.";

/// `YYYY-MM-DD` — same convention as `winner_comps.rs`'s own `today()`.
fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

/// Title-cases a `snake_case`/`kebab-case` map slug into a display name — `"mega_random"` ->
/// `"Mega Random"`, `"black_forest"` -> `"Black Forest"`, `"arabia"` -> `"Arabia"`. Byte-for-byte
/// mirror of the old `build-civ-cube.mjs`'s `prettify`.
fn prettify(slug: &str) -> String {
    slug.split(['-', '_'])
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Builds both `civ-cube.json` and `civ-cube-dims.json` from `civ_cube` view rows plus the SAME
/// kept-patches list `civ-meta.json`'s own `patches` axis uses (`civ_meta::build_patches` over
/// `patch_axis` rows) — the single source of truth `civ_cube.sql`'s doc requires so the two
/// documents' patch axes can never independently drift apart.
///
/// `rows` outside the kept-patch axis are dropped entirely (mirrors the old generator's
/// `keepPatches.has(...)` guard, applied BEFORE aggregation there and here alike) — every
/// dimension (`civs`, `maps`, `months`) is then derived from what remains, so a civ/map that only
/// ever appears on a since-dropped patch simply has no entry, same "data-driven, not padded" shape
/// as the old JS output.
pub fn build_civ_cube(rows: &[CivCubeRow], patches: &[PatchEntry]) -> (CivCubeDoc, CivCubeDimsDoc) {
    let kept_patches: HashSet<&str> = patches.iter().map(|p| p.patch.as_str()).collect();
    let rows: Vec<&CivCubeRow> = rows
        .iter()
        .filter(|r| kept_patches.contains(r.build.as_str()))
        .collect();

    let civs: Vec<String> = rows
        .iter()
        .map(|r| r.civ_slug.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(str::to_string)
        .collect();

    // Fixed nine-bucket vocabulary, unconditional — matches civ-meta.json's own `eloBuckets`
    // (never filtered by what's actually present in a thin sample).
    let elos: Vec<String> = pipeline_core::elo::ELO_BUCKETS
        .iter()
        .map(|s| s.to_string())
        .collect();

    let maps: Vec<String> = {
        let mut totals: BTreeMap<&str, i64> = BTreeMap::new();
        for r in &rows {
            *totals.entry(r.map_slug.as_str()).or_insert(0) += r.games;
        }
        // Most-played first; ties broken alphabetically for determinism — the old JS's stable
        // `Array.sort` tie order depended on `Object.keys` insertion order, not a meaningful
        // invariant worth reproducing (see the module doc).
        let mut ordered: Vec<(&str, i64)> = totals.into_iter().collect();
        ordered.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        ordered.into_iter().map(|(slug, _)| slug.to_string()).collect()
    };
    let map_names: Vec<String> = maps.iter().map(|s| prettify(s)).collect();

    let present_builds: HashSet<&str> = rows.iter().map(|r| r.build.as_str()).collect();
    let months: Vec<CivCubeMonth> = patches
        .iter()
        .filter(|p| present_builds.contains(p.patch.as_str()))
        .map(|p| CivCubeMonth {
            patch: p.patch.clone(),
            label: p.label.clone(),
        })
        .collect();

    let civ_index: HashMap<&str, u32> = civs
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i as u32))
        .collect();
    let elo_index: HashMap<&str, u32> = elos
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i as u32))
        .collect();
    let map_index: HashMap<&str, u32> = maps
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i as u32))
        .collect();
    let month_index: HashMap<&str, u32> = months
        .iter()
        .enumerate()
        .map(|(i, m)| (m.patch.as_str(), i as u32))
        .collect();

    // Every lookup below is total by construction: `civs`/`maps`/`months` are each derived FROM
    // `rows` (or, for `elos`, a fixed superset of every possible `elo_bucket`), so every row's four
    // keys are guaranteed present in their respective index. `filter_map`/`?` rather than a direct
    // index keeps this defensive (no panic path) — the same Option-combinator style
    // `civ_meta.rs::build_doc` uses throughout, rather than asserting the invariant with
    // `.expect(...)`.
    let packed: Vec<CivCubeRowPacked> = rows
        .iter()
        .filter_map(|r| {
            Some((
                *civ_index.get(r.civ_slug.as_str())?,
                *elo_index.get(r.elo_bucket.as_str())?,
                *map_index.get(r.map_slug.as_str())?,
                *month_index.get(r.build.as_str())?,
                r.games as u64,
                r.wins as u64,
            ))
        })
        .collect();

    let generated = today();
    let doc = CivCubeDoc {
        generated: generated.clone(),
        source: SOURCE.to_string(),
        civs,
        elos: elos.clone(),
        maps: maps.clone(),
        map_names: map_names.clone(),
        months: months.clone(),
        rows: packed,
    };
    let dims = CivCubeDimsDoc {
        generated,
        elos,
        maps,
        map_names,
        months,
    };
    (doc, dims)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(civ: &str, bucket: &str, map: &str, build: &str, games: i64, wins: i64) -> CivCubeRow {
        CivCubeRow {
            civ_slug: civ.to_string(),
            elo_bucket: bucket.to_string(),
            map_slug: map.to_string(),
            build: build.to_string(),
            games,
            wins,
        }
    }

    fn patch(build: &str, label: &str) -> PatchEntry {
        PatchEntry {
            patch: build.to_string(),
            label: label.to_string(),
            matches: 5000,
        }
    }

    /// `CivCubeMonth` (like every other `model.rs` struct) deliberately has no `PartialEq` derive
    /// (see that file's uniform `#[derive(Debug, Clone, Serialize, Deserialize)]` convention) — so
    /// tests compare its `(patch, label)` pairs instead of the struct itself, same posture
    /// `civ_meta.rs`'s own `PatchEntry`-comparing test uses.
    fn month_pairs(months: &[CivCubeMonth]) -> Vec<(&str, &str)> {
        months
            .iter()
            .map(|m| (m.patch.as_str(), m.label.as_str()))
            .collect()
    }

    #[test]
    fn packs_a_row_using_each_dims_index() {
        let rows = vec![row("franks", "1000-1199", "arabia", "179158", 120, 70)];
        let patches = vec![patch("179158", "Update 179158")];
        let (doc, _dims) = build_civ_cube(&rows, &patches);

        assert_eq!(doc.civs, vec!["franks"]);
        assert_eq!(doc.maps, vec!["arabia"]);
        assert_eq!(doc.map_names, vec!["Arabia"]);
        assert_eq!(doc.months.len(), 1);
        assert_eq!(doc.months[0].patch, "179158");

        let civ_i = doc.civs.iter().position(|c| c == "franks").unwrap() as u32;
        let elo_i = doc.elos.iter().position(|e| e == "1000-1199").unwrap() as u32;
        let map_i = doc.maps.iter().position(|m| m == "arabia").unwrap() as u32;
        let month_i = doc.months.iter().position(|m| m.patch == "179158").unwrap() as u32;
        assert_eq!(doc.rows, vec![(civ_i, elo_i, map_i, month_i, 120, 70)]);
    }

    #[test]
    fn elos_axis_is_the_fixed_nine_bucket_list_regardless_of_data() {
        let rows = vec![row("franks", "1000-1199", "arabia", "179158", 50, 25)];
        let patches = vec![patch("179158", "Update 179158")];
        let (doc, dims) = build_civ_cube(&rows, &patches);
        assert_eq!(doc.elos.len(), 9);
        assert_eq!(dims.elos, doc.elos);
    }

    #[test]
    fn civs_dim_is_data_driven_sorted_and_not_every_known_civ() {
        let rows = vec![
            row("teutons", "1000-1199", "arabia", "179158", 40, 20),
            row("aztecs", "1000-1199", "arabia", "179158", 30, 15),
        ];
        let patches = vec![patch("179158", "Update 179158")];
        let (doc, _dims) = build_civ_cube(&rows, &patches);
        assert_eq!(doc.civs, vec!["aztecs", "teutons"], "sorted, only civs with data");
    }

    #[test]
    fn maps_sort_by_total_games_descending_ties_broken_alphabetically() {
        let rows = vec![
            row("franks", "1000-1199", "arena", "179158", 50, 25),
            row("franks", "1000-1199", "arabia", "179158", 100, 60),
            row("franks", "1000-1199", "nomad", "179158", 50, 20), // tie with arena
        ];
        let patches = vec![patch("179158", "Update 179158")];
        let (doc, _dims) = build_civ_cube(&rows, &patches);
        assert_eq!(doc.maps, vec!["arabia", "arena", "nomad"]);
    }

    #[test]
    fn months_axis_is_patches_filtered_to_builds_actually_present_in_rows() {
        let rows = vec![row("franks", "1000-1199", "arabia", "179158", 40, 20)];
        // Newest-first order, like `civ_meta::build_patches`'s own output — "200000" never
        // appears in `rows`, so it must be dropped from `months` even though it's a kept patch.
        let patches = vec![
            patch("200000", "Update 200000"),
            patch("179158", "Update 179158"),
        ];
        let (doc, dims) = build_civ_cube(&rows, &patches);
        assert_eq!(doc.months.len(), 1);
        assert_eq!(doc.months[0].patch, "179158");
        assert_eq!(month_pairs(&dims.months), month_pairs(&doc.months));
    }

    #[test]
    fn rows_outside_the_kept_patch_axis_are_dropped_entirely() {
        // "999999" is not in `patches` at all (e.g. a thin build that never cleared
        // civ-meta.json's own MIN_PATCH_TOTAL_MATCHES gate) — the whole cell must disappear, not
        // just its `months` entry, mirroring the old generator's `keepPatches.has(...)` guard.
        let rows = vec![
            row("franks", "1000-1199", "arabia", "179158", 40, 20),
            row("franks", "1000-1199", "arabia", "999999", 40, 20),
        ];
        let patches = vec![patch("179158", "Update 179158")];
        let (doc, _dims) = build_civ_cube(&rows, &patches);
        assert_eq!(doc.rows.len(), 1);
    }

    #[test]
    fn dims_doc_has_no_civs_or_rows_and_shares_everything_else_with_the_full_doc() {
        let rows = vec![row("franks", "1000-1199", "arabia", "179158", 40, 20)];
        let patches = vec![patch("179158", "Update 179158")];
        let (doc, dims) = build_civ_cube(&rows, &patches);
        assert_eq!(dims.generated, doc.generated);
        assert_eq!(dims.elos, doc.elos);
        assert_eq!(dims.maps, doc.maps);
        assert_eq!(dims.map_names, doc.map_names);
        assert_eq!(month_pairs(&dims.months), month_pairs(&doc.months));
    }

    #[test]
    fn prettify_handles_snake_case_and_bare_slugs() {
        assert_eq!(prettify("arabia"), "Arabia");
        assert_eq!(prettify("mega_random"), "Mega Random");
        assert_eq!(prettify("black_forest"), "Black Forest");
        assert_eq!(prettify("fortified_clearing"), "Fortified Clearing");
    }

    #[test]
    fn source_and_generated_are_always_populated_even_with_no_rows() {
        let (doc, dims) = build_civ_cube(&[], &[]);
        assert!(doc.source.contains("civ_cube"));
        assert_eq!(doc.generated.len(), 10, "YYYY-MM-DD");
        assert!(doc.civs.is_empty());
        assert!(doc.rows.is_empty());
        assert_eq!(doc.elos.len(), 9, "elos axis is unconditional");
        assert_eq!(dims.elos.len(), 9);
    }
}
