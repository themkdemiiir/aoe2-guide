//! Per-`(use_type, node_id)` English display NAME, resolved from the raw per-civ tree-node
//! observations committed in `reference-data/aoe2techtree-tree-nodes.json` (base-game civs ONLY —
//! `data/trees/*.json`, deliberately NOT `chronicles/data/trees` — mirrors
//! `scripts/build-unit-tech-names.mjs`'s own exclusion: Return of Rome/Chronicles reskin some
//! shared ids with different flavor names, e.g. Militia getting a different name on a
//! Macedonians-style tree — a real, verified conflict, not a data error; see that script's module
//! doc §3). [`crate::unit_tech_names`] is the sole consumer.
//!
//! A `node_id` observed with two DIFFERING names (within the same `use_type`) is a fail-loud
//! [`RefdataError::ConflictingTreeNodeName`] — never silently picks one, mirroring
//! `build-unit-tech-names.mjs`'s own `throw` on the same condition. The committed slice is already
//! deduped and `node_status != "NotAvailable"`-filtered (see its `provenance.note`) and verified
//! conflict-free for the pinned SHA, so in practice this never trips — the check exists for when
//! the slice is regenerated against a future aoe2techtree revision.

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use serde::Deserialize;

use crate::error::{RefdataError, Result};

#[derive(Debug, Deserialize)]
struct TreeNodesDoc {
    observations: Vec<Observation>,
}

/// One `[node_id, name, use_type]` observation row — `use_type` is always `"Unit"` or `"Tech"` by
/// construction of the committed slice (see [`parse`]'s handling of anything else).
#[derive(Debug, Deserialize)]
struct Observation(i32, String, String);

/// `(use_type, node_id) -> name`, split into the two tables [`crate::unit_tech_names`] queries.
#[derive(Debug, Clone, Default)]
pub struct TreeNodeNames {
    units: HashMap<i32, String>,
    techs: HashMap<i32, String>,
}

impl TreeNodeNames {
    /// Resolves a `data.Unit` id to its base-game tree display name, or `None` if no base-game
    /// civ tree ever surfaces it (e.g. a legacy/unused id) — the caller treats `None` as "omit
    /// this id", never a fabricated name.
    pub fn unit_name(&self, node_id: i32) -> Option<&str> {
        self.units.get(&node_id).map(String::as_str)
    }

    /// Resolves a `data.Tech` id to its base-game tree display name. See [`Self::unit_name`].
    pub fn tech_name(&self, node_id: i32) -> Option<&str> {
        self.techs.get(&node_id).map(String::as_str)
    }
}

/// Parses `aoe2techtree-tree-nodes.json`-shaped text. Exposed standalone (not only via [`load`])
/// so tests can drive it with small inline fixtures.
pub fn parse(json: &str) -> Result<TreeNodeNames> {
    let doc: TreeNodesDoc = serde_json::from_str(json).map_err(|source| {
        RefdataError::ParseSource { file: "aoe2techtree-tree-nodes.json", source }
    })?;

    let mut units: HashMap<i32, String> = HashMap::new();
    let mut techs: HashMap<i32, String> = HashMap::new();
    for Observation(node_id, name, use_type) in doc.observations {
        let table = match use_type.as_str() {
            "Unit" => &mut units,
            "Tech" => &mut techs,
            other => {
                return Err(RefdataError::UnknownTreeNodeUseType {
                    node_id,
                    use_type: other.to_string(),
                });
            }
        };
        match table.entry(node_id) {
            Entry::Occupied(existing) if existing.get() != &name => {
                return Err(RefdataError::ConflictingTreeNodeName {
                    node_id,
                    use_type,
                    first: existing.get().clone(),
                    second: name,
                });
            }
            Entry::Occupied(_) => {} // identical repeat observation — fine, nothing to do
            Entry::Vacant(vacant) => {
                vacant.insert(name);
            }
        }
    }
    Ok(TreeNodeNames { units, techs })
}

/// Loads the real committed `aoe2techtree-tree-nodes.json`, baked into the binary at compile time.
pub fn load() -> Result<TreeNodeNames> {
    parse(include_str!(
        "../../../../reference-data/aoe2techtree-tree-nodes.json"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_unit_and_tech_names_by_node_id() {
        let json = r#"{"observations":[
            [4,"Archer","Unit"],
            [22,"Loom","Tech"]
        ]}"#;
        let t = parse(json).unwrap();
        assert_eq!(t.unit_name(4), Some("Archer"));
        assert_eq!(t.tech_name(22), Some("Loom"));
        assert_eq!(t.unit_name(999), None);
    }

    #[test]
    fn identical_repeated_observations_are_fine() {
        // A node_id seen under the same name more than once (e.g. two dedup passes overlapped)
        // must not be treated as a conflict.
        let json = r#"{"observations":[[4,"Archer","Unit"],[4,"Archer","Unit"]]}"#;
        let t = parse(json).unwrap();
        assert_eq!(t.unit_name(4), Some("Archer"));
    }

    #[test]
    fn conflicting_names_for_the_same_node_id_fail_loud() {
        let json = r#"{"observations":[[4,"Archer","Unit"],[4,"Levy","Unit"]]}"#;
        match parse(json).unwrap_err() {
            RefdataError::ConflictingTreeNodeName { node_id, first, second, .. } => {
                assert_eq!(node_id, 4);
                assert_eq!(first, "Archer");
                assert_eq!(second, "Levy");
            }
            other => panic!("expected ConflictingTreeNodeName, got {other:?}"),
        }
    }

    #[test]
    fn an_unexpected_use_type_fails_loud() {
        let json = r#"{"observations":[[4,"Archer","Building"]]}"#;
        match parse(json).unwrap_err() {
            RefdataError::UnknownTreeNodeUseType { node_id, use_type } => {
                assert_eq!(node_id, 4);
                assert_eq!(use_type, "Building");
            }
            other => panic!("expected UnknownTreeNodeUseType, got {other:?}"),
        }
    }

    #[test]
    fn real_slice_resolves_known_ids_and_has_no_conflicts() {
        let t = load().expect("committed slice must parse (also proves no conflicts)");
        assert_eq!(t.unit_name(4), Some("Archer"));
        assert_eq!(t.tech_name(22), Some("Loom"));
    }
}
