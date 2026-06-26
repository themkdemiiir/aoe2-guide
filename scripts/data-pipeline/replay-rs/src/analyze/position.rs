//! Light flank/pocket tag (color/slot rule) + coordinate metrics (enemy proximity,
//! time-to-find-enemy). The role is intentionally a simple slot rule, not geometry;
//! the coordinate code is the repurposed geometry the user asked for. source: spec.

use std::collections::HashMap;

use crate::analyze::model::{Ev, EvKind, Family, PlayerInfo, Role};

/// Within each team, min & max slot = Flank, inner = Pocket (documented ranked "1357 vs
/// 2468" convention). 2v2 special-case: lower slot = Flank, higher = Pocket. Gated to
/// >=2 teams (team_id 1 = unaffiliated/FFA) and Open|Closed maps only.
pub fn assign_roles(players: &[PlayerInfo], family: Family) -> HashMap<i32, Role> {
    let mut out = HashMap::new();
    if !matches!(family, Family::Open | Family::Closed) {
        return out;
    }
    let mut teams: HashMap<u8, Vec<i32>> = HashMap::new();
    for p in players {
        if p.team_id != 1 {
            teams.entry(p.team_id).or_default().push(p.player_number);
        }
    }
    if teams.len() < 2 {
        return out; // FFA / unresolved teams
    }
    for (_t, mut slots) in teams {
        slots.sort();
        let n = slots.len();
        for (i, pn) in slots.iter().enumerate() {
            let role = match n {
                1 => continue,
                2 => {
                    if i == 0 {
                        Role::Flank
                    } else {
                        Role::Pocket
                    }
                }
                _ => {
                    if i == 0 || i == n - 1 {
                        Role::Flank
                    } else {
                        Role::Pocket
                    }
                }
            };
            out.insert(*pn, role);
        }
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct CoordMetric {
    pub nearest_enemy_dist: Option<f32>,
    pub find_enemy_ms: Option<u32>,
}

const FIND_RADIUS: f32 = 15.0; // tiles; "a unit reached the enemy base region". source: spec estimate.

pub fn coord_metrics(
    evs: &[Ev],
    players: &[PlayerInfo],
    bases: &HashMap<i32, (f32, f32)>,
) -> HashMap<i32, CoordMetric> {
    let team: HashMap<i32, u8> = players.iter().map(|p| (p.player_number, p.team_id)).collect();
    let dist = |a: (f32, f32), b: (f32, f32)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    let mut out = HashMap::new();
    for p in players {
        let Some(&base) = bases.get(&p.player_number) else {
            out.insert(p.player_number, CoordMetric::default());
            continue;
        };
        let enemies: Vec<(f32, f32)> = bases
            .iter()
            .filter(|(pn, _)| team.get(pn) != Some(&p.team_id))
            .map(|(_, &b)| b)
            .collect();
        let nearest = enemies
            .iter()
            .map(|&e| dist(base, e))
            .filter(|d| d.is_finite())
            .min_by(f32::total_cmp);
        let find = evs
            .iter()
            .filter(|e| e.player == p.player_number)
            .filter_map(|e| {
                let c = match e.kind {
                    EvKind::Move { x, y } | EvKind::Interact { x, y } => Some((x, y)),
                    _ => None,
                }?;
                if enemies.iter().any(|&en| dist(c, en) < FIND_RADIUS) {
                    Some(e.t_ms)
                } else {
                    None
                }
            })
            .min();
        out.insert(p.player_number, CoordMetric { nearest_enemy_dist: nearest, find_enemy_ms: find });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pn: i32, team: u8) -> PlayerInfo {
        PlayerInfo {
            player_number: pn,
            civ_id: 1,
            name: String::new(),
            color_id: pn,
            team_id: team,
            won: None,
        }
    }

    #[test]
    fn roles_4v4_min_max_slot_are_flanks() {
        let ps = vec![p(1, 2), p(3, 2), p(5, 2), p(7, 2), p(2, 3), p(4, 3), p(6, 3), p(8, 3)];
        let r = assign_roles(&ps, Family::Open);
        assert_eq!(r[&1], Role::Flank);
        assert_eq!(r[&7], Role::Flank);
        assert_eq!(r[&3], Role::Pocket);
        assert_eq!(r[&5], Role::Pocket);
    }

    #[test]
    fn roles_skip_when_family_other_or_one_team() {
        let ps = vec![p(1, 2), p(3, 2), p(2, 3), p(4, 3)];
        assert!(assign_roles(&ps, Family::Water).is_empty()); // out-of-scope map
        let one = vec![p(1, 1), p(2, 1), p(3, 1), p(4, 1)]; // all team 1 = FFA
        assert!(assign_roles(&one, Family::Open).is_empty());
    }

    #[test]
    fn roles_2v2_lower_slot_flank_higher_pocket() {
        let ps = vec![p(1, 2), p(3, 2), p(2, 3), p(4, 3)];
        let r = assign_roles(&ps, Family::Open);
        assert_eq!(r[&1], Role::Flank);
        assert_eq!(r[&3], Role::Pocket);
    }

    #[test]
    fn coord_metrics_nearest_enemy_and_find_time() {
        let ps = vec![p(1, 2), p(2, 3)];
        let mut bases = HashMap::new();
        bases.insert(1, (10.0, 10.0));
        bases.insert(2, (50.0, 10.0)); // 40 tiles away
        // p1 moves a unit next to p2's base at t=120s
        let evs = vec![Ev { player: 1, t_ms: 120_000, kind: EvKind::Move { x: 52.0, y: 11.0 } }];
        let m = coord_metrics(&evs, &ps, &bases);
        assert!((m[&1].nearest_enemy_dist.unwrap() - 40.0).abs() < 0.1);
        assert_eq!(m[&1].find_enemy_ms, Some(120_000));
        assert_eq!(m[&2].find_enemy_ms, None);
    }
}
