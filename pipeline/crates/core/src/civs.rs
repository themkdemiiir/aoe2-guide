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

use crate::ids::{GameCivId, RelicCivId};

/// A lookup for an id absent from the committed map. A new DLC/patch likely shifted (or extended)
/// the id space — re-derive the JSON map before trusting anything built on top of this lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownCivId(pub i32);

impl fmt::Display for UnknownCivId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown civ id {}", self.0)
    }
}

impl std::error::Error for UnknownCivId {}

/// Game/replay `civ_id` -> slug (`src/data/civ-id-map.json`). Id 0 is "random", not a real civ.
#[derive(Debug, Clone)]
pub struct GameCivMap(HashMap<i32, String>);

impl GameCivMap {
    /// Resolves a game-space [`GameCivId`] to its slug. `Err(UnknownCivId)` for anything not in
    /// the committed map — never guesses.
    pub fn slug(&self, id: GameCivId) -> Result<&str, UnknownCivId> {
        self.0
            .get(&id.0)
            .map(String::as_str)
            .ok_or(UnknownCivId(id.0))
    }

    /// Iterates every `(civ_id, slug)` pair — e.g. for the dims loader to load the whole
    /// reference table rather than look up one id at a time.
    pub fn entries(&self) -> impl Iterator<Item = (i32, &str)> {
        self.0.iter().map(|(&id, slug)| (id, slug.as_str()))
    }
}

/// Parses `civ-id-map.json`-shaped text (a flat `{ "id": "slug" }` object).
pub fn parse_game_civs(json: &str) -> serde_json::Result<GameCivMap> {
    let raw: HashMap<String, String> = serde_json::from_str(json)?;
    Ok(GameCivMap(str_keys_to_i32(raw)))
}

/// Loads the real, committed `src/data/civ-id-map.json`, baked into the binary at compile time.
pub fn load_game_civs() -> GameCivMap {
    parse_game_civs(include_str!("../../../../src/data/civ-id-map.json")).expect("civ-id-map.json")
}

/// Relic API `civilization_id` -> slug (`src/data/relic-civ-id-map.json`). `map` is the id->slug
/// lookup; `provenance.validFrom` is read too (it feeds `civs_relic.valid_from` — see
/// [`RelicCivMap::valid_from`]). The rest of `provenance` is documentation for humans, not read
/// here.
#[derive(Debug, Deserialize)]
struct RelicDoc {
    provenance: RelicProvenance,
    map: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct RelicProvenance {
    #[serde(rename = "validFrom")]
    valid_from: String,
}

/// The Relic API's own civ id space. NEVER look up a game/replay `civ_id` here (and vice versa
/// for [`GameCivMap`]) — see the module doc comment.
#[derive(Debug, Clone)]
pub struct RelicCivMap {
    by_id: HashMap<i32, String>,
    /// `provenance.validFrom` verbatim (ISO `YYYY-MM-DD`, e.g. `"2025-09-01"`) — the era this
    /// whole id->slug mapping is valid from (see the module doc: the Relic id space re-shuffles
    /// at some patch/DLC boundaries). Kept as a raw string, not a date type, so this module — like
    /// [`GameCivMap`] — stays free of a `chrono` dependency; a caller that needs a real date
    /// parses this at the point of use.
    valid_from: String,
}

impl RelicCivMap {
    /// Resolves a Relic-space [`RelicCivId`] to its slug. `Err(UnknownCivId)` for anything not
    /// in the committed map — never guesses.
    pub fn slug(&self, id: RelicCivId) -> Result<&str, UnknownCivId> {
        self.by_id
            .get(&id.0)
            .map(String::as_str)
            .ok_or(UnknownCivId(id.0))
    }

    /// Iterates every `(civilization_id, slug)` pair — e.g. for the dims loader to load the whole
    /// reference table rather than look up one id at a time.
    pub fn entries(&self) -> impl Iterator<Item = (i32, &str)> {
        self.by_id.iter().map(|(&id, slug)| (id, slug.as_str()))
    }

    /// The single era this whole mapping is valid from (`provenance.validFrom`, ISO
    /// `YYYY-MM-DD`) — see the field doc.
    pub fn valid_from(&self) -> &str {
        &self.valid_from
    }
}

/// Parses `relic-civ-id-map.json`-shaped text (`{ "provenance": {...}, "map": { "id": "slug" } }`).
pub fn parse_relic_civs(json: &str) -> serde_json::Result<RelicCivMap> {
    let doc: RelicDoc = serde_json::from_str(json)?;
    Ok(RelicCivMap {
        by_id: str_keys_to_i32(doc.map),
        valid_from: doc.provenance.valid_from,
    })
}

/// Loads the real, committed `src/data/relic-civ-id-map.json`, baked into the binary at compile time.
pub fn load_relic_civs() -> RelicCivMap {
    parse_relic_civs(include_str!("../../../../src/data/relic-civ-id-map.json"))
        .expect("relic-civ-id-map.json")
}

/// Both source files key their JSON object by decimal-string id; convert once, sharing the same
/// rule for both id spaces (a malformed key is dropped rather than panicking the parse, matching
/// neither file ever having one in practice — see the tests below).
fn str_keys_to_i32(raw: HashMap<String, String>) -> HashMap<i32, String> {
    raw.into_iter()
        .filter_map(|(k, v)| k.parse::<i32>().ok().map(|id| (id, v)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_civ_id_2_is_franks() {
        let civs = load_game_civs();
        assert_eq!(civs.slug(GameCivId(2)).unwrap(), "franks");
    }

    #[test]
    fn relic_civ_id_5_is_britons() {
        let civs = load_relic_civs();
        assert_eq!(civs.slug(RelicCivId(5)).unwrap(), "britons");
    }

    #[test]
    fn unknown_game_civ_id_fails_loud() {
        let civs = load_game_civs();
        assert_eq!(civs.slug(GameCivId(9999)), Err(UnknownCivId(9999)));
    }

    #[test]
    fn unknown_relic_civ_id_fails_loud() {
        let civs = load_relic_civs();
        assert!(civs.slug(RelicCivId(9999)).is_err());
    }

    #[test]
    fn game_civ_entries_cover_every_row_including_franks() {
        let civs = load_game_civs();
        let by_id: HashMap<i32, &str> = civs.entries().collect();
        assert_eq!(by_id.get(&2), Some(&"franks"));
        assert!(by_id.len() > 40, "civ-id-map.json has ~50 entries");
    }

    #[test]
    fn relic_civ_entries_and_valid_from_cover_the_real_file() {
        let civs = load_relic_civs();
        let by_id: HashMap<i32, &str> = civs.entries().collect();
        assert_eq!(by_id.get(&5), Some(&"britons"));
        assert!(by_id.len() > 40, "relic-civ-id-map.json has ~50 entries");
        assert_eq!(civs.valid_from(), "2025-09-01");
    }
}
