//! Assembles the four `civ-matchups*.json` docs from `pipeline/dbt`'s `matchups_*` views' rows.
//! Pure (no `Client`, no IO) — mirrors `civ_meta.rs`'s split between fetch (`query.rs`) and
//! assembly (this module).
//!
//! Threshold constants are named per `refresh-matchups-current.mjs`'s own JS constant (the single
//! old generator that produced all four of these files in one pass — see that script's module
//! doc), same "replicate, don't reinvent" posture as `civ_meta.rs`.
//!
//! Sourced from the full aoestats.io ranked archive (`matches.source='aoestats'`, all-time,
//! 2022-08 onward) — not a dev sample. A directed civ pair only appears in `civ-matchups.json`/
//! `civ-matchups-by-map.json` once it clears `MIN_GAMES`/`MIN_MAP_GAMES`; `civ-matchups-by-elo.json`
//! clears the lower `MIN_BUCKET_GAMES`/`MIN_ALL_GAMES`; `civ-matchups-team.json` is built from team
//! matches only. Real SQL, real thresholds — a thin cell is omitted rather than padded, per
//! `civ_meta.rs`'s own "populated_1v1"-style caveat.

use std::collections::BTreeMap;

use pipeline_core::elo::ELO_BUCKETS;

use crate::model::{
    CivMatchupsByEloDoc, CivMatchupsByMapDoc, CivMatchupsDoc, EloBucketMap, EloMinGames,
    MatchupOpponent,
};
use crate::query::{MatchupByEloRow, MatchupByMapRow, MatchupRow};

/// `refresh-matchups-current.mjs`'s `MIN` — the 1v1-overall per-pair games gate.
pub const MIN_GAMES: i64 = 50;
/// `refresh-matchups-current.mjs`'s `MIN_MAP`.
pub const MIN_MAP_GAMES: i64 = 50;
/// `refresh-matchups-current.mjs`'s `MIN_BUCKET` (a single per-bucket cell).
pub const MIN_BUCKET_GAMES: i64 = 30;
/// `refresh-matchups-current.mjs`'s `MIN_ALL` (the elo-agnostic `"all"` rollup cell within
/// `civ-matchups-by-elo.json` — numerically identical to `MIN_BUCKET_GAMES` today, but named
/// separately because it gates a logically distinct thing, matching the JS source's own two
/// separate constants).
pub const MIN_ALL_GAMES: i64 = 30;
/// `refresh-matchups-current.mjs`'s `MIN_TEAM`.
pub const MIN_TEAM_GAMES: i64 = 100;

const SOURCE: &str = "aoestats.io ranked archive (all-time, 2022-08 onward) — PostgreSQL \
                       pipeline via the dbt matchups_* models";

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

/// Groups `rows` by `civ_slug` into a `Vec<MatchupOpponent>` sorted by win rate descending (ties
/// broken by opponent slug for determinism) — `refresh-matchups-current.mjs`'s own `civs[c].sort((a,
/// b) => b.winRate - a.winRate)`. `min_games` is the per-pair inclusion gate.
fn group_opponents(rows: &[MatchupRow], min_games: i64) -> BTreeMap<String, Vec<MatchupOpponent>> {
    let mut civs: BTreeMap<String, Vec<MatchupOpponent>> = BTreeMap::new();
    for row in rows {
        if row.games < min_games {
            continue;
        }
        civs.entry(row.civ_slug.clone())
            .or_default()
            .push(MatchupOpponent {
                opp: row.opp_slug.clone(),
                games: row.games as u64,
                win_rate: row.winrate,
            });
    }
    for opponents in civs.values_mut() {
        opponents.sort_by(|a, b| {
            b.win_rate
                .partial_cmp(&a.win_rate)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.opp.cmp(&b.opp))
        });
    }
    civs
}

/// Builds `civ-matchups.json` from `matchups_1v1` rows.
pub fn build_civ_matchups(rows: &[MatchupRow]) -> CivMatchupsDoc {
    CivMatchupsDoc {
        source: SOURCE.to_string(),
        generated: today(),
        ladder: "1v1".to_string(),
        min_games: MIN_GAMES as u64,
        note: "winRate = how often <civ> beats <opp> in 1v1 RM. Mirrors excluded.".to_string(),
        civs: group_opponents(rows, MIN_GAMES),
    }
}

/// Builds `civ-matchups-team.json` from `matchups_team` rows — same shape as
/// `civ-matchups.json`, different ladder/threshold/content.
pub fn build_civ_matchups_team(rows: &[MatchupRow]) -> CivMatchupsDoc {
    CivMatchupsDoc {
        source: SOURCE.to_string(),
        generated: today(),
        ladder: "team".to_string(),
        min_games: MIN_TEAM_GAMES as u64,
        note: "winRate = how often <civ>'s team wins when <opp> is on the enemy team (team RM; \
               confounded by the 3 other civs per side)."
            .to_string(),
        civs: group_opponents(rows, MIN_TEAM_GAMES),
    }
}

