//! Best-effort floating-resources estimate. There is no resource data in the command
//! stream, so model income (villagers × a nominal rate) minus known spend (from
//! costs.json) and report windows where the bank visibly grows. source: spec.

use crate::analyze::data::Costs;
use crate::analyze::metrics::vils_at;
use crate::analyze::model::{Ev, EvKind};

pub const CAVEAT: &str = "Float is ESTIMATED (income≈villagers×0.30 res/s minus known spend); it ignores \
villager assignment, walk distance, civ bonuses, market trades, upgrades, and deaths — it flags big floats, not exact banks.";

const GATHER_PER_S: f64 = 0.30; // source: spec effective blended rate
const STEP_MS: u32 = 30_000; // 30s sampling
const MIN_GAP_PER_MIN: f64 = 150.0; // only report windows where the bank grows faster than this

#[derive(Debug, Clone)]
pub struct FloatWindow {
    pub start_ms: u32,
    pub end_ms: u32,
    pub rate_gap_per_min: f64,
    pub approx_banked: f64,
}

fn spend_to(evs: &[Ev], player: i32, t: u32, c: &Costs) -> f64 {
    evs.iter()
        .filter(|e| e.player == player && e.t_ms <= t)
        .map(|e| {
            let cost = match e.kind {
                EvKind::Train(u) => c.unit.get(&u).copied(),
                EvKind::Build { id, .. } => c.building.get(&id).copied(),
                EvKind::Research(t) => c.tech.get(&t).copied(),
                _ => None,
            };
            cost.map(|x| x.food + x.wood + x.gold + x.stone).unwrap_or(0.0)
        })
        .sum()
}

pub fn estimate(evs: &[Ev], player: i32, c: &Costs, duration_ms: u32) -> Vec<FloatWindow> {
    let mut out = Vec::new();
    let income_to = |t: u32| {
        // integrate villagers(t)*rate by stepping; cheap and good enough
        let mut acc = 0.0;
        let mut s = 0u32;
        while s < t {
            acc += vils_at(evs, player, s) as f64 * GATHER_PER_S * (STEP_MS as f64 / 1000.0);
            s += STEP_MS;
        }
        acc
    };
    let surplus_at = |t: u32| (income_to(t) - spend_to(evs, player, t, c)).max(0.0);
    // track the surplus AT WINDOW START so rate_gap_per_min is a true per-minute rate.
    let mut win: Option<(u32, f64)> = None; // (start_ms, surplus_at_start)
    let mut prev_surplus = 0.0;
    let close = |start: u32, start_surplus: f64, end: u32, end_surplus: f64, out: &mut Vec<FloatWindow>| {
        let mins = ((end - start) as f64 / 60_000.0).max(STEP_MS as f64 / 60_000.0);
        let banked = (end_surplus - start_surplus).max(0.0);
        out.push(FloatWindow {
            start_ms: start,
            end_ms: end,
            rate_gap_per_min: banked / mins,
            approx_banked: banked,
        });
    };
    let mut t = STEP_MS;
    while t <= duration_ms {
        let surplus = surplus_at(t);
        let gap_per_min = (surplus - prev_surplus) / (STEP_MS as f64 / 60_000.0);
        if gap_per_min >= MIN_GAP_PER_MIN {
            win.get_or_insert((t - STEP_MS, prev_surplus));
        } else if let Some((start, start_surplus)) = win.take() {
            close(start, start_surplus, t, surplus, &mut out);
        }
        prev_surplus = surplus;
        t += STEP_MS;
    }
    if let Some((start, start_surplus)) = win {
        close(start, start_surplus, duration_ms, prev_surplus, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::data::Costs;
    use crate::analyze::model::{Ev, EvKind};
    use std::collections::HashMap;

    fn empty_costs() -> Costs {
        Costs { unit: HashMap::new(), building: HashMap::new(), tech: HashMap::new() }
    }

    #[test]
    fn pure_income_no_spend_is_one_big_float() {
        // 10 villagers from t≈0, no spend, over 600s => income ~ 10*0.30*600 = 1800 res banked.
        let evs: Vec<Ev> = (0..10).map(|k| Ev { player: 1, t_ms: k, kind: EvKind::Train(83) }).collect();
        let w = estimate(&evs, 1, &empty_costs(), 600_000);
        assert!(!w.is_empty());
        let banked: f64 = w.iter().map(|x| x.approx_banked).sum();
        assert!(banked > 1000.0, "banked={banked}");
        // the per-minute rate is a rate, not the absolute bank
        assert!(w[0].rate_gap_per_min < banked);
    }
}
