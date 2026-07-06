//! Game `unit_id` -> `{ name, internal_name }` lookup, parsed from the committed
//! `src/data/unit-names.json` (SINGLE SOURCE OF TRUTH — read here, never hand-edited; regenerate
//! via `pnpm build:unit-tech-names`, which extracts from aoe2techtree — see that script's module
//! doc for the exact resolution — never hand-typed).
//!
//! Same id space as [`crate::ids::GameUnitId`] (a replay `train` command's unit id — see that
//! type's doc). Backs the `units` dimension table (`match_player_units.unit_id`'s FK target):
//! a replay training a unit id absent from this table is a fail-loud FK violation at ingest time,
//! not a silently accepted row — see `m20260706_000015_create_units.rs`'s forward-compat note.

use std::collections::HashMap;
use std::fmt;

use serde::Deserialize;

use crate::ids::GameUnitId;

/// A lookup for a unit id absent from the committed table. A new DLC/patch likely added it —
/// re-run `pnpm build:unit-tech-names` (which re-derives from aoe2techtree) before trusting
/// anything built on top of this lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownUnitId(pub i32);

impl fmt::Display for UnknownUnitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown unit id {}", self.0)
    }
}

impl std::error::Error for UnknownUnitId {}

/// One `unit-names.json` entry. `internal_name` (the DAT `internal_name`, e.g. `"SCOUT"`) is
/// documentation/debugging only — never used for lookups — and is `None` for the rare id the
/// source JSON couldn't attach one to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitInfo {
    pub name: String,
    pub internal_name: Option<String>,
}

/// Game `unit_id` -> [`UnitInfo`]. See the module doc for the id space and source.
#[derive(Debug, Clone, Default)]
pub struct UnitTable(HashMap<i32, UnitInfo>);

impl UnitTable {
    /// Resolves a [`GameUnitId`] to its English display name. `Err(UnknownUnitId)` for anything
    /// not in the committed table — never guesses.
    pub fn name(&self, id: GameUnitId) -> Result<&str, UnknownUnitId> {
        self.0
            .get(&id.0)
            .map(|info| info.name.as_str())
            .ok_or(UnknownUnitId(id.0))
    }

    /// Iterates every `(unit_id, UnitInfo)` pair — e.g. for the dims loader to load the whole
    /// reference table rather than look up one id at a time.
    pub fn entries(&self) -> impl Iterator<Item = (i32, &UnitInfo)> {
        self.0.iter().map(|(&id, info)| (id, info))
    }
}

/// `unit-names.json`'s shape: `{ "provenance": {...}, "map": { "id": "name" }, "internalNames":
/// { "id": "internal_name" } }`. `provenance` is documentation for humans, not read here.
#[derive(Debug, Deserialize)]
struct UnitDoc {
    map: HashMap<String, String>,
    #[serde(rename = "internalNames", default)]
    internal_names: HashMap<String, String>,
}

/// Parses `unit-names.json`-shaped text.
pub fn parse_units(json: &str) -> serde_json::Result<UnitTable> {
    let UnitDoc {
        map,
        internal_names,
    } = serde_json::from_str(json)?;
    let mut by_id = HashMap::with_capacity(map.len());
    // A malformed key is dropped rather than panicking the parse, matching `civs.rs`'s
    // `str_keys_to_i32` rule (neither the civ nor unit/tech source files ever have one in
    // practice — see the tests below).
    for (id_str, name) in map {
        let Ok(id) = id_str.parse::<i32>() else {
            continue;
        };
        let internal_name = internal_names.get(&id_str).cloned();
        by_id.insert(
            id,
            UnitInfo {
                name,
                internal_name,
            },
        );
    }
    Ok(UnitTable(by_id))
}

/// Loads the real, committed `src/data/unit-names.json`, baked into the binary at compile time.
/// `Err` (never a panic — playbook rule 8: no `unwrap`/`expect`/`panic` in a `pub` lib fn) if that
/// committed file is somehow malformed; the caller (`dims::load_dims`) `.context()`s it.
pub fn load_units() -> serde_json::Result<UnitTable> {
    parse_units(include_str!("../../../../src/data/unit-names.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_id_448_is_scout_cavalry() {
        let units = load_units().expect("unit-names.json must parse");
        assert_eq!(units.name(GameUnitId(448)).unwrap(), "Scout Cavalry");
    }

    #[test]
    fn unit_id_83_is_villager() {
        // `replay::config::VILLAGER_ID` — cross-checks the same id this crate's sibling `replay`
        // crate hardcodes.
        let units = load_units().expect("unit-names.json must parse");
        assert_eq!(units.name(GameUnitId(83)).unwrap(), "Villager");
    }

    #[test]
    fn unknown_unit_id_fails_loud() {
        let units = load_units().expect("unit-names.json must parse");
        assert_eq!(units.name(GameUnitId(999_999)), Err(UnknownUnitId(999_999)));
    }

    #[test]
    fn entries_covers_every_row_including_scout_cavalry() {
        let units = load_units().expect("unit-names.json must parse");
        let by_id: HashMap<i32, &str> = units
            .entries()
            .map(|(id, info)| (id, info.name.as_str()))
            .collect();
        assert_eq!(by_id.get(&448), Some(&"Scout Cavalry"));
        assert!(by_id.len() > 200, "unit-names.json has ~238 entries");
    }

    #[test]
    fn internal_name_is_populated_for_a_known_id() {
        let units = load_units().expect("unit-names.json must parse");
        let (_, info) = units
            .entries()
            .find(|(id, _)| *id == 448)
            .expect("id 448 present");
        assert_eq!(info.internal_name.as_deref(), Some("SCOUT"));
    }

    #[test]
    fn parses_a_minimal_fixture() {
        let json = r#"{
            "provenance": { "source": "x", "sha": "y", "note": "z" },
            "map": { "448": "Scout Cavalry", "83": "Villager" },
            "internalNames": { "448": "SCOUT" }
        }"#;
        let units = parse_units(json).expect("valid fixture must parse");
        assert_eq!(units.name(GameUnitId(448)).unwrap(), "Scout Cavalry");
        assert_eq!(units.name(GameUnitId(83)).unwrap(), "Villager");
        let (_, info) = units
            .entries()
            .find(|(id, _)| *id == 83)
            .expect("id 83 present");
        assert_eq!(
            info.internal_name, None,
            "an id with no internalNames entry must stay None, never fabricated"
        );
    }
}
