//! Shared data types for the `analyze` subcommand.
//!
//! Split into: CLI inputs, the normalized event stream (`Ev`/`EvKind`), per-game and
//! per-player facts from the parser (`GameMeta`/`PlayerInfo`), and the analysis
//! outputs (`PlayerMetrics`, `Finding`). Pure metric functions live in `metrics.rs`;
//! nothing here touches `aoe2rec`.

use serde::Serialize;

// --- map families (from data/maps.tsv) ---------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Open,
    Closed,
    Hybrid,
    Water,
    Nomad,
    Special,
    Other,
}

// --- normalized event stream -------------------------------------------------
/// One player action, normalized away from aoe2rec's variant zoo. `Train` amounts
/// are pre-expanded by `walk` (one `Ev` per queued unit) so counting is a filter.
#[derive(Debug, Clone, Copy)]
pub enum EvKind {
    Train(u16),                       // DeQueue unit_id
    Research(u16),                    // technology_type
    Build { id: i64, x: f32, y: f32 }, // building_id + map position
    Move { x: f32, y: f32 },
    Interact { x: f32, y: f32 },
    MarketBuy,                        // resource/amount undecoded — count only
    MarketSell,
    Other,
}

#[derive(Debug, Clone, Copy)]
pub struct Ev {
    pub player: i32,
    pub t_ms: u32,
    pub kind: EvKind,
}

// --- per-game / per-player facts from the parser -----------------------------
#[derive(Debug, Clone)]
pub struct GameMeta {
    pub map_id: u32,
    pub duration_ms: u32,
    pub rec_player: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerInfo {
    pub player_number: i32,
    /// Relic profile id — stable player identity across matches. source: replay GameSettings
    /// players (same field the bulk pipeline's `players` table extracts).
    pub profile_id: i64,
    pub civ_id: u32,
    pub name: String,
    pub color_id: i32,
    pub team_id: u8,
    /// won/lost from team-aware resignation logic; None when undecided. source: aoe2rec.
    pub won: Option<bool>,
}

// --- position / role ---------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Flank,
    Pocket,
}

// --- analysis outputs --------------------------------------------------------
/// Everything computed for one player, ready to compare + render.
#[derive(Debug, Clone, Serialize)]
pub struct PlayerMetrics {
    pub info: PlayerInfo,
    pub feudal_ms: Option<u32>,
    pub castle_ms: Option<u32>,
    pub imperial_ms: Option<u32>,
    /// this civ's age-up research durations (feudal/castle/imperial seconds); civ-aware
    /// (baseline 130/160/190, Malay faster). Add to CLICK times for completion-based
    /// comparisons. Always present — sourced from the engine, so the UI never defaults it.
    pub age_res_s: (f64, f64, f64),
    pub vils_castle: u32,
    pub idle_dark_ms: u32,
    pub idle_feudal_ms: u32,
    pub first_military_ms: Option<u32>,
    pub eco_techs: Vec<(u16, u32)>, // (tech_id, first-research ms)
    pub apm: f64,
    pub elo_1v1: Option<i32>,
    pub elo_team: Option<i32>,
    pub role: Option<Role>,
    /// peak floating-resource window (estimate): rate res/min, total banked, and (start,end) ms.
    pub float_peak_rate: f64,
    pub float_banked: f64,
    pub float_window: Option<(u32, u32)>,
    /// distance from this player's base to the nearest enemy base (tiles), if computable.
    pub nearest_enemy_dist: Option<f32>,
    /// time the player's units first reached an enemy base region (scouting), if any.
    pub find_enemy_ms: Option<u32>,
    /// rule-based opening tag ("Scouts into Archers", "Fast Castle", …); None = nothing to say.
    pub opening: Option<String>,
    /// cumulative eco units QUEUED per minute (index = minute; same DeQueue basis as vils@Castle).
    pub vils_series: Vec<u32>,
    /// cumulative military units QUEUED per minute.
    pub mil_series: Vec<u32>,
    /// effective actions per minute (raw command count, same basis as `apm`).
    pub apm_series: Vec<u32>,
    /// eco/military splits of apm counting ONLY unambiguously attributable actions
    /// (eco/mil unit trains, cost-verified building sets, watched eco techs + age-ups) —
    /// lower bounds; moves/attacks stay unclassified.
    pub eco_apm: f64,
    pub mil_apm: f64,
    /// market transactions (command counts; resource/amount not decoded).
    pub market_buys: u32,
    pub market_sells: u32,
    /// the RESOLVED benchmark slice this player was compared against (winners'
    /// vils; age medians), with its honesty labels — None when the map/civ has
    /// no data. The UI renders this as the "reference" column verbatim.
    pub reference: Option<RefSlice>,
}

