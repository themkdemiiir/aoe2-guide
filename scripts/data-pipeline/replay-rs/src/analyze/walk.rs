//! Bridge: `aoe2rec::Savegame` -> normalized events + per-game meta + per-player ELO.
//! This is the only analyze module coupled to the parser; the pure logic lives elsewhere.

use std::collections::HashMap;

use aoe2rec::actions::ActionData;
use aoe2rec::{Operation, PostGameBlock, Savegame};

use crate::analyze::model::{Ev, EvKind, GameMeta, PlayerInfo};

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

#[derive(Default)]
pub struct EloTable {
    by: HashMap<(u32, i32), i32>, // (ladder_id, game_player_number) -> elo
}

impl EloTable {
    /// Leaderboard `player_number` is 0-indexed; game player_number = lb + 1.
    pub fn insert(&mut self, ladder: u32, lb_player_number: i32, elo: i32) {
        self.by.insert((ladder, lb_player_number + 1), elo);
    }
    pub fn elo(&self, player_number: i32, ladder: u32) -> Option<i32> {
        self.by.get(&(ladder, player_number)).copied()
    }
}

pub struct Walked {
    pub meta: GameMeta,
    pub players: Vec<PlayerInfo>,
    pub evs: Vec<Ev>,
    pub elo: EloTable,
    /// one count per raw Operation::Action per player (BEFORE DeQueue amount-expansion) —
    /// the basis for eAPM. The rec logs no selects/camera, so this is already "effective".
    pub actions: HashMap<i32, u32>,
}

/// player_id position differs per ActionData variant; we only need it for the variants we map.
fn pid(ad: &ActionData) -> i32 {
    match ad {
        ActionData::DeQueue { player_id, .. }
        | ActionData::Research { player_id, .. }
        | ActionData::Build { player_id, .. }
        | ActionData::Move { player_id, .. }
        | ActionData::Interact { player_id, .. } => *player_id as i32,
        _ => 0,
    }
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

    for op in &game.operations {
        match op {
            Operation::Action { action_data, world_time, .. } => {
                let p = pid(action_data);
                *actions.entry(p).or_insert(0) += 1; // one per raw command (eAPM basis)
                let kind = match action_data {
                    ActionData::DeQueue { unit_id, amount, .. } => {
                        // expand `amount` to repeated Train events for cumulative counting.
                        // cap the expansion: a corrupt replay could carry a huge `amount`.
                        let n = (*amount).max(1).min(60);
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
                    _ => EvKind::Other,
                };
                evs.push(Ev { player: p, t_ms: *world_time, kind });
            }
            Operation::PostGame { blocks, .. } => {
                for b in blocks {
                    if let PostGameBlock::Leaderboards { leaderboards, .. } = b {
                        for lb in leaderboards {
                            for lp in &lb.players {
                                elo.insert(lb.id, lp.player_number, lp.elo);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let meta = GameMeta {
        map_id: gs.resolved_map_id,
        duration_ms: game.get_duration(), // parser's canonical duration (world_time + Sync increments)
        rec_player: game.zheader.replay.rec_player as i32,
    };

    Walked { meta, players, evs, elo, actions }
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

    #[test]
    fn leaderboard_index_maps_to_game_player_number() {
        // leaderboard pn is 0-indexed; game pn = lb + 1
        let mut t = EloTable::default();
        t.insert(3, 0, 1290);
        t.insert(3, 1, 1309);
        assert_eq!(t.elo(1, 3), Some(1290));
        assert_eq!(t.elo(2, 3), Some(1309));
        assert_eq!(t.elo(2, 4), None);
    }

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
