//! Bridge: `aoe2rec::Savegame` -> normalized events + per-game meta + per-player ELO.
//! This is the only analyze module coupled to the parser; the pure logic lives elsewhere.

use std::collections::HashMap;

use aoe2rec::actions::ActionData;
use aoe2rec::{Operation, Savegame};

use crate::analyze::metrics::classify_cmd;
use crate::analyze::model::{Ev, EvKind, GameMeta, PlayerInfo};
use crate::postgame::{collect_leaderboard_elo, EloTable};

/// Build action body: `x:f32@[4..8]`, `y:f32@[8..12]`, `building_id:i32@[12..16]`.
/// Same `[12..16]` building-id offset as `extract::decode_build_id`; this superset also
/// returns the position. source: validated probe against real replays.
pub fn decode_build(d: &[u8]) -> Option<(i64, f32, f32)> {
    if d.len() < 16 {
        return None;
    }
    let x = f32::from_le_bytes(d[4..8].try_into().ok()?);
    let y = f32::from_le_bytes(d[8..12].try_into().ok()?);
    let id = i32::from_le_bytes(d[12..16].try_into().ok()?) as i64;
    Some((id, x, y))
}

pub struct Walked {
    pub meta: GameMeta,
    pub players: Vec<PlayerInfo>,
    pub evs: Vec<Ev>,
    pub elo: EloTable,
    /// one count per raw Operation::Action per player (BEFORE DeQueue amount-expansion) —
    /// the basis for eAPM. The rec logs no selects/camera, so this is already "effective".
    pub actions: HashMap<i32, u32>,
    /// the same count bucketed per minute (index = minute since game start) —
    /// feeds the APM-over-time chart.
    pub action_series: HashMap<i32, Vec<u32>>,
    /// per player (eco, mil) counts of UNAMBIGUOUSLY attributable commands
    /// (metrics::classify_cmd) — same per-command basis as `actions`.
    pub cmd_split: HashMap<i32, (u32, u32)>,
}

fn pid(ad: &ActionData) -> i32 {
    // Every ActionData variant carries the acting player as its first field. An
    // EXHAUSTIVE match (no wildcard) so a future variant fails to compile here
    // rather than being silently dropped onto phantom player 0 — the old 7-arm
    // match lost ~20% of commands, undercounting APM. (vendor/aoe2rec is a pinned
    // clone-on-build fork, so this list is stable; regenerate if the enum changes.)
    (match ad {
        ActionData::Interact { player_id, .. }
        | ActionData::Stop { player_id, .. }
        | ActionData::AiInteract { player_id, .. }
        | ActionData::Move { player_id, .. }
        | ActionData::Create { player_id, .. }
        | ActionData::AddAttribute { player_id, .. }
        | ActionData::GiveAttribute { player_id, .. }
        | ActionData::AiMove { player_id, .. }
        | ActionData::Resign { player_id, .. }
        | ActionData::Spec { player_id, .. }
        | ActionData::Waypoint { player_id, .. }
        | ActionData::Stance { player_id, .. }
        | ActionData::Guard { player_id, .. }
        | ActionData::Follow { player_id, .. }
        | ActionData::Patrol { player_id, .. }
        | ActionData::Formation { player_id, .. }
        | ActionData::Save { player_id, .. }
        | ActionData::AiWaypoint { player_id, .. }
        | ActionData::Chapter { player_id, .. }
        | ActionData::DeAttackMove { player_id, .. }
        | ActionData::DeUnknown35 { player_id, .. }
        | ActionData::DeUnknown37 { player_id, .. }
        | ActionData::Autoscout { player_id, .. }
        | ActionData::DeUnknown39 { player_id, .. }
        | ActionData::Transform { player_id, .. }
        | ActionData::SwitchAttack { player_id, .. }
        | ActionData::Unknown44 { player_id, .. }
        | ActionData::Unknown45 { player_id, .. }
        | ActionData::AiCommand { player_id, .. }
        | ActionData::AiQueue { player_id, .. }
        | ActionData::Research { player_id, .. }
        | ActionData::Build { player_id, .. }
        | ActionData::Game { player_id, .. }
        | ActionData::Unknown104 { player_id, .. }
        | ActionData::Wall { player_id, .. }
        | ActionData::Delete { player_id, .. }
        | ActionData::AttackGround { player_id, .. }
        | ActionData::Tribute { player_id, .. }
        | ActionData::DeUnknown109 { player_id, .. }
        | ActionData::Repair { player_id, .. }
        | ActionData::Release { player_id, .. }
        | ActionData::Multiqueue { player_id, .. }
        | ActionData::ToggleGate { player_id, .. }
        | ActionData::Flare { player_id, .. }
        | ActionData::Order { player_id, .. }
        | ActionData::Queue { player_id, .. }
        | ActionData::Gatherpoint { player_id, .. }
        | ActionData::Sell { player_id, .. }
        | ActionData::Buy { player_id, .. }
        | ActionData::DropRelic { player_id, .. }
        | ActionData::TownBell { player_id, .. }
        | ActionData::BackToWork { player_id, .. }
        | ActionData::DeQueue { player_id, .. }
        | ActionData::DeUnknown130 { player_id, .. }
        | ActionData::DeUnknown131 { player_id, .. }
        | ActionData::DeUnknown135 { player_id, .. }
        | ActionData::DeUnknown140 { player_id, .. }
        | ActionData::DeUnknown196 { player_id, .. }
        | ActionData::Achievements { player_id, .. } => *player_id,
    }) as i32
}

