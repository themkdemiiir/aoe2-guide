//! Shared data types for the `analyze` subcommand.
//!
//! Split into: CLI inputs, the normalized event stream (`Ev`/`EvKind`), per-game and
//! per-player facts from the parser (`GameMeta`/`PlayerInfo`), and the analysis
//! outputs (`PlayerMetrics`, `Finding`). Pure metric functions live in `metrics.rs`;
//! nothing here touches `aoe2rec`.

use std::path::PathBuf;

// --- map families (from data/maps.tsv) ---------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Open,
    Closed,
    Hybrid,
    Water,
    Nomad,
    Special,
    Other,
}

// --- CLI ---------------------------------------------------------------------
#[derive(Debug, Clone)]
pub enum Input {
    File(PathBuf),
    MatchId(i64),
}

#[derive(Debug, Clone)]
pub struct AnalyzeArgs {
    pub input: Input,
    pub you: Option<String>,
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

#[derive(Debug, Clone)]
pub struct PlayerInfo {
    pub player_number: i32,
    pub civ_id: u32,
    pub name: String,
    pub color_id: i32,
    pub team_id: u8,
    /// won/lost from team-aware resignation logic; None when undecided. source: aoe2rec.
    pub won: Option<bool>,
}

// --- position / role ---------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Flank,
    Pocket,
}

// --- analysis outputs --------------------------------------------------------
/// Everything computed for one player, ready to compare + render.
#[derive(Debug, Clone)]
pub struct PlayerMetrics {
    pub info: PlayerInfo,
    pub feudal_ms: Option<u32>,
    pub castle_ms: Option<u32>,
    pub imperial_ms: Option<u32>,
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
}

/// The four comparison bases: your elo-bucket median, the 2500+ "pro" median, the opponent
/// in this replay, and absolute-waste thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    YourElo,
    Pro,
    Opponent,
    Absolute,
}

/// Ordered so `High` sorts last by default; "top fixes" sorts by `Reverse(severity)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Med,
    High,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub player_number: i32,
    pub metric: String,
    pub your: String,
    pub reference: String,
    pub basis: Basis,
    pub severity: Severity,
    pub note: String,
}
