//! Render the terminal report: a per-player scorecard table (comfy-table) + a
//! prioritized "Top Fixes" list. Color (owo-colors) is applied only when stdout is a
//! TTY, so piped/tested output stays plain.

use std::collections::HashMap;
use std::io::IsTerminal;

use comfy_table::Table;
use owo_colors::OwoColorize;

use crate::analyze::float;
use crate::analyze::model::{Basis, Family, Finding, GameMeta, PlayerMetrics, Role, Severity};

/// AoE2 DE `color_id` is the 1-indexed slot color (1=Blue … 8=Orange), and the 8th color
/// is stored as `0` — verified by an 8-player replay whose color_ids were a clean sequential
/// 1,2,3,4,5,6,7,0 (Blue…Gray,Orange). source: colors research + observed replays.
fn color_name(id: i32) -> &'static str {
    match id {
        1 => "Blue",
        2 => "Red",
        3 => "Green",
        4 => "Yellow",
        5 => "Cyan",
        6 => "Purple",
        7 => "Gray",
        0 | 8 => "Orange",
        _ => "?",
    }
}

fn opt_mmss(ms: Option<u32>) -> String {
    ms.map(|m| format!("{}:{:02}", m / 60000, (m / 1000) % 60))
        .unwrap_or_else(|| "—".into())
}

fn sev_symbol(s: Severity) -> &'static str {
    match s {
        Severity::High => "✗",
        Severity::Med => "⚠",
        Severity::Low => "·",
    }
}

pub fn render(
    meta: &GameMeta,
    map_name: &str,
    family: Family,
    metrics: &[PlayerMetrics],
    findings: &[Finding],
    you: i32,
) -> String {
    let color = std::io::stdout().is_terminal();
    let names: HashMap<i32, String> = metrics
        .iter()
        .map(|m| (m.info.player_number, m.info.name.clone()))
        .collect();
    let mut s = String::new();

    s.push_str(&format!(
        "\nAoE2 Game Analysis — {} players · {} · {}:{:02}\n\n",
        metrics.len(),
        map_name,
        meta.duration_ms / 60000,
        (meta.duration_ms / 1000) % 60
    ));

    // --- scorecard ---
    let mut t = Table::new();
    t.set_header(vec![
        "Player", "Role", "Feudal", "Castle", "Imp", "Vils@C", "IdleDk", "APM", "ELO", "Result",
    ]);
    for m in metrics {
        let you_mark = if m.info.player_number == you { "➤ " } else { "  " };
        let role = match m.role {
            Some(Role::Flank) => "flank",
            Some(Role::Pocket) => "pocket",
            None => "-",
        };
        let elo = m
            .elo_team
            .or(m.elo_1v1)
            .map(|e| e.to_string())
            .unwrap_or_else(|| "-".into());
        let result = match m.info.won {
            Some(true) => "win",
            Some(false) => "loss",
            None => "-",
        };
        t.add_row(vec![
            format!("{}{} ({})", you_mark, m.info.name, color_name(m.info.color_id)),
            role.to_string(),
            opt_mmss(m.feudal_ms),
            opt_mmss(m.castle_ms),
            opt_mmss(m.imperial_ms),
            if m.castle_ms.is_some() { m.vils_castle.to_string() } else { "—".into() },
            format!("{}s", m.idle_dark_ms / 1000),
            format!("{:.0}", m.apm),
            elo,
            result.to_string(),
        ]);
    }
    s.push_str(&t.to_string());
    s.push('\n');

    // --- scouting & position (only when base geometry resolved, i.e. open/closed maps) ---
    let coord_lines: Vec<String> = metrics
        .iter()
        .filter_map(|m| {
            let d = m.nearest_enemy_dist?;
            let scout = m
                .find_enemy_ms
                .map(|t| format!("found enemy {}:{:02}", t / 60000, (t / 1000) % 60))
                .unwrap_or_else(|| "never reached enemy".into());
            Some(format!("  {}: nearest enemy ~{:.0} tiles · {}", m.info.name, d, scout))
        })
        .collect();
    // open maps only: on closed/water you're walled, so "never reached enemy" is normal.
    if family == Family::Open && !coord_lines.is_empty() {
        s.push_str("\nScouting & position\n");
        for l in &coord_lines {
            s.push_str(l);
            s.push('\n');
        }
    }

    // --- Top Fixes (findings are pre-sorted by severity desc) ---
    s.push_str("\nTop Fixes\n");
    if findings.is_empty() {
        s.push_str("  (no major macro issues flagged — nice and clean)\n");
    }
    for f in findings.iter().take(8) {
        let sym = sev_symbol(f.severity);
        let sym = if color {
            match f.severity {
                Severity::High => sym.red().to_string(),
                Severity::Med => sym.yellow().to_string(),
                Severity::Low => sym.dimmed().to_string(),
            }
        } else {
            sym.to_string()
        };
        let who = names.get(&f.player_number).cloned().unwrap_or_default();
        let basis = match f.basis {
            Basis::YourElo => " [vs your elo]",
            Basis::Pro => " [vs 2500+]",
            Basis::Opponent => " [vs opponent]",
            Basis::Absolute => "",
        };
        s.push_str(&format!(
            "  {sym} {who}: {} — you {} (ref {}){basis} — {}\n",
            f.metric, f.your, f.reference, f.note
        ));
    }

    // --- honest footer ---
    s.push_str("\n");
    s.push_str(&format!("note: {}\n", float::CAVEAT));
    s.push_str("note: macro coach only — no fights, micro, map control, or exact resources.\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::model::{Basis, PlayerInfo};

    fn pm(name: &str, pn: i32) -> PlayerMetrics {
        PlayerMetrics {
            info: PlayerInfo {
                player_number: pn,
                civ_id: 2,
                name: name.into(),
                color_id: pn,
                team_id: 2,
                won: Some(true),
            },
            feudal_ms: Some(720_000),
            castle_ms: Some(1_020_000),
            imperial_ms: None,
            vils_castle: 40,
            idle_dark_ms: 30_000,
            idle_feudal_ms: 0,
            first_military_ms: Some(700_000),
            eco_techs: vec![(22, 600_000)],
            apm: 95.0,
            elo_1v1: Some(1300),
            elo_team: None,
            role: None,
            float_peak_rate: 0.0,
            float_banked: 0.0,
            float_window: None,
            nearest_enemy_dist: None,
            find_enemy_ms: None,
        }
    }

    #[test]
    fn report_contains_player_and_top_fix() {
        let meta = GameMeta {
            map_id: 9,
            duration_ms: 2_400_000,
            rec_player: 1,
        };
        let metrics = vec![pm("MKD", 1)];
        let findings = vec![Finding {
            player_number: 1,
            metric: "idle TC (dark)".into(),
            your: "30s".into(),
            reference: "~0-15s".into(),
            basis: Basis::Absolute,
            severity: Severity::High,
            note: "tighten production".into(),
        }];
        let s = render(&meta, "Arabia", Family::Open, &metrics, &findings, 1);
        assert!(s.contains("Arabia"));
        assert!(s.contains("Top Fixes"));
        assert!(s.contains("MKD"));
        assert!(s.contains("idle TC (dark)"));
    }
}
