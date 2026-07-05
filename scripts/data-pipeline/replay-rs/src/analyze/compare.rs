//! Assemble per-player metrics from a walked replay, then turn them into prioritized
//! `Finding`s via the four comparisons (your-elo / pro / opponent / absolute waste).

use std::collections::HashMap;

use crate::analyze::data::{self, Benchmark, Costs};
use crate::analyze::model::{Basis, Family, Finding, PlayerMetrics, Role, Severity};
use crate::analyze::position::CoordMetric;
use crate::analyze::walk::Walked;
use crate::analyze::{float, metrics};

const FEUDAL_RES_S: f64 = 130.0; // baseline age-up research (data.json Tech 101/102/103 ResearchTime)
const CASTLE_RES_S: f64 = 160.0;
const IMP_RES_S: f64 = 190.0;

// Malay: "Advancing to the next Age is 66% faster" (src/data/civilizations.json malay bonus;
// AoE2 wiki / Liquipedia Feudal Age). "66% faster" = research-rate ×1.66 ⇒ time = base / 1.66,
// confirmed by the wiki's Malay Feudal 78s (=130/1.66) and Castle 96s (=160/1.66).
const MALAY_AGE_FACTOR: f64 = 1.0 / 1.66;

/// Click→completion research durations, civ-aware. Baseline 130/160/190s is the standard
/// no-bonus age-up research time; only civs with a sourced age-up speed bonus deviate.
fn age_research_s(civ: &str) -> (f64, f64, f64) {
    let m = match civ {
        "malay" => MALAY_AGE_FACTOR,
        _ => 1.0,
    };
    (FEUDAL_RES_S * m, CASTLE_RES_S * m, IMP_RES_S * m)
}

/// Compute every player's metrics. Roles + coordinate metrics are computed upstream
/// (they need cross-player context) and threaded in.
pub fn build_metrics(
    w: &Walked,
    costs: &Costs,
    roles: &HashMap<i32, Role>,
    coords: &HashMap<i32, CoordMetric>,
) -> Vec<PlayerMetrics> {
    let dur = w.meta.duration_ms;
    w.players
        .iter()
        .map(|info| {
            let pn = info.player_number;
            let (feudal_ms, castle_ms, imperial_ms) = metrics::age_clicks(&w.evs, pn);
            let vils_castle = castle_ms
                .map(|c| metrics::vils_at(&w.evs, pn, c))
                .unwrap_or_else(|| metrics::vils_at(&w.evs, pn, dur));
            let idle_dark_ms = feudal_ms.map(|f| metrics::idle_tc_ms(&w.evs, pn, f)).unwrap_or(0);
            let idle_feudal_ms = match (feudal_ms, castle_ms) {
                (Some(f), Some(c)) => {
                    metrics::idle_tc_ms(&w.evs, pn, c).saturating_sub(metrics::idle_tc_ms(&w.evs, pn, f))
                }
                _ => 0,
            };
            // peak floating-resource window (by rate) -> rate + banked + (start,end)
            let peak = float::estimate(&w.evs, pn, costs, dur)
                .into_iter()
                .max_by(|a, b| a.rate_gap_per_min.total_cmp(&b.rate_gap_per_min));
            let cm = coords.get(&pn).cloned().unwrap_or_default();
            let (vils_series, mil_series) = metrics::production_series(&w.evs, pn, dur);
            let mins = (dur as f64 / 60_000.0).max(1.0 / 60.0);
            let (eco_cmds, mil_cmds) = w.cmd_split.get(&pn).copied().unwrap_or((0, 0));
            let (market_buys, market_sells) = metrics::market_counts(&w.evs, pn);
            PlayerMetrics {
                info: info.clone(),
                feudal_ms,
                castle_ms,
                imperial_ms,
                vils_castle,
                idle_dark_ms,
                idle_feudal_ms,
                first_military_ms: metrics::first_military_ms(&w.evs, pn),
                eco_techs: metrics::eco_tech_times(&w.evs, pn),
                apm: w.actions.get(&pn).copied().unwrap_or(0) as f64
                    / (dur as f64 / 60_000.0).max(1.0 / 60.0),
                elo_1v1: w.elo.elo(pn, 3),
                elo_team: w.elo.elo(pn, 4),
                role: roles.get(&pn).copied(),
                float_peak_rate: peak.as_ref().map(|p| p.rate_gap_per_min).unwrap_or(0.0),
                float_banked: peak.as_ref().map(|p| p.approx_banked).unwrap_or(0.0),
                float_window: peak.as_ref().map(|p| (p.start_ms, p.end_ms)),
                nearest_enemy_dist: cm.nearest_enemy_dist,
                find_enemy_ms: cm.find_enemy_ms,
                opening: metrics::classify_opening(&w.evs, pn, feudal_ms, castle_ms),
                vils_series,
                mil_series,
                apm_series: w.action_series.get(&pn).cloned().unwrap_or_default(),
                eco_apm: eco_cmds as f64 / mins,
                mil_apm: mil_cmds as f64 / mins,
                market_buys,
                market_sells,
                reference: None, // attached after findings (attach_references)
            }
        })
        .collect()
}

