//! Assembles [`CivMetaDoc`] from the `pipeline/dbt` views' rows. Pure (no `Client`, no IO) —
//! `main.rs` fetches [`CivMetaInputs`]'s fields via `query.rs`, then hands them to [`build_doc`].
//!
//! Threshold constants below are named per the ONE legacy JS generator each mirrors (see each
//! doc comment) — the M5a brief's "read `refresh-civ-current.mjs`" plus the earlier
//! `aggregate-rich.mjs`/`aggregate-patches.mjs` it preserves fields from (see those scripts'
//! module docs for why civ-meta.json is actually built by several generators, not one).

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::Utc;

use crate::model::{
    AgeUp, Appearances, BucketStats, CivEntry, CivMetaDoc, LadderStats, Opening, PatchEntry,
};
use crate::query::{AgeUpRow, ByMapRow, ByPatchRow, CivMetaRow, OpeningRow, PatchAxisRow};
use crate::stats::{pct, tier_of, wilson, TIER_METHOD};

/// Minimum overall (civ, ladder) games before that ladder's `LadderStats` block is populated
/// (otherwise `null`) — `refresh-civ-current.mjs`'s `c.g < 500` civ-sample gate.
pub const MIN_LADDER_GAMES: i64 = 500;
/// Minimum games in a single elo bucket before it's included in `byElo` —
/// `refresh-civ-current.mjs`'s `MIN_ELO`.
pub const MIN_ELO_BUCKET_GAMES: i64 = 100;
/// Minimum games on a single map before it's included in `byMap` — `refresh-civ-current.mjs`'s
/// `MIN_MAP`.
pub const MIN_MAP_GAMES: i64 = 200;
/// Minimum TOTAL (all-civs, 1v1-ladder-only) matches on a build before it's kept on the `patches`
/// axis at all — `aggregate-patches.mjs`'s `MIN_PATCH_MATCHES`.
pub const MIN_PATCH_TOTAL_MATCHES: i64 = 3000;
/// Minimum per-civ games on an already-kept patch before it appears in that civ's `byPatch` —
/// `aggregate-patches.mjs`'s (patch-scoped) `MIN_CIV_GAMES`.
pub const MIN_CIV_PATCH_GAMES: i64 = 100;
/// Newest N builds kept on the `patches` axis — `aggregate-patches.mjs`'s `MAX_PATCHES`.
pub const MAX_PATCHES: usize = 16;

const LADDERS: [&str; 2] = ["1v1", "team"];

/// Every input [`build_doc`] needs, already fetched from the `pipeline/dbt` views — bundled so the
/// assembly step itself stays `Client`-free and unit-testable with hand-built fixture rows.
#[derive(Debug, Default)]
pub struct CivMetaInputs {
    pub civ_slugs: Vec<String>,
    pub overall_and_elo: Vec<CivMetaRow>,
    pub by_map: Vec<ByMapRow>,
    pub by_patch: Vec<ByPatchRow>,
    pub openings: Vec<OpeningRow>,
    pub ageup: Vec<AgeUpRow>,
    pub patch_axis: Vec<PatchAxisRow>,
    pub source_date: Option<String>,
}

type LadderKey = (String, String);

