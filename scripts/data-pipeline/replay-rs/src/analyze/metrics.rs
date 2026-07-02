//! Pure per-player metric functions over a time-sorted `&[Ev]`. No parser coupling,
//! so every function unit-tests with hand-built event vectors.

use crate::analyze::model::{Ev, EvKind};
use crate::config::{ECO_UNIT_IDS, VILLAGER_ID}; // reuse the validated parser constants — NO duplication

pub const VIL_TRAIN_MS: u32 = 25_000; // source: villager train time 25s

/// Watched eco upgrades + display name. source: AoE2 tech ids (verified vs aoe2techtree
/// data.json: 22 Loom, 213 Wheel Barrow, 249 Hand Cart, 202 Double Bit Axe, 203 Bow Saw,
/// 14 Horse Collar, 13 Heavy Plow, 55 Gold Mining, 182 Gold Shaft Mining, 278 Stone Mining).
pub const WATCHED_TECHS: &[(u16, &str)] = &[
    (22, "Loom"),
    (213, "Wheelbarrow"),
    (249, "Hand Cart"),
    (202, "Double-Bit Axe"),
    (203, "Bow Saw"),
    (14, "Horse Collar"),
    (13, "Heavy Plow"),
    (55, "Gold Mining"),
    (182, "Gold Shaft Mining"),
    (278, "Stone Mining"),
];

fn first_research(evs: &[Ev], player: i32, tech: u16) -> Option<u32> {
    evs.iter()
        .filter(|e| e.player == player)
        .filter_map(|e| match e.kind {
            EvKind::Research(t) if t == tech => Some(e.t_ms),
            _ => None,
        })
        .min()
}

/// (feudal, castle, imperial) age-up CLICK times in ms. source: replay techs 101/102/103
/// (= feudal/castle/imperial; see config::age_name — NOT the aoe2techtree data.json names).
pub fn age_clicks(evs: &[Ev], player: i32) -> (Option<u32>, Option<u32>, Option<u32>) {
    (
        first_research(evs, player, 101),
        first_research(evs, player, 102),
        first_research(evs, player, 103),
    )
}

/// Cumulative villagers queued by `until_ms` (DeQueue of unit 83; amounts pre-expanded by walk).
pub fn vils_at(evs: &[Ev], player: i32, until_ms: u32) -> u32 {
    evs.iter()
        .filter(|e| e.player == player && e.t_ms <= until_ms)
        .filter(|e| matches!(e.kind, EvKind::Train(VILLAGER_ID)))
        .count() as u32
}

/// Crude idle-TC estimate over [0, until]: window minus (villagers * 25s busy time).
/// Honest v1: assumes ~1 TC in the early game; flags BIG idle, not exact gaps. source: spec.
pub fn idle_tc_ms(evs: &[Ev], player: i32, until_ms: u32) -> u32 {
    let busy = vils_at(evs, player, until_ms).saturating_mul(VIL_TRAIN_MS);
    until_ms.saturating_sub(busy)
}

/// First trained non-eco unit time = first military. source: config::ECO_UNIT_IDS.
pub fn first_military_ms(evs: &[Ev], player: i32) -> Option<u32> {
    evs.iter()
        .filter(|e| e.player == player)
        .filter_map(|e| match e.kind {
            EvKind::Train(u) if !ECO_UNIT_IDS.contains(&u) => Some(e.t_ms),
            _ => None,
        })
        .min()
}

/// Watched eco upgrades and their first-research time.
pub fn eco_tech_times(evs: &[Ev], player: i32) -> Vec<(u16, u32)> {
    WATCHED_TECHS
        .iter()
        .filter_map(|&(id, _)| first_research(evs, player, id).map(|t| (id, t)))
        .collect()
}

// Feudal-opening unit lines. source: .cache/aoe2-data data.json, ids verified by COST
// (internal names are legacy-shifted): 74 Militia 50F/20G, 75 Man-at-Arms, 93 Spearman
// 35F/25W, 4 Archer, 7 Skirmisher 25F/35W, 448 Scout Cavalry, 751 Eagle Scout.
const MILITIA_LINE: [u16; 2] = [74, 75];
const OPENER_LINES: &[(&[u16], &str)] = &[
    (&[448], "Scouts"),
    (&[4], "Archers"),
    (&[7], "Skirms"),
    (&[751], "Eagles"),
    (&[74, 75], "M@A"),
    (&[93], "Spears"),
];