/// More than 2 real players => treat as a team game (picks the team ELO + team benchmark slice).
pub fn is_team_game(metrics: &[PlayerMetrics]) -> bool {
    metrics.len() > 2
}

/// Attach each player's RESOLVED benchmark slice (same resolution the findings
/// use) so the UI can render a You|Opponent|Reference matrix without shipping
/// the whole benchmark to the client.
pub fn attach_references(
    metrics: &mut [PlayerMetrics],
    bench: &Benchmark,
    civs: &HashMap<u32, String>,
    map_slug: &str,
    mode: &str,
) {
    use crate::analyze::model::RefSlice;
    let team = mode == "team";
    for m in metrics.iter_mut() {
        let elo = if team { m.elo_team.or(m.elo_1v1) } else { m.elo_1v1.or(m.elo_team) };
        let (Some(civ), Some(elo)) = (civs.get(&m.info.civ_id).map(String::as_str), elo) else {
            continue;
        };
        let bucket = data::elo_bucket(elo);
        if let Some((s, kind)) = bench.slice(civ, map_slug, bucket, mode) {
            m.reference = Some(RefSlice {
                feudal_s: s.feudal_s,
                castle_s: s.castle_s,
                imperial_s: s.imperial_s,
                vils_castle: s.vils_castle,
                age_res_s: age_research_s(civ),
                bucket: bucket.to_string(),
                kind: match kind {
                    data::MatchKind::Exact => "exact",
                    data::MatchKind::MapMode => "map_mode",
                    data::MatchKind::MapAll => "map_all",
                }
                .to_string(),
            });
        }
    }
}

fn fmt_secs(ms: u32) -> String {
    format!("{}s", ms / 1000)
}
fn fmt_mmss(secs: f64) -> String {
    crate::config::mmss(secs.max(0.0) as u32)
}

