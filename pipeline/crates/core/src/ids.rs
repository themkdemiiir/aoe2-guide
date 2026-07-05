//! Id-space newtypes that make cross-space transposition a compile error.
//!
//! [`GameCivId`] and [`RelicCivId`] wrap the two DISTINCT `civ_id`/`civilization_id` integer
//! spaces documented in [`crate::civs`] — applying one map's id to the other silently shuffles
//! every civ label (a real production bug, June 2026). Before these newtypes, both ids were bare
//! `u32`s the compiler could not tell apart; now `GameCivMap::slug` and `RelicCivMap::slug` only
//! accept their own type, so passing the wrong id space is a `cargo build` failure instead of a
//! silent mislabel.
//!
//! [`MatchId`] and [`ProfileId`] exist for the same reason at the row-identity boundary: both are
//! `i64`s in Postgres (`bigint`), and a future join/lookup mixing them up would be just as silent
//! without the newtype.
//!
//! All four wrap the DB-native **signed** integer (`i32`/`i64`, matching Postgres `integer`/
//! `bigint`) rather than `u32`/`u64`: they flow into `tokio-postgres` binary COPY, Postgres has no
//! unsigned integer types, and `tokio-postgres` has no `ToSql for u32`/`u64`. Civ/match/profile ids
//! are always small non-negative numbers in practice, so this is zero value change — just the
//! Rust type that makes the eventual COPY code compile.
//!
//! Deliberately **not** `#[postgres(transparent)]` (unlike the playbook's original sketch): that
//! would pull a `postgres-types` dependency into `core`, breaking the "functional core, no DB
//! deps" rule (see the crate-level doc). These are plain, pure, `#[serde(transparent)]` domain
//! types; downstream COPY code unwraps `.0` at the point of use — ingest already writes explicit
//! COPY params, so this is a one-token cost, not new ceremony.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The GAME/replay `civ_id` space (`src/data/civ-id-map.json`), used by the analyzer + replay
/// data. See the module doc for why this must never be confused with [`RelicCivId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GameCivId(pub i32);

impl fmt::Display for GameCivId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The Relic API's OWN `civilization_id` space (`src/data/relic-civ-id-map.json`), used by every
/// crawl consumer. See the module doc for why this must never be confused with [`GameCivId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RelicCivId(pub i32);

impl fmt::Display for RelicCivId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A `matches` row's primary key (Postgres `bigint`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MatchId(pub i64);

impl fmt::Display for MatchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A Relic `profile_id` (Postgres `bigint`) — the player identity both the Relic API and replays
/// key on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(pub i64);

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_prints_inner_value() {
        assert_eq!(GameCivId(2).to_string(), "2");
        assert_eq!(RelicCivId(5).to_string(), "5");
        assert_eq!(MatchId(123456789).to_string(), "123456789");
        assert_eq!(ProfileId(987654321).to_string(), "987654321");
    }

    #[test]
    fn serde_transparent_round_trips_as_bare_int() {
        // `#[serde(transparent)]` means these serialize as the bare integer, not `{"0": 2}` — the
        // shape downstream JSON (e.g. a `ReplayBatch`) already expects.
        assert_eq!(serde_json::to_string(&GameCivId(2)).unwrap(), "2");
        let id: GameCivId = serde_json::from_str("2").unwrap();
        assert_eq!(id, GameCivId(2));
    }
}