/// Cumulative eco/military units QUEUED per minute (index = minute since start).
/// Same DeQueue basis as vils@Castle / first-military — commands, not completions.
pub fn production_series(evs: &[Ev], player: i32, duration_ms: u32) -> (Vec<u32>, Vec<u32>) {
    let mins = (duration_ms / 60_000) as usize + 1;
    let mut eco = vec![0u32; mins];
    let mut mil = vec![0u32; mins];
    for e in evs.iter().filter(|e| e.player == player) {
        if let EvKind::Train(u) = e.kind {
            let m = (e.t_ms / 60_000) as usize;
            if m >= mins {
                continue; // action logged past the canonical duration — ignore
            }
            if ECO_UNIT_IDS.contains(&u) {
                eco[m] += 1;
            } else {
                mil[m] += 1;
            }
        }
    }
    for i in 1..mins {
        eco[i] += eco[i - 1];
        mil[i] += mil[i - 1];
    }
    (eco, mil)
}

// Building sets for the eco/military action split. source: .cache/aoe2-data data.json
// Building table, ids verified by COST (70 House 25W, 68 Mill 100W, 50 Farm 60W,
// 562 Lumber Camp 100W, 584 Mining Camp 100W, 45 Dock 150W, 84 Market 175W,
// 199 Fish Trap 100W, 109/621 Town Center | 12 Barracks 175W, 87 Archery Range 175W,
// 101 Stable 175W, 49 Siege Workshop 200W, 82 Castle 650S, 79 Watch Tower 35W/125S).
pub const ECO_BUILDINGS: [i64; 10] = [70, 68, 50, 562, 584, 45, 84, 199, 109, 621];
pub const MIL_BUILDINGS: [i64; 6] = [12, 87, 101, 49, 82, 79];
pub const AGE_TECHS: [u16; 3] = [101, 102, 103]; // feudal/castle/imperial (config::age_name)

/// Classify ONE command's event kind as eco/military; None = unattributable
/// (moves, attacks, unknown buildings, non-watched techs). Called by walk on the
/// per-COMMAND kind (before DeQueue amount-expansion), so the counts share the
/// `apm` command basis. These are lower bounds, not a partition of apm.
pub fn classify_cmd(kind: &EvKind) -> Option<bool /* is_eco */> {
    match *kind {
        EvKind::Train(u) => Some(ECO_UNIT_IDS.contains(&u)),
        EvKind::Build { id, .. } => {
            if ECO_BUILDINGS.contains(&id) {
                Some(true)
            } else if MIL_BUILDINGS.contains(&id) {
                Some(false)
            } else {
                None
            }
        }
        EvKind::Research(t) => (AGE_TECHS.contains(&t)
            || WATCHED_TECHS.iter().any(|&(id, _)| id == t))
        .then_some(true),
        _ => None,
    }
}

/// Market Buy/Sell command counts (resource/amount live in an undecoded blob).
pub fn market_counts(evs: &[Ev], player: i32) -> (u32, u32) {
    let mut buys = 0;
    let mut sells = 0;
    for e in evs.iter().filter(|e| e.player == player) {
        match e.kind {
            EvKind::MarketBuy => buys += 1,
            EvKind::MarketSell => sells += 1,
            _ => {}
        }
    }
    (buys, sells)
}

/// Rule-based opening tag (light port of dj0wns/AoE_Rec_Opening_Analysis):
/// dark-age militia ≥3 = Drush; then the first two DISTINCT unit lines opened in
/// the Feudal window, in train order ("Scouts into Archers"); no feudal military
/// but Castle reached = "Fast Castle". None when there's nothing to say (no
/// Feudal, or a passive Feudal with no Castle) — never guess.
pub fn classify_opening(
    evs: &[Ev],
    player: i32,
    feudal_ms: Option<u32>,
    castle_ms: Option<u32>,
) -> Option<String> {
    let feudal = feudal_ms?;
    let castle_or = castle_ms.unwrap_or(feudal + 12 * 60_000);
    let dark_militia = evs
        .iter()
        .filter(|e| e.player == player && e.t_ms < feudal)
        .filter(|e| matches!(e.kind, EvKind::Train(u) if MILITIA_LINE.contains(&u)))
        .count();

    let mut opened: Vec<(u32, &str)> = OPENER_LINES
        .iter()
        .filter_map(|(ids, tag)| {
            evs.iter()
                .filter(|e| e.player == player && e.t_ms >= feudal && e.t_ms < castle_or)
                .filter_map(|e| match e.kind {
                    EvKind::Train(u) if ids.contains(&u) => Some(e.t_ms),
                    _ => None,
                })
                .min()
                .map(|t| (t, *tag))
        })
        .collect();
    opened.sort_by_key(|&(t, _)| t);

    let feudal_tags: Vec<&str> = opened.iter().take(2).map(|&(_, tag)| tag).collect();
    let body = match feudal_tags.as_slice() {
        [] if castle_ms.is_some() => Some("Fast Castle".to_string()),
        [] => None,
        [one] => Some((*one).to_string()),
        [a, b] => Some(format!("{a} into {b}")),
        _ => unreachable!(),
    }?;
    Some(if dark_militia >= 3 { format!("Drush + {body}") } else { body })
}