/// mode is decided once by the caller (analyze()) and recorded in ReportMeta so the
/// findings and the report can never disagree. "team" | "1v1".
pub fn findings(
    metrics: &[PlayerMetrics],
    bench: &Benchmark,
    civs: &HashMap<u32, String>,
    family: Family,
    map_slug: &str,
    mode: &str,
) -> Vec<Finding> {
    let team = mode == "team";
    let mut out = Vec::new();

    for m in metrics {
        let pn = m.info.player_number;

        // --- absolute waste: idle TC (ESTIMATE = window − vils×25s; thresholds are lenient
        //     so a normal mid-elo player banking for an age-up isn't flagged) ---
        if m.idle_dark_ms > 70_000 {
            out.push(mk(pn, "idle_tc_dark_high", "idle TC (dark)", &fmt_secs(m.idle_dark_ms), "~0-30s", Basis::Absolute, Severity::High,
                "Lots of Dark-Age TC idle (estimate) — keep a villager queued non-stop."));
        } else if m.idle_dark_ms > 35_000 {
            out.push(mk(pn, "idle_tc_dark_med", "idle TC (dark)", &fmt_secs(m.idle_dark_ms), "~0-30s", Basis::Absolute, Severity::Med,
                "Some Dark-Age TC idle (estimate) — tighten villager production."));
        }
        if m.idle_feudal_ms > 50_000 {
            out.push(mk(pn, "idle_tc_feudal", "idle TC (feudal)", &fmt_secs(m.idle_feudal_ms), "<25s", Basis::Absolute, Severity::Med,
                "TC idle in Feudal (estimate) — each 25s idle ≈ one lost villager."));
        }

        // --- vs your-elo benchmark: age timing, MAP-SPECIFIC. Fires on any map that has data
        // (the benchmark's per-map medians already encode FC-on-Arena etc.), and returns None
        // for unknown maps so we never compare against an unrelated map. Pockets boom, so give
        // a later Castle more slack. Times convert click->completion (+research duration).
        let elo = if team { m.elo_team.or(m.elo_1v1) } else { m.elo_1v1.or(m.elo_team) };
        if let (Some(civ), Some(elo)) = (civs.get(&m.info.civ_id).map(String::as_str), elo) {
            let bucket = data::elo_bucket(elo);
            let (feudal_res, castle_res, imp_res) = age_research_s(civ);
            if let Some((s, kind)) = bench.slice(civ, map_slug, bucket, mode) {
                let (ref_desc, ref_kind) = match kind {
                    data::MatchKind::Exact => (format!("{bucket} {mode} median on {map_slug}"), "exact"),
                    data::MatchKind::MapMode => (format!("{map_slug} {mode} median (all elo)"), "mapmode"),
                    data::MatchKind::MapAll if mode == "1v1" => {
                        (format!("{map_slug} median (no 1v1 baseline — team-heavy rollup)"), "mapall_1v1")
                    }
                    data::MatchKind::MapAll => (format!("{map_slug} median (all elo/mode)"), "mapall"),
                };
                if let (Some(fms), Some(ref_s)) = (m.feudal_ms, s.feudal_s) {
                    let comp = fms as f64 / 1000.0 + feudal_res;
                    if comp > ref_s + 60.0 {
                        let sev = if comp > ref_s + 120.0 { Severity::High } else { Severity::Med };
                        let mut f = mk(pn, "feudal_slow", "Feudal up-time", &fmt_mmss(comp), &fmt_mmss(ref_s), Basis::YourElo, sev,
                            &format!("Slower to Feudal than the {ref_desc} for {civ}."));
                        f.vars.insert("ref_kind", ref_kind.to_string());
                        f.vars.insert("bucket", bucket.to_string());
                        f.vars.insert("mode", mode.to_string());
                        f.vars.insert("map", map_slug.to_string());
                        f.vars.insert("civ", civ.to_string());
                        out.push(f);
                    }
                }
                if let (Some(cms), Some(ref_s)) = (m.castle_ms, s.castle_s) {
                    let slack = if matches!(m.role, Some(Role::Pocket)) { 180.0 } else { 90.0 };
                    let comp = cms as f64 / 1000.0 + castle_res;
                    if comp > ref_s + slack {
                        let sev = if comp > ref_s + slack + 90.0 { Severity::High } else { Severity::Med };
                        let mut f = mk(pn, "castle_slow", "Castle up-time", &fmt_mmss(comp), &fmt_mmss(ref_s), Basis::YourElo, sev,
                            &format!("Slower to Castle than the {ref_desc}."));
                        f.vars.insert("ref_kind", ref_kind.to_string());
                        f.vars.insert("bucket", bucket.to_string());
                        f.vars.insert("mode", mode.to_string());
                        f.vars.insert("map", map_slug.to_string());
                        out.push(f);
                    }
                }
                if let (Some(ims), Some(ref_s)) = (m.imperial_ms, s.imperial_s) {
                    let comp = ims as f64 / 1000.0 + imp_res;
                    if comp > ref_s + 180.0 {
                        let mut f = mk(pn, "imperial_slow", "Imperial up-time", &fmt_mmss(comp), &fmt_mmss(ref_s), Basis::YourElo, Severity::Low,
                            &format!("Slower to Imperial than the {ref_desc}."));
                        f.vars.insert("ref_kind", ref_kind.to_string());
                        f.vars.insert("bucket", bucket.to_string());
                        f.vars.insert("mode", mode.to_string());
                        f.vars.insert("map", map_slug.to_string());
                        out.push(f);
                    }
                }
                // --- vs WINNERS at this slice: villagers trained by Castle Age.
                // Gap thresholds ≈ 2.5min / 5min of lost TC production (25s/vil).
                if let (Some(_), Some(ref_v)) = (m.castle_ms, s.vils_castle) {
                    let yours = m.vils_castle as f64;
                    if yours + 6.0 < ref_v {
                        let sev = if yours + 12.0 < ref_v { Severity::High } else { Severity::Med };
                        let mut f = mk(pn, "villagers_castle", "villagers by Castle", &m.vils_castle.to_string(), &format!("~{ref_v:.0}"),
                            Basis::YourElo, sev,
                            &format!("Winners in this bracket train ~{ref_v:.0} villagers by Castle Age — TC idle time usually explains the gap."));
                        f.vars.insert("ref_v", format!("{ref_v:.0}"));
                        out.push(f);
                    }
                }
            }
            // --- vs PRO (the exact 2500+ median on this map) — aspirational, only when far behind ---
            if let Some((p, data::MatchKind::Exact)) = bench.slice(civ, map_slug, "2500+", mode) {
                if let (Some(cms), Some(pro_s)) = (m.castle_ms, p.castle_s) {
                    let comp = cms as f64 / 1000.0 + castle_res;
                    if comp > pro_s + 240.0 {
                        let mut f = mk(pn, "castle_vs_pro", "Castle vs pro", &fmt_mmss(comp), &fmt_mmss(pro_s), Basis::Pro, Severity::Low,
                            &format!("Castle is {} behind the 2500+ {map_slug} median — the pace to aim for.",
                                fmt_mmss(comp - pro_s)));
                        f.vars.insert("gap", fmt_mmss(comp - pro_s));
                        f.vars.insert("map", map_slug.to_string());
                        out.push(f);
                    }
                }
            }
        }

        // --- absolute: missing key eco techs BY the age boundary (not just "ever") ---
        let researched_by = |id: u16, by: Option<u32>| {
            matches!((m.eco_techs.iter().find(|&&(t, _)| t == id), by), (Some(&(_, tt)), Some(b)) if tt <= b)
        };
        if m.castle_ms.is_some() && !researched_by(213, m.castle_ms) {
            out.push(mk(pn, "no_wheelbarrow", "Wheelbarrow", "not by Castle", "by Castle Age", Basis::Absolute, Severity::Med,
                "No Wheelbarrow by Castle Age — a big eco multiplier left on the table."));
        }
        if m.feudal_ms.is_some() && !researched_by(22, m.feudal_ms) {
            out.push(mk(pn, "no_loom", "Loom", "not by Feudal", "by Feudal", Basis::Absolute, Severity::Low,
                "No Loom by Feudal — cheap villager survivability vs early aggression."));
        }

        // --- absolute: estimated float (with the peak window) ---
        if m.float_peak_rate > 250.0 {
            let when = m
                .float_window
                .map(|(s, e)| format!(" ({}–{})", fmt_mmss(s as f64 / 1000.0), fmt_mmss(e as f64 / 1000.0)))
                .unwrap_or_default();
            out.push(mk(pn, "floating_res", "floating resources",
                &format!("~{:.0} res/min ≈ {:.0} banked{when}", m.float_peak_rate, m.float_banked),
                "reinvest", Basis::Absolute, Severity::Med,
                "Economy out-produced spending — add production buildings/villagers (estimate)."));
        }

        // --- absolute: late / no military (gentle; even a boom needs defense) ---
        if m.first_military_ms.is_none_or(|t| t > 20 * 60_000) {
            out.push(mk(pn, "late_military", "military timing",
                &m.first_military_ms.map(|t| fmt_mmss(t as f64 / 1000.0)).unwrap_or_else(|| "none".into()),
                "<20 min", Basis::Absolute, Severity::Low,
                "No/very-late military — even a fast-castle boom needs units before the enemy arrives."));
        }

        // --- scouting: found the enemy late or never — open maps only (you're walled in on
        //     closed/water, so "never reached enemy" is normal there, not a mistake) ---
        if family == Family::Open && m.nearest_enemy_dist.is_some() {
            match m.find_enemy_ms {
                None => out.push(mk(pn, "never_scouted", "scouting", "never reached enemy", "by ~6–8 min", Basis::Absolute, Severity::Med,
                    "Your units never reached the enemy base — scout to read their build and react.")),
                Some(t) if t > 8 * 60_000 => out.push(mk(pn, "scouted_late", "scouting", &fmt_mmss(t as f64 / 1000.0), "by ~6–8 min",
                    Basis::Absolute, Severity::Low, "Found the enemy late — scout earlier to read their strategy.")),
                _ => {}
            }
        }
    }

    // --- opponent head-to-head: who reached Feudal first (1v1 only, both timed) ---
    if !team && metrics.len() == 2 {
        if let (Some(a), Some(b)) = (metrics[0].feudal_ms, metrics[1].feudal_ms) {
            let (slow, fast, gap) = if a > b { (0usize, 1usize, a - b) } else { (1, 0, b - a) };
            if gap > 45_000 {
                let pn = metrics[slow].info.player_number;
                let mut f = mk(pn, "feudal_vs_opp", "Feudal vs opponent", &fmt_mmss(a.max(b) as f64 / 1000.0),
                    &fmt_mmss(a.min(b) as f64 / 1000.0), Basis::Opponent, Severity::Med,
                    &format!("Reached Feudal {}s after {} — you started the game behind.", gap / 1000, metrics[fast].info.name));
                f.vars.insert("gap", (gap / 1000).to_string());
                f.vars.insert("opp", metrics[fast].info.name.clone());
                out.push(f);
            }
        }
    }

    out.sort_by_key(|f| std::cmp::Reverse(f.severity));
    out
}