/// The benchmark slice actually used for this player (already fallback-resolved).
#[derive(Debug, Clone, Serialize)]
pub struct RefSlice {
    pub feudal_s: Option<f64>,
    pub castle_s: Option<f64>,
    pub imperial_s: Option<f64>,
    /// median villagers trained by Castle among WINNERS of the slice
    pub vils_castle: Option<f64>,
    /// elo bucket the player resolved to (e.g. "1400-1649")
    pub bucket: String,
    /// how precise the match was: "exact" | "map_mode" | "map_all"
    pub kind: String,
}

/// The four comparison bases: your elo-bucket median, the 2500+ "pro" median, the opponent
/// in this replay, and absolute-waste thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    YourElo,
    Pro,
    Opponent,
    Absolute,
}

/// Ordered so `High` sorts last by default; "top fixes" sorts by `Reverse(severity)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Med,
    High,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub player_number: i32,
    /// Stable snake_case code, one per emit site, so the browser can localize the
    /// metric + note (the English strings stay for the CLI renderer). Additive to
    /// the JSON contract — no SCHEMA_VERSION bump.
    pub code: &'static str,
    pub metric: String,
    pub your: String,
    pub reference: String,
    pub basis: Basis,
    pub severity: Severity,
    pub note: String,
    /// Template values for the localized note (empty for static findings).
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub vars: std::collections::BTreeMap<&'static str, String>,
}

// --- serialized report (the one-way-door JSON contract; see the Phase C spec) --
/// Bump on any breaking shape change so a consumer can detect instead of misparse.
pub const SCHEMA_VERSION: u32 = 1;

/// Honesty footer, shared verbatim by terminal + JSON. source: analyzer design spec.
pub const MACRO_CAVEAT: &str =
    "macro coach only — no fights, micro, map control, or exact resources.";

/// How to pick the "you" player. Name = --you (case-insensitive); ProfileId = --latest.
#[derive(Debug, Clone)]
pub enum YouSel { Auto, Name(String), ProfileId(i64) }

#[derive(Debug, Clone, Serialize)]
pub struct ReportMeta {
    pub map_id: u32,
    pub map_name: String,
    pub family: Family,
    pub mode: String, // "1v1" | "team"
    pub duration_ms: u32,
    pub you: i32, // player_number
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub meta: ReportMeta,
    pub players: Vec<PlayerMetrics>,
    pub findings: Vec<Finding>, // FULL list, severity-sorted; terminal's take(8) is a render choice
    pub caveats: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_serializes_versioned_snake_case() {
        let r = Report {
            schema_version: SCHEMA_VERSION,
            meta: ReportMeta {
                map_id: 9, map_name: "Arabia".into(), family: Family::Open,
                mode: "1v1".into(), duration_ms: 2_400_000, you: 1,
            },
            players: vec![],
            findings: vec![Finding {
                player_number: 1, code: "feudal_slow", metric: "Feudal up-time".into(), your: "12:10".into(),
                reference: "11:20".into(), basis: Basis::YourElo, severity: Severity::High,
                note: "n".into(), vars: std::collections::BTreeMap::new(),
            }],
            caveats: vec![MACRO_CAVEAT.to_string()],
        };
        let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(v["schema_version"], 1);
        assert_eq!(v["meta"]["family"], "open");
        assert_eq!(v["meta"]["mode"], "1v1");
        assert_eq!(v["findings"][0]["basis"], "your_elo");
        assert_eq!(v["findings"][0]["severity"], "high");
    }
}