pub fn build_doc(inputs: CivMetaInputs) -> CivMetaDoc {
    let patches = build_patches(&inputs.patch_axis);
    let kept_patches: HashSet<&str> = patches.iter().map(|p| p.patch.as_str()).collect();
    let appearances = build_appearances(&inputs.overall_and_elo);

    let mut overall: HashMap<LadderKey, &CivMetaRow> = HashMap::new();
    let mut by_elo: HashMap<LadderKey, Vec<&CivMetaRow>> = HashMap::new();
    for row in &inputs.overall_and_elo {
        let key = (row.civ_slug.clone(), row.ladder.clone());
        if row.elo_bucket == "all" {
            overall.insert(key, row);
        } else {
            by_elo.entry(key).or_default().push(row);
        }
    }

    let mut by_map: HashMap<LadderKey, Vec<&ByMapRow>> = HashMap::new();
    for row in &inputs.by_map {
        by_map
            .entry((row.civ_slug.clone(), row.ladder.clone()))
            .or_default()
            .push(row);
    }

    let mut by_patch: HashMap<LadderKey, Vec<&ByPatchRow>> = HashMap::new();
    for row in &inputs.by_patch {
        by_patch
            .entry((row.civ_slug.clone(), row.ladder.clone()))
            .or_default()
            .push(row);
    }

    let mut openings: HashMap<LadderKey, Vec<&OpeningRow>> = HashMap::new();
    for row in &inputs.openings {
        openings
            .entry((row.civ_slug.clone(), row.ladder.clone()))
            .or_default()
            .push(row);
    }

    let mut ageup: HashMap<LadderKey, &AgeUpRow> = HashMap::new();
    for row in &inputs.ageup {
        ageup.insert((row.civ_slug.clone(), row.ladder.clone()), row);
    }

    let mut civs = BTreeMap::new();
    for slug in &inputs.civ_slugs {
        let mut entry = CivEntry {
            ladder_1v1: None,
            team: None,
        };
        for ladder in LADDERS {
            let key = (slug.clone(), ladder.to_string());
            let stats = overall.get(&key).copied().and_then(|row| {
                if row.games < MIN_LADDER_GAMES {
                    return None;
                }
                Some(build_ladder_stats(
                    row,
                    by_elo.get(&key).map(Vec::as_slice).unwrap_or_default(),
                    by_map.get(&key).map(Vec::as_slice).unwrap_or_default(),
                    by_patch.get(&key).map(Vec::as_slice).unwrap_or_default(),
                    &kept_patches,
                    openings.get(&key).map(Vec::as_slice).unwrap_or_default(),
                    ageup.get(&key).copied(),
                ))
            });
            if ladder == "1v1" {
                entry.ladder_1v1 = stats;
            } else {
                entry.team = stats;
            }
        }
        civs.insert(slug.clone(), entry);
    }

    CivMetaDoc {
        source: "PostgreSQL pipeline (matches.source='aoestats') via the dbt civ_meta model — \
                  M5a sample, see the task report for scope"
            .to_string(),
        ladders: LADDERS.iter().map(|s| s.to_string()).collect(),
        generated: Utc::now().format("%Y-%m-%d").to_string(),
        source_date: inputs.source_date.unwrap_or_else(|| "unknown".to_string()),
        appearances,
        patches,
        elo_buckets: pipeline_core::elo::ELO_BUCKETS
            .iter()
            .map(|s| s.to_string())
            .collect(),
        tier_method: TIER_METHOD.to_string(),
        civs,
        patch_source: "matches.build (aoestats-sourced rows only; M5a sample — the full \
                        official-build axis lands at M6)"
            .to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_ladder_stats(
    overall: &CivMetaRow,
    elo_rows: &[&CivMetaRow],
    map_rows: &[&ByMapRow],
    patch_rows: &[&ByPatchRow],
    kept_patches: &HashSet<&str>,
    opening_rows: &[&OpeningRow],
    ageup_row: Option<&AgeUpRow>,
) -> LadderStats {
    let (lo, hi) = wilson(overall.wins as f64, overall.games as f64);
    let play_rate = pct(overall.games as f64 / overall.ladder_total_games.max(1) as f64);

    let by_elo: BTreeMap<String, BucketStats> = elo_rows
        .iter()
        .filter(|r| r.games >= MIN_ELO_BUCKET_GAMES)
        .map(|r| {
            (
                r.elo_bucket.clone(),
                BucketStats {
                    games: r.games as u64,
                    win_rate: r.winrate,
                },
            )
        })
        .collect();

    let by_map: BTreeMap<String, BucketStats> = map_rows
        .iter()
        .filter(|r| r.games >= MIN_MAP_GAMES)
        .map(|r| {
            (
                r.map_slug.clone(),
                BucketStats {
                    games: r.games as u64,
                    win_rate: r.winrate,
                },
            )
        })
        .collect();

    let by_patch: BTreeMap<String, BucketStats> = patch_rows
        .iter()
        .filter(|r| r.games >= MIN_CIV_PATCH_GAMES && kept_patches.contains(r.build.as_str()))
        .map(|r| {
            (
                r.build.clone(),
                BucketStats {
                    games: r.games as u64,
                    win_rate: r.winrate,
                },
            )
        })
        .collect();

    let openings: Vec<Opening> = opening_rows
        .iter()
        .map(|r| Opening {
            opening: r.opening.clone(),
            share: pct(r.games as f64 / overall.games as f64),
        })
        .collect();

    // "All or nothing", matching the committed file's shape: `ageUp` is either a full 3-key
    // object or `null`, never partial — see `AgeUpRow`'s doc for why any of the three medians can
    // individually be absent even when the row itself exists.
    let age_up = ageup_row.and_then(build_age_up);

    LadderStats {
        games: overall.games as u64,
        win_rate: overall.winrate,
        ci95: [pct(lo), pct(hi)],
        play_rate,
        tier: Some(tier_of(overall.winrate).to_string()),
        by_patch,
        by_elo,
        by_map,
        openings,
        age_up,
    }
}

fn build_age_up(row: &AgeUpRow) -> Option<AgeUp> {
    Some(AgeUp {
        feudal: row.feudal_median?.round() as i64,
        castle: row.castle_median?.round() as i64,
        imperial: row.imperial_median?.round() as i64,
    })
}

fn build_appearances(rows: &[CivMetaRow]) -> Appearances {
    let mut totals: HashMap<&str, i64> = HashMap::new();
    for row in rows {
        totals
            .entry(row.ladder.as_str())
            .or_insert(row.ladder_total_games);
    }
    Appearances {
        ladder_1v1: *totals.get("1v1").unwrap_or(&0) as u64,
        team: *totals.get("team").unwrap_or(&0) as u64,
    }
}

fn build_patches(rows: &[PatchAxisRow]) -> Vec<PatchEntry> {
    let mut kept: Vec<&PatchAxisRow> = rows
        .iter()
        .filter(|r| r.matches >= MIN_PATCH_TOTAL_MATCHES)
        .collect();
    // Builds are monotonically increasing ids — sort newest-first, matching
    // `aggregate-patches.mjs`'s `Number(b[0]) - Number(a[0])`.
    kept.sort_by_key(|r| std::cmp::Reverse(r.build.parse::<i64>().unwrap_or(i64::MIN)));
    kept.truncate(MAX_PATCHES);
    kept.into_iter()
        .map(|r| PatchEntry {
            patch: r.build.clone(),
            label: format!("Update {}", r.build),
            matches: r.matches as u64,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        civ: &str,
        ladder: &str,
        elo_bucket: &str,
        games: i64,
        wins: i64,
        total: i64,
    ) -> CivMetaRow {
        CivMetaRow {
            civ_slug: civ.to_string(),
            ladder: ladder.to_string(),
            elo_bucket: elo_bucket.to_string(),
            games,
            wins,
            winrate: pct(wins as f64 / games as f64),
            ladder_total_games: total,
        }
    }

    #[test]
    fn civ_below_min_games_gets_null_ladder_entry() {
        let inputs = CivMetaInputs {
            civ_slugs: vec!["franks".to_string()],
            overall_and_elo: vec![row("franks", "1v1", "all", 10, 5, 10)],
            ..Default::default()
        };
        let doc = build_doc(inputs);
        assert!(doc.civs["franks"].ladder_1v1.is_none());
        assert!(doc.civs["franks"].team.is_none());
    }

    #[test]
    fn civ_above_min_games_gets_populated_ladder_entry() {
        let inputs = CivMetaInputs {
            civ_slugs: vec!["franks".to_string()],
            overall_and_elo: vec![
                row("franks", "1v1", "all", 600, 330, 1200),
                row("franks", "1v1", "<1000", 150, 80, 1200),
                row("franks", "1v1", "1000-1199", 50, 20, 1200), // below MIN_ELO_BUCKET_GAMES
            ],
            ..Default::default()
        };
        let doc = build_doc(inputs);
        let stats = doc.civs["franks"].ladder_1v1.as_ref().expect("populated");
        assert_eq!(stats.games, 600);
        assert_eq!(stats.by_elo.len(), 1, "only the >=100-game bucket survives");
        assert!(stats.by_elo.contains_key("<1000"));
        assert!(doc.civs["franks"].team.is_none());
    }

    #[test]
    fn patches_axis_drops_thin_builds_and_sorts_newest_first() {
        let rows = vec![
            PatchAxisRow {
                build: "100".to_string(),
                matches: MIN_PATCH_TOTAL_MATCHES - 1,
            },
            PatchAxisRow {
                build: "200".to_string(),
                matches: MIN_PATCH_TOTAL_MATCHES,
            },
            PatchAxisRow {
                build: "300".to_string(),
                matches: MIN_PATCH_TOTAL_MATCHES + 500,
            },
        ];
        let patches = build_patches(&rows);
        assert_eq!(
            patches.iter().map(|p| p.patch.as_str()).collect::<Vec<_>>(),
            vec!["300", "200"]
        );
        assert_eq!(patches[0].label, "Update 300");
    }

    #[test]
    fn civ_with_no_rows_at_all_still_gets_an_entry_with_null_ladders() {
        let inputs = CivMetaInputs {
            civ_slugs: vec!["muisca".to_string()],
            ..Default::default()
        };
        let doc = build_doc(inputs);
        let entry = &doc.civs["muisca"];
        assert!(entry.ladder_1v1.is_none());
        assert!(entry.team.is_none());
    }
}
