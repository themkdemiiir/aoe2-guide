//! The typed input contract for [`crate::ingest_batch`] — one Rust struct per destination table,
//! mirroring its columns exactly (types + nullability). Producers (the replay parser, aoestats
//! ETL — later tasks) build a [`ReplayBatch`] and hand it to `ingest_batch`; this crate never
//! fabricates a value for a field the schema requires.

use chrono::{DateTime, Utc};
use pipeline_core::{Age, GameCivId, GameUnitId, MatchId, ProfileId};
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
    pub match_id: MatchId,
    pub source: MatchSource,
    pub ladder: Ladder,
    /// NOT NULL — FK to `maps(map_id)`. Plain `i32`: no `MapId` newtype exists yet.
    // TODO(MapId): newtype this once `core::ids` grows a `MapId`.
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
    pub match_id: MatchId,
    pub profile_id: ProfileId,
    /// NOT NULL — FK to `civs(civ_id)`.
    pub civ_id: GameCivId,
    pub elo: Option<i32>,
    pub won: Option<bool>,
    pub opening: Option<String>,
    /// `real` (float4) in Postgres — `f32`, not `f64`, so the Rust type matches the column
    /// exactly instead of silently truncating on write. See [`crate::ingest::copy_match_players`].
    pub feudal_t: Option<f32>,
    pub castle_t: Option<f32>,
    pub imperial_t: Option<f32>,
}

/// One row for `replay_events` — the highest-volume table (~100M rows), the COPY-critical path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewReplayEvent {
    pub match_id: MatchId,
    pub profile_id: Option<ProfileId>,
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
    pub match_id: MatchId,
    pub profile_id: ProfileId,
    /// NOT NULL — FK to `civs(civ_id)`.
    pub civ_id: GameCivId,
    pub won: Option<bool>,
    pub age: Age,
    pub uptime_ms: i32,
    pub villagers: Option<i32>,
    pub military: Option<i32>,
    pub n_buildings: Option<i32>,
    pub n_research: Option<i32>,
}

/// One row for `match_player_units` — one (match, player, DISTINCT unit_id) `trained` total.
/// **Replay-source ONLY** (aoestats' archive gives no per-unit breakdown — see the migration's
/// doc, `m20260706_000012_create_match_player_units.rs`). `trained` is Σ `amount` over that
/// player's `train` commands for `unit_id` — units QUEUED, never surviving army (the replay
/// format has no deaths/losses); see `replay::derive`'s module doc for the full metric rationale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewMatchPlayerUnit {
    pub match_id: MatchId,
    pub profile_id: ProfileId,
    pub unit_id: GameUnitId,
    pub trained: i32,
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
    #[serde(default)]
    pub player_units: Vec<NewMatchPlayerUnit>,
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
    /// Rows inserted into `match_player_units`, gated on `matches_inserted`.
    pub units: u64,
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    /// The id newtypes (`MatchId`/`ProfileId`/`GameCivId`) are `#[serde(transparent)]` and
    /// [`Age`] is a lowercase bare string — adopting them in the DTOs (5b) must not change the
    /// JSON shape a `ReplayBatch` producer/consumer (the CLI, and later `replay`) depends on.
    #[test]
    fn wire_format_is_unchanged_by_the_newtypes() {
        let played_at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let batch = ReplayBatch {
            matches: vec![NewMatch {
                match_id: MatchId(1001),
                source: MatchSource::Replay,
                ladder: Ladder::OneVOne,
                map_id: 1,
                build: Some(101),
                patch: Some("1.0".to_owned()),
                played_at,
                duration_ms: Some(1_800_000),
                n_players: 2,
            }],
            players: vec![NewMatchPlayer {
                match_id: MatchId(1001),
                profile_id: ProfileId(5001),
                civ_id: GameCivId(1),
                elo: Some(1400),
                won: Some(true),
                opening: None,
                feudal_t: Some(320.5),
                castle_t: None,
                imperial_t: None,
            }],
            events: vec![],
            ages: vec![NewReplayAge {
                match_id: MatchId(1001),
                profile_id: ProfileId(5001),
                civ_id: GameCivId(1),
                won: Some(true),
                age: Age::Dark,
                uptime_ms: 100,
                villagers: None,
                military: None,
                n_buildings: None,
                n_research: None,
            }],
            player_units: vec![NewMatchPlayerUnit {
                match_id: MatchId(1001),
                profile_id: ProfileId(5001),
                unit_id: GameUnitId(83),
                trained: 5,
            }],
        };

        let value = serde_json::to_value(&batch).expect("ReplayBatch must serialize");

        // Newtypes serialize as bare integers, not `{"0": 1001}`.
        assert_eq!(value["matches"][0]["match_id"], serde_json::json!(1001));
        assert_eq!(value["players"][0]["match_id"], serde_json::json!(1001));
        assert_eq!(value["players"][0]["profile_id"], serde_json::json!(5001));
        assert_eq!(value["players"][0]["civ_id"], serde_json::json!(1));
        assert_eq!(value["player_units"][0]["unit_id"], serde_json::json!(83));

        // Age is a lowercase bare string, not `{"Dark": null}`.
        assert_eq!(value["ages"][0]["age"], serde_json::json!("dark"));

        // And it round-trips back to an identical struct.
        let round_tripped: ReplayBatch =
            serde_json::from_value(value).expect("ReplayBatch must deserialize back");
        assert_eq!(round_tripped, batch);
    }
}
