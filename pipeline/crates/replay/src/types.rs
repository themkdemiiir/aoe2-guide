//! [`ParsedReplay`] — the pure, gameplay-only output of [`crate::parse`].
//!
//! Deliberately excludes `source`/`ladder`/`played_at`: a replay's own bytes never carry those
//! (they come from whatever discovery seed downloaded the file — Task 4c), so this type only
//! models what the bytes themselves actually know. The ingest adapter (Task 4d) joins the two.

use pipeline_core::{Age, GameCivId, MatchId, ProfileId};
use serde::{Deserialize, Serialize};

/// Everything decoded from one `.aoe2record`. `match_id` is supplied by the caller (see
/// [`crate::parse`]'s doc) — it is never encoded in the replay bytes themselves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedReplay {
    pub match_id: MatchId,
    pub build: Option<i32>,
    /// `None` if the replay didn't carry a resolvable map id; ingest asserts `NOT NULL` later —
    /// this parser never fabricates one.
    pub map_id: Option<i32>,
    pub duration_ms: i32,
    pub n_players: i16,
    pub players: Vec<ReplayPlayer>,
    pub events: Vec<ReplayEvent>,
    pub ages: Vec<ReplayAge>,
}

/// One real player (gaia / empty civ-id-0 slots are dropped during parsing — see
/// `parse::players_map`). Keeps `name`/`team`/`color` even though the current `ingest` schema
/// doesn't store them: that's the adapter's choice to drop them, not this pure parser's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayPlayer {
    pub player_number: i16,
    pub profile_id: ProfileId,
    pub civ_id: GameCivId,
    pub name: String,
    pub team: i16,
    pub color: i32,
    /// `None` when the replay records no resignation at all (winner unknown from this replay
    /// alone) — never guessed.
    pub won: Option<bool>,
    /// Ranked ELO read from the replay's post-game leaderboard block (ladder 3 = 1v1 RM, 4 =
    /// team RM). `None` when the replay carries no post-game block.
    pub elo: Option<i32>,
}

/// One decoded action from the replay's operation stream — the full timeline, nothing dropped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayEvent {
    /// `None` when the acting `player_number` doesn't resolve to a real player (a gaia/empty
    /// slot, or an id the game settings never listed).
    pub profile_id: Option<ProfileId>,
    pub player_number: i16,
    pub t_ms: i32,
    pub kind: String,
    pub target_id: Option<i64>,
    pub amount: Option<i64>,
    pub detail: Option<String>,
}

/// One (player, age) age-up summary: cumulative composition up to the first tick that player
/// researched the given age.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayAge {
    pub profile_id: ProfileId,
    pub civ_id: GameCivId,
    pub won: Option<bool>,
    pub age: Age,
    pub uptime_ms: i32,
    pub villagers: i32,
    pub military: i32,
    pub n_buildings: i32,
    pub n_research: i32,
}
