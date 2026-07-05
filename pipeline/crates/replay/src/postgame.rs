//! Post-game leaderboard ELO — read out of a replay's `Operation::PostGame ->
//! PostGameBlock::Leaderboards` block.
//!
//! Ported verbatim from the OLD extractor's `scripts/data-pipeline/replay-rs/src/postgame.rs`
//! (itself factored out so the CLI-only `extract` and the WASM-bound `analyzer` crate shared one
//! walk). `parse::parse` calls [`collect_leaderboard_elo`] while walking the operation stream,
//! exactly as the old `extract::extract` did.

use std::collections::HashMap;

use aoe2rec::PostGameBlock;

/// Per-`(ladder_id, game_player_number)` ranked ELO, read from a replay's post-game leaderboard
/// blocks. Ladder ids: 3 = 1v1 RM, 4 = team RM.
#[derive(Default)]
pub(crate) struct EloTable {
    by: HashMap<(u32, i32), i32>, // (ladder_id, game_player_number) -> elo
}

impl EloTable {
    /// Leaderboard `player_number` is 0-indexed; game player_number = lb + 1.
    pub(crate) fn insert(&mut self, ladder: u32, lb_player_number: i32, elo: i32) {
        self.by.insert((ladder, lb_player_number + 1), elo);
    }
    pub(crate) fn elo(&self, player_number: i32, ladder: u32) -> Option<i32> {
        self.by.get(&(ladder, player_number)).copied()
    }
}

/// Walk one `Operation::PostGame`'s blocks and merge any `Leaderboards` entries into `elo`.
/// Merges rather than replaces so a caller can fold this over every `PostGame` operation in the
/// replay (in practice there's at most one, but nothing here assumes that).
pub(crate) fn collect_leaderboard_elo(blocks: &[PostGameBlock], elo: &mut EloTable) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use aoe2rec::{Leaderboard, LeaderboardPlayer};

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

    /// The shared helper, exercised against a hand-built `PostGameBlock` (no real replay
    /// fixtures are committed to this crate) — proves it maps `(ladder_id, player_number) ->
    /// elo` correctly, including the none-when-absent case (never fabricate a value for a
    /// missing ladder/player).
    #[test]
    fn collect_leaderboard_elo_reads_postgame_blocks() {
        let blocks = vec![PostGameBlock::Leaderboards {
            length: 0,
            num_leaderboards: 1,
            leaderboards: vec![Leaderboard {
                id: 3, // 1v1 RM
                unknown1: 0,
                num_players: 2,
                players: vec![
                    LeaderboardPlayer {
                        player_number: 0,
                        rank: 1,
                        elo: 1650,
                    },
                    LeaderboardPlayer {
                        player_number: 1,
                        rank: 2,
                        elo: 1590,
                    },
                ],
            }],
            realignment_field: (),
        }];
        let mut elo = EloTable::default();
        collect_leaderboard_elo(&blocks, &mut elo);
        assert_eq!(elo.elo(1, 3), Some(1650));
        assert_eq!(elo.elo(2, 3), Some(1590));
        // absent ladder / absent player number -> honest None, never fabricated
        assert_eq!(elo.elo(1, 4), None);
        assert_eq!(elo.elo(3, 3), None);
    }

    #[test]
    fn non_leaderboard_blocks_are_ignored() {
        let blocks = vec![PostGameBlock::WorldTime {
            length: 0,
            world_time: 12_345,
        }];
        let mut elo = EloTable::default();
        collect_leaderboard_elo(&blocks, &mut elo);
        assert_eq!(elo.elo(1, 3), None);
    }
}
