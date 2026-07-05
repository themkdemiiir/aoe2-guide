//! Replay -> per-player macro-coaching report (the `analyze` subcommand).
//!
//! Pipeline: parse (aoe2rec) -> walk (events+meta+elo) -> metrics -> position ->
//! benchmark compare -> report. Pure metric logic (`metrics`) is isolated from the
//! parser bridge (`walk`) so it unit-tests with hand-built event vectors.

pub mod model;
pub mod maps;
pub mod metrics;
pub mod walk;
pub mod data;
pub mod position;
pub mod float;
pub mod compare;

pub use model::{Report, YouSel};

use std::collections::HashMap;

use aoe2rec::Savegame;

/// Pure analysis: parsed replay in -> Report out. No file/network IO (committed
/// data/* are include_str!-baked). This is the future WASM boundary. Errs ONLY
/// when an explicit you-selector matches no player (no-fallback rule).
pub fn analyze(game: &Savegame, you: &YouSel) -> anyhow::Result<Report> {
    let w = walk::walk(game);
    let map_table = maps::load();
    let (map_name, family) = map_table.lookup(w.meta.map_id);
    let map_name = map_name.to_string();
    let map_slug = maps::slug(&map_name);

    let roles = position::assign_roles(&w.players, family);
    let bases: HashMap<i32, (f32, f32)> = w.players.iter()
        .filter_map(|p| metrics::base_centroid(&w.evs, p.player_number, 360_000)
            .map(|b| (p.player_number, b)))
        .collect();
    let coords = position::coord_metrics(&w.evs, &w.players, &bases);

    let costs = data::load_costs();
    let bench = data::load_benchmark();
    let civs = data::load_civs();

    let mut players = compare::build_metrics(&w, &costs, &civs, &roles, &coords, family);
    let mode = if compare::is_team_game(&players) { "team" } else { "1v1" };
    let findings = compare::findings(&players, &bench, &civs, family, &map_slug, mode);
    compare::attach_references(&mut players, &bench, &civs, &map_slug, mode);
    let you = resolve_you(you, &w.players, w.meta.rec_player)?;

    Ok(Report {
        schema_version: model::SCHEMA_VERSION,
        meta: model::ReportMeta {
            map_id: w.meta.map_id, map_name, family, mode: mode.to_string(),
            duration_ms: w.meta.duration_ms, you,
        },
        players,
        findings,
        caveats: vec![float::CAVEAT.to_string(), model::MACRO_CAVEAT.to_string()],
    })
}

/// "you" = Auto (the recorder) or an explicit name/profile selector. An explicit
/// selector that matches no player is an ERROR (no-fallback rule) — silently
/// coaching the wrong player would be worse than failing.
fn resolve_you(sel: &YouSel, players: &[model::PlayerInfo], rec: i32) -> anyhow::Result<i32> {
    match sel {
        YouSel::Name(n) => players.iter()
            .find(|p| p.name.eq_ignore_ascii_case(n)).map(|p| p.player_number)
            .ok_or_else(|| anyhow::anyhow!(
                "--you '{n}' matches no player (players: {})",
                players.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "))),
        YouSel::ProfileId(id) => players.iter()
            .find(|p| p.profile_id == *id).map(|p| p.player_number)
            .ok_or_else(|| anyhow::anyhow!("profile {id} is not a player in this replay")),
        YouSel::Auto => Ok(rec),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::model::{PlayerInfo, YouSel};

    fn p(pn: i32, profile_id: i64, name: &str) -> PlayerInfo {
        PlayerInfo { player_number: pn, profile_id, civ_id: 2, name: name.into(),
                     color_id: pn, team_id: 2, won: None }
    }

    #[test]
    fn resolve_you_by_profile_name_auto_and_loud_error() {
        let players = vec![p(1, 111, "Alice"), p(2, 222, "Bob")];
        assert_eq!(resolve_you(&YouSel::ProfileId(222), &players, 1).unwrap(), 2);
        assert_eq!(resolve_you(&YouSel::Name("bob".into()), &players, 1).unwrap(), 2); // case-insensitive
        assert_eq!(resolve_you(&YouSel::Auto, &players, 1).unwrap(), 1);
        // no-fallback rule: an unmatched explicit selector is an ERROR, never the recorder
        assert!(resolve_you(&YouSel::ProfileId(999), &players, 1).is_err());
        assert!(resolve_you(&YouSel::Name("nobody".into()), &players, 1).is_err());
    }
}