/// Builds `civ-matchups-by-map.json` from `matchups_1v1_by_map` rows.
pub fn build_civ_matchups_by_map(rows: &[MatchupByMapRow]) -> CivMatchupsByMapDoc {
    let mut civs: BTreeMap<String, BTreeMap<String, Vec<MatchupOpponent>>> = BTreeMap::new();
    for row in rows {
        if row.games < MIN_MAP_GAMES {
            continue;
        }
        civs.entry(row.civ_slug.clone())
            .or_default()
            .entry(row.map_slug.clone())
            .or_default()
            .push(MatchupOpponent {
                opp: row.opp_slug.clone(),
                games: row.games as u64,
                win_rate: row.winrate,
            });
    }
    for maps in civs.values_mut() {
        for opponents in maps.values_mut() {
            opponents.sort_by(|a, b| {
                b.win_rate
                    .partial_cmp(&a.win_rate)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.opp.cmp(&b.opp))
            });
        }
    }
    CivMatchupsByMapDoc {
        source: SOURCE.to_string(),
        generated: today(),
        ladder: "1v1".to_string(),
        min_games: MIN_MAP_GAMES as u64,
        note: "winRate = how often <civ> beats <opp> on <map> in 1v1 RM. Mirrors excluded."
            .to_string(),
        civs,
    }
}

/// Builds `civ-matchups-by-elo.json` from `matchups_1v1_by_elo` rows (`elo_bucket = "all"`
/// included as just another bucket key — see that view's doc).
pub fn build_civ_matchups_by_elo(rows: &[MatchupByEloRow]) -> CivMatchupsByEloDoc {
    let mut civs: BTreeMap<String, BTreeMap<String, EloBucketMap>> = BTreeMap::new();
    for row in rows {
        let min = if row.elo_bucket == "all" {
            MIN_ALL_GAMES
        } else {
            MIN_BUCKET_GAMES
        };
        if row.games < min {
            continue;
        }
        civs.entry(row.civ_slug.clone())
            .or_default()
            .entry(row.opp_slug.clone())
            .or_default()
            .insert(row.elo_bucket.clone(), (row.winrate, row.games as u64));
    }
    let mut elo_buckets = vec!["all".to_string()];
    elo_buckets.extend(ELO_BUCKETS.iter().map(|s| s.to_string()));
    CivMatchupsByEloDoc {
        source: SOURCE.to_string(),
        generated: today(),
        ladder: "1v1".to_string(),
        elo_buckets,
        min_games: EloMinGames {
            bucket: MIN_BUCKET_GAMES as u64,
            all: MIN_ALL_GAMES as u64,
        },
        note: "[winRate, games] of <civ> vs <opp> per elo bucket. Mirrors excluded.".to_string(),
        civs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(civ: &str, opp: &str, games: i64, winrate: f64) -> MatchupRow {
        MatchupRow {
            civ_slug: civ.to_string(),
            opp_slug: opp.to_string(),
            games,
            winrate,
        }
    }

    #[test]
    fn pairs_below_min_games_are_dropped() {
        let rows = vec![row("franks", "britons", MIN_GAMES - 1, 60.0)];
        let doc = build_civ_matchups(&rows);
        assert!(doc.civs.is_empty());
    }

    #[test]
    fn opponents_sort_by_win_rate_descending() {
        let rows = vec![
            row("franks", "britons", MIN_GAMES, 40.0),
            row("franks", "teutons", MIN_GAMES, 60.0),
        ];
        let doc = build_civ_matchups(&rows);
        let opponents = &doc.civs["franks"];
        assert_eq!(opponents[0].opp, "teutons");
        assert_eq!(opponents[1].opp, "britons");
    }

    #[test]
    fn by_elo_all_bucket_and_real_bucket_use_different_thresholds() {
        let rows = vec![
            MatchupByEloRow {
                civ_slug: "franks".to_string(),
                opp_slug: "britons".to_string(),
                elo_bucket: "all".to_string(),
                games: MIN_ALL_GAMES,
                winrate: 55.0,
            },
            MatchupByEloRow {
                civ_slug: "franks".to_string(),
                opp_slug: "britons".to_string(),
                elo_bucket: "<1000".to_string(),
                games: MIN_BUCKET_GAMES - 1,
                winrate: 50.0,
            },
        ];
        let doc = build_civ_matchups_by_elo(&rows);
        let buckets = &doc.civs["franks"]["britons"];
        assert_eq!(buckets.len(), 1, "the thin real bucket must be dropped");
        assert!(buckets.contains_key("all"));
        assert_eq!(buckets["all"], (55.0, MIN_ALL_GAMES as u64));
    }

    #[test]
    fn elo_buckets_axis_starts_with_all() {
        let doc = build_civ_matchups_by_elo(&[]);
        assert_eq!(doc.elo_buckets[0], "all");
        assert_eq!(doc.elo_buckets.len(), 1 + ELO_BUCKETS.len());
    }
}
