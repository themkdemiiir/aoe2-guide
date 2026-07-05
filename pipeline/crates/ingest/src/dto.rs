//! The typed input contract for [`crate::ingest_batch`] — one Rust struct per destination table,
//! mirroring its columns exactly (types + nullability). Producers (the replay parser, aoestats
//! ETL — later tasks) build a [`ReplayBatch`] and hand it to `ingest_batch`; this crate never
//! fabricates a value for a field the schema requires.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// `matches.source` — mirrors the PG enum `source_kind` (`'replay' | 'aoestats'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchSource {
    Replay,
    Aoestats,
}

impl MatchSource {
    /// The exact `source_kind` label this variant casts to (`text::source_kind`) at the INSERT
    /// step — see [`crate::ingest_batch`]'s staging-to-real INSERT SQL.
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            MatchSource::Replay => "replay",
            MatchSource::Aoestats => "aoestats",
        }
    }
}

/// `matches.ladder` — mirrors the PG enum `ladder_kind` (`'1v1' | 'team'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ladder {
    #[serde(rename = "1v1")]
    OneVOne,
    Team,
}

impl Ladder {
    /// The exact `ladder_kind` label this variant casts to (`text::ladder_kind`) at the INSERT
    /// step.
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            Ladder::OneVOne => "1v1",
            Ladder::Team => "team",
        }
    }
}

/// One row for `matches`. The replay parser does NOT emit `source`/`ladder`/`played_at` — those
/// are discovery-seed fields the caller (Task 4) must supply; they are non-optional here so a
/// caller that lacks a real value fails to compile rather than silently defaulting one in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewMatch {
    pub match_id: i64,
    pub source: MatchSource,
    pub ladder: Ladder,
    /// NOT NULL — FK to `maps(map_id)`.
    pub map_id: i32,
    pub build: Option<i32>,
    pub patch: Option<String>,
    /// NOT NULL, `timestamptz`.
    pub played_at: DateTime<Utc>,
    pub duration_ms: Option<i32>,
    /// NOT NULL.
    pub n_players: i16,
}

/// One row for `match_players`. Deliberately excludes `elo_bucket` — it is a
/// `GENERATED ALWAYS ... STORED` column; the schema itself rejects an explicit write to it.
/// Also excludes the parser's `name`/`team`/`color`/`player_number` — not stored by this schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewMatchPlayer {
    pub match_id: i64,
    pub profile_id: i64,
    /// NOT NULL — FK to `civs(civ_id)`.
    pub civ_id: i32,
    pub elo: Option<i32>,
    pub won: Option<bool>,
    pub opening: Option<String>,
    pub feudal_t: Option<f64>,
    pub castle_t: Option<f64>,
    pub imperial_t: Option<f64>,
}

/// One row for `replay_events` — the highest-volume table (~100M rows), the COPY-critical path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewReplayEvent {
    pub match_id: i64,
    pub profile_id: Option<i64>,
    pub player_number: i16,
    pub t_ms: i32,
    pub kind: String,
    pub target_id: Option<i64>,
    pub amount: Option<i64>,
    pub detail: Option<String>,
}

/// One row for `replay_ages` — one (match, player, age) age-up summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewReplayAge {
    pub match_id: i64,
    pub profile_id: i64,
    /// NOT NULL — FK to `civs(civ_id)`.
    pub civ_id: i32,
    pub won: Option<bool>,
    pub age: String,
    pub uptime_ms: i32,
    pub villagers: Option<i32>,
    pub military: Option<i32>,
    pub n_buildings: Option<i32>,
    pub n_research: Option<i32>,
}

/// One ingest unit: everything [`crate::ingest_batch`] loads in a single transaction.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReplayBatch {
    #[serde(default)]
    pub matches: Vec<NewMatch>,
    #[serde(default)]
    pub players: Vec<NewMatchPlayer>,
    #[serde(default)]
    pub events: Vec<NewReplayEvent>,
    #[serde(default)]
    pub ages: Vec<NewReplayAge>,
}

/// The outcome of one [`crate::ingest_batch`] call.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestStats {
    /// Rows newly inserted into `matches` (genuinely-new `match_id`s).
    pub matches_inserted: u64,
    /// Rows in the batch whose `match_id` already existed (re-ingest, or an intra-batch dupe).
    pub matches_skipped: u64,
    /// Rows inserted into `match_players`, gated on `matches_inserted`.
    pub players: u64,
    /// Rows inserted into `replay_events`, gated on `matches_inserted`.
    pub events: u64,
    /// Rows inserted into `replay_ages`, gated on `matches_inserted`.
    pub ages: u64,
}