pub fn walk(game: &Savegame) -> Walked {
    let gs = &game.zheader.game_settings;
    // Team-aware winner: the winning team is the one with ZERO resignations (losers
    // resign; the winning side never does, so it survives even partial recordings).
    // Undecided (None) only when nobody resigned, teams can't be resolved, or it's
    // otherwise ambiguous (draw / cut recording) — where claiming a winner would lie.
    let resigned: std::collections::HashSet<i32> =
        game.get_resignations().iter().map(|&r| r as i32).collect();
    let mut team_resigns: std::collections::HashMap<u8, (u32, u32)> = Default::default(); // team -> (members, resigned)
    for p in gs.players.iter().filter(|p| p.civ_id != 0) {
        let e = team_resigns.entry(p.resolved_team_id).or_insert((0, 0));
        e.0 += 1;
        if resigned.contains(&p.player_number) {
            e.1 += 1;
        }
    }
    let zero_resign: Vec<u8> = team_resigns.iter().filter(|(_, (_, r))| *r == 0).map(|(t, _)| *t).collect();
    let winning_team =
        (!resigned.is_empty() && team_resigns.len() >= 2 && zero_resign.len() == 1).then(|| zero_resign[0]);
    let won = |team_id: u8| -> Option<bool> { winning_team.map(|w| team_id == w) };
    let players: Vec<PlayerInfo> = gs
        .players
        .iter()
        .filter(|p| p.civ_id != 0)
        .map(|p| PlayerInfo {
            player_number: p.player_number,
            profile_id: p.profile_id as i64,
            civ_id: p.civ_id,
            name: String::from(&p.name),
            color_id: p.color_id,
            team_id: p.resolved_team_id,
            won: won(p.resolved_team_id),
        })
        .collect();

    let mut evs = Vec::new();
    let mut elo = EloTable::default();
    let mut actions: HashMap<i32, u32> = HashMap::new();
    let mut action_series: HashMap<i32, Vec<u32>> = HashMap::new();
    let mut cmd_split: HashMap<i32, (u32, u32)> = HashMap::new();

    for op in &game.operations {
        match op {
            Operation::Action { action_data, world_time, .. } => {
                let p = pid(action_data);
                *actions.entry(p).or_insert(0) += 1; // one per raw command (eAPM basis)
                // same basis, bucketed per minute (APM-over-time chart)
                let min = (*world_time / 60_000) as usize;
                let series = action_series.entry(p).or_default();
                if series.len() <= min {
                    series.resize(min + 1, 0);
                }
                series[min] += 1;
                let kind = match action_data {
                    ActionData::DeQueue { unit_id, amount, .. } => {
                        // expand `amount` to repeated Train events for cumulative counting.
                        // cap the expansion: a corrupt replay could carry a huge `amount`.
                        let n = (*amount).clamp(1, 60);
                        for _ in 1..n {
                            evs.push(Ev { player: p, t_ms: *world_time, kind: EvKind::Train(*unit_id) });
                        }
                        EvKind::Train(*unit_id)
                    }
                    ActionData::Research { technology_type, .. } => EvKind::Research(*technology_type),
                    ActionData::Build { data, .. } => match decode_build(data) {
                        Some((id, x, y)) => EvKind::Build { id, x, y },
                        None => EvKind::Other,
                    },
                    ActionData::Move { x, y, .. } => EvKind::Move { x: *x, y: *y },
                    ActionData::Interact { x, y, .. } => EvKind::Interact { x: *x, y: *y },
                    // resource type/amount live in an undecoded blob — counts only (honest)
                    ActionData::Buy { .. } => EvKind::MarketBuy,
                    ActionData::Sell { .. } => EvKind::MarketSell,
                    _ => EvKind::Other,
                };
                if let Some(is_eco) = classify_cmd(&kind) {
                    let e = cmd_split.entry(p).or_insert((0, 0));
                    if is_eco { e.0 += 1 } else { e.1 += 1 }
                }
                evs.push(Ev { player: p, t_ms: *world_time, kind });
            }
            Operation::PostGame { blocks, .. } => {
                collect_leaderboard_elo(blocks, &mut elo);
            }
            _ => {}
        }
    }

    let meta = GameMeta {
        map_id: gs.resolved_map_id,
        duration_ms: game.get_duration(), // parser's canonical duration (world_time + Sync increments)
        rec_player: game.zheader.replay.rec_player as i32,
    };

    Walked { meta, players, evs, elo, actions, action_series, cmd_split }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_blob_decodes_x_y_id() {
        // x=69.0 @ [4..8], y=32.0 @ [8..12], id=70 @ [12..16]
        let mut d = vec![0u8; 16];
        d[4..8].copy_from_slice(&69.0f32.to_le_bytes());
        d[8..12].copy_from_slice(&32.0f32.to_le_bytes());
        d[12..16].copy_from_slice(&70i32.to_le_bytes());
        assert_eq!(decode_build(&d), Some((70, 69.0, 32.0)));
        assert_eq!(decode_build(&[0u8; 4]), None);
    }

    // the (ladder_id, player_number) -> elo mapping itself is exercised where
    // `EloTable`/`collect_leaderboard_elo` now live: crate::postgame::tests.

    /// Runs only on demand against a real save: TEST_REPLAY=/path cargo test ... -- --ignored
    #[test]
    #[ignore]
    fn walks_a_real_replay() {
        let path = std::env::var("TEST_REPLAY").expect("set TEST_REPLAY=/path/to.aoe2record");
        let game = Savegame::from_file(std::path::Path::new(&path)).unwrap();
        let w = walk(&game);
        assert!(!w.players.is_empty());
        assert!(w.meta.duration_ms > 0);
        eprintln!(
            "map_id={} players={} evs={}",
            w.meta.map_id,
            w.players.len(),
            w.evs.len()
        );
    }
}