// A Finding constructor maps 1:1 to the struct's fields — 8 params is inherent, not a smell.
#[allow(clippy::too_many_arguments)]
fn mk(pn: i32, code: &'static str, metric: &str, your: &str, reference: &str, basis: Basis, severity: Severity, note: &str) -> Finding {
    Finding {
        player_number: pn,
        code,
        metric: metric.to_string(),
        your: your.to_string(),
        reference: reference.to_string(),
        basis,
        severity,
        note: note.to_string(),
        vars: std::collections::BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::data::load_benchmark;
    use crate::analyze::model::PlayerInfo;

    fn pm(pn: i32, idle_dark_ms: u32, feudal_ms: Option<u32>) -> PlayerMetrics {
        PlayerMetrics {
            info: PlayerInfo {
                player_number: pn,
                profile_id: 0,
                civ_id: 2,
                name: "MKD".into(),
                color_id: pn,
                team_id: 2,
                won: None,
            },
            feudal_ms,
            castle_ms: None,
            imperial_ms: None,
            vils_castle: 0,
            idle_dark_ms,
            idle_feudal_ms: 0,
            first_military_ms: Some(700_000), // keep military-timing finding out of these tests
            eco_techs: vec![],
            apm: 0.0,
            elo_1v1: None,
            elo_team: None,
            role: None,
            float_peak_rate: 0.0,
            float_banked: 0.0,
            float_window: None,
            nearest_enemy_dist: None,
            find_enemy_ms: None,
            opening: None,
            vils_series: vec![],
            mil_series: vec![],
            apm_series: vec![],
            eco_apm: 0.0,
            mil_apm: 0.0,
            market_buys: 0,
            market_sells: 0,
            reference: None,
        }
    }

    #[test]
    fn flags_high_dark_idle_tc() {
        let f = findings(&[pm(1, 80_000, Some(720_000))], &load_benchmark(), &HashMap::new(), Family::Open, "arabia", "1v1");
        assert!(f
            .iter()
            .any(|x| x.metric.contains("idle TC") && matches!(x.severity, Severity::High)));
    }

    #[test]
    fn every_finding_carries_a_code() {
        // Metrics that trip several categories (idle TC, slow Feudal, missing Loom):
        // every emitted finding must carry a non-empty stable code so the browser can
        // localize its metric + note.
        let mut civs = HashMap::new();
        civs.insert(2u32, "franks".to_string());
        let mut m = pm(1, 80_000, Some(900_000)); // high dark idle + 15:00 Feudal
        m.elo_1v1 = Some(1000);
        m.castle_ms = Some(1_500_000);
        let f = findings(&[m], &load_benchmark(), &civs, Family::Open, "arabia", "1v1");
        assert!(!f.is_empty(), "expected findings to test");
        for x in &f {
            assert!(!x.code.is_empty(), "finding for metric {:?} has no code", x.metric);
        }
    }

    #[test]
    fn benchmark_finding_fires_for_slow_feudal() {
        // franks (civ 2) at a slow Feudal (15:00 click) vs the ~11min benchmark.
        let mut civs = HashMap::new();
        civs.insert(2u32, "franks".to_string());
        let mut m = pm(1, 0, Some(900_000)); // 15:00 click
        m.elo_1v1 = Some(1300);
        let f = findings(&[m, pm(2, 0, Some(600_000))], &load_benchmark(), &civs, Family::Open, "arabia", "1v1");
        // 2 players => 1v1 mode; franks arabia 1v1 falls back to the arabia rollup.
        assert!(f.iter().any(|x| x.metric == "Feudal up-time" && x.basis == Basis::YourElo));
    }

    #[test]
    fn age_research_no_bonus_civ_uses_baseline() {
        assert_eq!(age_research_s("franks"), (130.0, 160.0, 190.0));
    }

    #[test]
    fn age_research_malay_faster_than_baseline() {
        let (f, c, i) = age_research_s("malay");
        assert!(f < 130.0 && c < 160.0 && i < 190.0);
        // 130 / 1.66 ≈ 78.31
        assert!((f - 130.0 / 1.66).abs() < 0.01);
    }
}
