//! Game `tech_id` -> `{ name, internal_name }` lookup, parsed from the committed
//! `src/data/tech-names.json` (SINGLE SOURCE OF TRUTH — read here, never hand-edited; regenerate
//! via `pnpm build:unit-tech-names`, which extracts from aoe2techtree — see that script's module
//! doc for the exact resolution — never hand-typed).
//!
//! Same id space as [`crate::ids::TechId`] (a replay `research` command's tech id — see that
//! type's doc; also `replay::config::WATCHED_TECHS`'s ids). Backs the `techs` dimension table
//! (`match_player_techs.tech_id`'s FK target). Unlike [`crate::units`], `derive::player_techs`
//! only ever emits the 10 `WATCHED_TECHS` ids, all present here — a FK violation on this table
//! would mean `WATCHED_TECHS`/this table drifted apart, not a new-DLC id (see
//! `m20260706_000016_create_techs.rs`'s forward-compat note).

use std::collections::HashMap;
use std::fmt;

use serde::Deserialize;

use crate::ids::TechId;

/// A lookup for a tech id absent from the committed table. A new DLC/patch likely added it —
/// re-run `pnpm build:unit-tech-names` (which re-derives from aoe2techtree) before trusting
/// anything built on top of this lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownTechId(pub i32);

impl fmt::Display for UnknownTechId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown tech id {}", self.0)
    }
}

impl std::error::Error for UnknownTechId {}

/// One `tech-names.json` entry. `internal_name` (the aoe2techtree `internal_name`, e.g.
/// `"Loom"`) is documentation/debugging only — never used for lookups — and is `None` for the
/// rare id the source JSON couldn't attach one to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechInfo {
    pub name: String,
    pub internal_name: Option<String>,
}

/// Game `tech_id` -> [`TechInfo`]. See the module doc for the id space and source.
#[derive(Debug, Clone, Default)]
pub struct TechTable(HashMap<i32, TechInfo>);

impl TechTable {
    /// Resolves a [`TechId`] to its English display name. `Err(UnknownTechId)` for anything not
    /// in the committed table — never guesses.
    pub fn name(&self, id: TechId) -> Result<&str, UnknownTechId> {
        self.0
            .get(&id.0)
            .map(|info| info.name.as_str())
            .ok_or(UnknownTechId(id.0))
    }

    /// Iterates every `(tech_id, TechInfo)` pair — e.g. for the dims loader to load the whole
    /// reference table rather than look up one id at a time.
    pub fn entries(&self) -> impl Iterator<Item = (i32, &TechInfo)> {
        self.0.iter().map(|(&id, info)| (id, info))
    }
}

/// `tech-names.json`'s shape: `{ "provenance": {...}, "map": { "id": "name" }, "internalNames":
/// { "id": "internal_name" } }`. `provenance` is documentation for humans, not read here.
#[derive(Debug, Deserialize)]
struct TechDoc {
    map: HashMap<String, String>,
    #[serde(rename = "internalNames", default)]
    internal_names: HashMap<String, String>,
}

/// Parses `tech-names.json`-shaped text.
pub fn parse_techs(json: &str) -> serde_json::Result<TechTable> {
    let TechDoc {
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
            TechInfo {
                name,
                internal_name,
            },
        );
    }
    Ok(TechTable(by_id))
}

/// Loads the real, committed `src/data/tech-names.json`, baked into the binary at compile time.
/// `Err` (never a panic — playbook rule 8: no `unwrap`/`expect`/`panic` in a `pub` lib fn) if that
/// committed file is somehow malformed; the caller (`dims::load_dims`) `.context()`s it.
pub fn load_techs() -> serde_json::Result<TechTable> {
    parse_techs(include_str!("../../../../src/data/tech-names.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tech_id_22_is_loom() {
        let techs = load_techs().expect("tech-names.json must parse");
        assert_eq!(techs.name(TechId(22)).unwrap(), "Loom");
    }

    #[test]
    fn watched_techs_all_resolve_to_a_name() {
        // Cross-checks `replay::config::WATCHED_TECHS`'s exact ids: Loom, Wheelbarrow, Hand Cart,
        // Double-Bit Axe, Bow Saw, Horse Collar, Heavy Plow, Gold Mining, Gold Shaft Mining, Stone
        // Mining. All must resolve here — a FK violation on `match_player_techs.tech_id` should
        // only ever mean `WATCHED_TECHS` and this table drifted apart, never a missing new-DLC id
        // (see the module doc and `m20260706_000016_create_techs.rs`).
        let techs = load_techs().expect("tech-names.json must parse");
        for id in [22, 213, 249, 202, 203, 14, 13, 55, 182, 278] {
            assert!(
                techs.name(TechId(id)).is_ok(),
                "tech id {id} must resolve to a name"
            );
        }
    }

    #[test]
    fn unknown_tech_id_fails_loud() {
        let techs = load_techs().expect("tech-names.json must parse");
        assert_eq!(techs.name(TechId(999_999)), Err(UnknownTechId(999_999)));
    }

    #[test]
    fn entries_covers_every_row_including_loom() {
        let techs = load_techs().expect("tech-names.json must parse");
        let by_id: HashMap<i32, &str> = techs
            .entries()
            .map(|(id, info)| (id, info.name.as_str()))
            .collect();
        assert_eq!(by_id.get(&22), Some(&"Loom"));
        assert!(by_id.len() > 150, "tech-names.json has ~192 entries");
    }

    #[test]
    fn internal_name_is_populated_for_a_known_id() {
        let techs = load_techs().expect("tech-names.json must parse");
        let (_, info) = techs
            .entries()
            .find(|(id, _)| *id == 22)
            .expect("id 22 present");
        assert_eq!(info.internal_name.as_deref(), Some("Loom"));
    }

    #[test]
    fn parses_a_minimal_fixture() {
        let json = r#"{
            "provenance": { "source": "x", "sha": "y", "note": "z" },
            "map": { "22": "Loom", "213": "Wheelbarrow" },
            "internalNames": { "22": "Loom" }
        }"#;
        let techs = parse_techs(json).expect("valid fixture must parse");
        assert_eq!(techs.name(TechId(22)).unwrap(), "Loom");
        assert_eq!(techs.name(TechId(213)).unwrap(), "Wheelbarrow");
        let (_, info) = techs
            .entries()
            .find(|(id, _)| *id == 213)
            .expect("id 213 present");
        assert_eq!(
            info.internal_name, None,
            "an id with no internalNames entry must stay None, never fabricated"
        );
    }
}
