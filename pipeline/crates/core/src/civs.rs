//! Two id-space -> civ-slug lookups, kept strictly separate:
//!
//! - [`GameCivMap`] — the GAME/replay `civ_id` space (`src/data/civ-id-map.json`), used by the
//!   analyzer + replay data.
//! - [`RelicCivMap`] — the Relic API's OWN `civilization_id` space (`src/data/relic-civ-id-map.json`),
//!   used by every crawl consumer.
//!
//! These two id spaces are NOT interchangeable — applying the wrong table silently shuffles every
//! civ label (a real production bug, see `scripts/data-pipeline/lib/relic-map.mjs`). Both maps
//! read the committed JSON files (SINGLE SOURCE OF TRUTH — never modified here) and fail loud on
//! an unknown id at lookup, mirroring `relicCivSlug`'s throw-on-unknown: a new DLC/patch id must
//! break the pipeline, not silently mislabel a civ.

use std::collections::HashMap;
use std::fmt;

use serde::Deserialize;

/// A lookup for an id absent from the committed map. A new DLC/patch likely shifted (or extended)
/// the id space — re-derive the JSON map before trusting anything built on top of this lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownCivId(pub u32);

impl fmt::Display for UnknownCivId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown civ id {}", self.0)
    }
}

impl std::error::Error for UnknownCivId {}

/// Game/replay `civ_id` -> slug (`src/data/civ-id-map.json`). Id 0 is "random", not a real civ.
#[derive(Debug, Clone)]
pub struct GameCivMap(HashMap<u32, String>);

impl GameCivMap {
    /// Resolves a game-space `civ_id` to its slug. `Err(UnknownCivId)` for anything not in the
    /// committed map — never guesses.
    pub fn slug(&self, id: u32) -> Result<&str, UnknownCivId> {
        self.0.get(&id).map(String::as_str).ok_or(UnknownCivId(id))
    }
}

/// Parses `civ-id-map.json`-shaped text (a flat `{ "id": "slug" }` object).
pub fn parse_game_civs(json: &str) -> serde_json::Result<GameCivMap> {
    let raw: HashMap<String, String> = serde_json::from_str(json)?;
    Ok(GameCivMap(str_keys_to_u32(raw)))
}

/// Loads the real, committed `src/data/civ-id-map.json`, baked into the binary at compile time.
pub fn load_game_civs() -> GameCivMap {
    parse_game_civs(include_str!("../../../../src/data/civ-id-map.json")).expect("civ-id-map.json")
}

/// Relic API `civilization_id` -> slug (`src/data/relic-civ-id-map.json`). Only the `map` key is
/// consumed; `provenance` is documentation for humans, not read here.
#[derive(Debug, Deserialize)]
struct RelicDoc {
    map: HashMap<String, String>,
}

/// The Relic API's own civ id space. NEVER look up a game/replay `civ_id` here (and vice versa
/// for [`GameCivMap`]) — see the module doc comment.
#[derive(Debug, Clone)]
pub struct RelicCivMap(HashMap<u32, String>);

impl RelicCivMap {
    /// Resolves a Relic-space `civilization_id` to its slug. `Err(UnknownCivId)` for anything not
    /// in the committed map — never guesses.
    pub fn slug(&self, id: u32) -> Result<&str, UnknownCivId> {
        self.0.get(&id).map(String::as_str).ok_or(UnknownCivId(id))
    }
}

/// Parses `relic-civ-id-map.json`-shaped text (`{ "provenance": {...}, "map": { "id": "slug" } }`).
pub fn parse_relic_civs(json: &str) -> serde_json::Result<RelicCivMap> {
    let doc: RelicDoc = serde_json::from_str(json)?;
    Ok(RelicCivMap(str_keys_to_u32(doc.map)))
}

/// Loads the real, committed `src/data/relic-civ-id-map.json`, baked into the binary at compile time.
pub fn load_relic_civs() -> RelicCivMap {
    parse_relic_civs(include_str!("../../../../src/data/relic-civ-id-map.json"))
        .expect("relic-civ-id-map.json")
}

/// Both source files key their JSON object by decimal-string id; convert once, sharing the same
/// rule for both id spaces (a malformed key is dropped rather than panicking the parse, matching
/// neither file ever having one in practice — see the tests below).
fn str_keys_to_u32(raw: HashMap<String, String>) -> HashMap<u32, String> {
    raw.into_iter()
        .filter_map(|(k, v)| k.parse::<u32>().ok().map(|id| (id, v)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_civ_id_2_is_franks() {
        let civs = load_game_civs();
        assert_eq!(civs.slug(2).unwrap(), "franks");
    }

    #[test]
    fn relic_civ_id_5_is_britons() {
        let civs = load_relic_civs();
        assert_eq!(civs.slug(5).unwrap(), "britons");
    }

    #[test]
    fn unknown_game_civ_id_fails_loud() {
        let civs = load_game_civs();
        assert_eq!(civs.slug(9999), Err(UnknownCivId(9999)));
    }

    #[test]
    fn unknown_relic_civ_id_fails_loud() {
        let civs = load_relic_civs();
        assert!(civs.slug(9999).is_err());
    }
}