fn median(v: Vec<f32>) -> Option<f32> {
    // filter non-finite (a junk Build/Move blob can decode to NaN) so the sort is total + safe
    let mut v: Vec<f32> = v.into_iter().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f32::total_cmp);
    Some(v[v.len() / 2])
}

/// Base = median (x,y) of the player's Build coords within `window_ms`. source: spec
/// (buildings are placed at home; cleaner than Move, which the scout pollutes).
pub fn base_centroid(evs: &[Ev], player: i32, window_ms: u32) -> Option<(f32, f32)> {
    let xs: Vec<f32> = evs
        .iter()
        .filter(|e| e.player == player && e.t_ms <= window_ms)
        .filter_map(|e| match e.kind {
            EvKind::Build { x, .. } => Some(x),
            _ => None,
        })
        .collect();
    let ys: Vec<f32> = evs
        .iter()
        .filter(|e| e.player == player && e.t_ms <= window_ms)
        .filter_map(|e| match e.kind {
            EvKind::Build { y, .. } => Some(y),
            _ => None,
        })
        .collect();
    match (median(xs), median(ys)) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    }
}

// APM is computed in `walk` (one count per raw Operation::Action, before the DeQueue
// amount-expansion) so a single shift-queue click isn't scored as N actions.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::model::{Ev, EvKind};

    fn ev(p: i32, t: u32, k: EvKind) -> Ev {
        Ev { player: p, t_ms: t, kind: k }
    }

    #[test]
    fn age_clicks_returns_earliest_of_each_age() {
        // age_clicks uses .min() over t_ms, so the earliest Research(101) wins.
        let evs = vec![
            ev(1, 600_000, EvKind::Research(101)),   // feudal
            ev(1, 1_000_000, EvKind::Research(102)), // castle
            ev(2, 700_000, EvKind::Research(101)),
        ];
        assert_eq!(age_clicks(&evs, 1), (Some(600_000), Some(1_000_000), None));
        assert_eq!(age_clicks(&evs, 2), (Some(700_000), None, None));
    }

    #[test]
    fn vils_at_counts_villager_trains_until_t() {
        let evs = vec![
            ev(1, 1000, EvKind::Train(83)),
            ev(1, 2000, EvKind::Train(83)),
            ev(1, 3000, EvKind::Train(38)), // knight, not a villager
            ev(1, 9000, EvKind::Train(83)),
        ];
        assert_eq!(vils_at(&evs, 1, 5000), 2);
        assert_eq!(vils_at(&evs, 1, 100_000), 3);
    }

    #[test]
    fn idle_tc_is_window_minus_busy() {
        // 2 villagers by 200s, window=200s => busy=2*25s=50s => idle=150s.
        let evs = vec![ev(1, 10_000, EvKind::Train(83)), ev(1, 20_000, EvKind::Train(83))];
        assert_eq!(idle_tc_ms(&evs, 1, 200_000), 150_000);
        // never negative
        let many: Vec<Ev> = (0..20).map(|k| ev(1, k * 1000, EvKind::Train(83))).collect();
        assert_eq!(idle_tc_ms(&many, 1, 100_000), 0);
    }

    #[test]
    fn first_military_is_first_non_eco_train() {
        let evs = vec![ev(1, 1000, EvKind::Train(83)), ev(1, 5000, EvKind::Train(7))];
        assert_eq!(first_military_ms(&evs, 1), Some(5000));
    }

    #[test]
    fn base_centroid_is_median_of_builds() {
        let evs = vec![
            ev(1, 1000, EvKind::Build { id: 70, x: 10.0, y: 70.0 }),
            ev(1, 2000, EvKind::Build { id: 70, x: 14.0, y: 74.0 }),
            ev(1, 3000, EvKind::Build { id: 562, x: 12.0, y: 72.0 }),
            ev(1, 999_000, EvKind::Build { id: 70, x: 200.0, y: 5.0 }), // outside window, ignored
        ];
        assert_eq!(base_centroid(&evs, 1, 360_000), Some((12.0, 72.0)));
    }

    #[test]
    fn eco_tech_times_picks_watched_only() {
        let evs = vec![
            ev(1, 100_000, EvKind::Research(22)),  // Loom
            ev(1, 200_000, EvKind::Research(999)), // not watched
            ev(1, 300_000, EvKind::Research(213)), // Wheelbarrow
        ];
        let got = eco_tech_times(&evs, 1);
        assert!(got.contains(&(22, 100_000)));
        assert!(got.contains(&(213, 300_000)));
        assert_eq!(got.len(), 2);
    }
}
