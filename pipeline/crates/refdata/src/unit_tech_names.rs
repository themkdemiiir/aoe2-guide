//! Builds `unit-names.json` + `tech-names.json` from aoe2techtree ALONE — the same two files
//! `scripts/build-unit-tech-names.mjs` produces. These are `include_str!`'d into `pipeline-core`
//! (`units.rs`/`techs.rs`, behind the `refdata` feature) for the replay-ingest units/techs
//! dimension tables — see those modules' doc for the id space they back (`GameUnitId`/`TechId`).
//!
//! Pipeline, mirroring `build-unit-tech-names.mjs`'s `buildDim` exactly:
//! 1. walk the FULL authoritative DAT id space for units ([`crate::techtree::load_unit_ids`], 245
//!    ids) / techs ([`crate::techs::load_tech_ids`], 194 ids) — NOT the tree data, so an id with
//!    zero tree observations still gets a chance;
//! 2. resolve each id's base-game civ-tree display name via [`crate::tree_nodes`];
//! 3. an id with no resolvable name is OMITTED from both `map` and `internalNames` — never
//!    guessed (mirrors `buildDim`'s `skipped` branch: these ids aren't reachable by a normal
//!    `train`/`research` command, e.g. a unit's internal alt-form id).
//!
//! Two separate `build_*` functions (not one generic one) — deliberately, to keep each a plain,
//! easy-to-follow loop rather than a generic-over-id-space abstraction this crate would only ever
//! instantiate twice.

use std::collections::BTreeMap;

use crate::error::Result;
use crate::model::{NameMapDoc, NameMapProvenance};
use crate::{techs, techtree, tree_nodes};

/// Shared `provenance.note` text for both output files — verbatim from
/// `scripts/build-unit-tech-names.mjs`'s own `provenanceNote`, so the committed files' prose
/// stays identical regardless of which generator (JS or this crate) last produced them.
const PROVENANCE_NOTE: &str = "id -> English display name. Ids + internal_name from aoe2techtree \
data/data.json's data.Unit/data.Tech; names resolved from base-game civ tree files \
(data/trees/*.json units_techs[].name, matched by node_id) at the same pinned SHA — NOT \
chronicles/data/trees (Return of Rome reskins some shared ids with different flavor names). Ids \
with no resolvable base-game tree name are omitted, never guessed — see \
scripts/build-unit-tech-names.mjs.";

fn provenance() -> NameMapProvenance {
    NameMapProvenance {
        source: "SiegeEngineers/aoe2techtree".to_string(),
        sha: crate::AOE2TECHTREE_SHA.to_string(),
        note: PROVENANCE_NOTE.to_string(),
    }
}

/// Builds `unit-names.json` from the committed reference data baked into this binary at compile
/// time — one of the two entry points `main.rs` calls.
pub fn build_unit_names() -> Result<NameMapDoc> {
    let unit_ids = techtree::load_unit_ids()?;
    let tree_names = tree_nodes::load()?;

    let mut map = BTreeMap::new();
    let mut internal_names = BTreeMap::new();
    for (id, unit) in unit_ids {
        let Some(name) = tree_names.unit_name(id) else {
            continue;
        };
        let id_str = id.to_string();
        map.insert(id_str.clone(), name.to_string());
        if let Some(internal) = unit.internal_name {
            internal_names.insert(id_str, internal);
        }
    }

    Ok(NameMapDoc { provenance: provenance(), map, internal_names })
}

/// Builds `tech-names.json` from the committed reference data baked into this binary at compile
/// time — the other entry point `main.rs` calls.
pub fn build_tech_names() -> Result<NameMapDoc> {
    let tech_ids = techs::load_tech_ids()?;
    let tree_names = tree_nodes::load()?;

    let mut map = BTreeMap::new();
    let mut internal_names = BTreeMap::new();
    for (id, tech) in tech_ids {
        let Some(name) = tree_names.tech_name(id) else {
            continue;
        };
        let id_str = id.to_string();
        map.insert(id_str.clone(), name.to_string());
        if let Some(internal) = tech.internal_name {
            internal_names.insert(id_str, internal);
        }
    }

    Ok(NameMapDoc { provenance: provenance(), map, internal_names })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_reference_data_produces_the_committed_counts() {
        // Matches the committed src/data/{unit,tech}-names.json resolved counts exactly (see
        // tests/shape_parity.rs for the shape-parity side of this).
        let units = build_unit_names().expect("build_unit_names must succeed");
        assert_eq!(units.map.len(), 238);
        assert_eq!(units.internal_names.len(), 238);

        let techs = build_tech_names().expect("build_tech_names must succeed");
        assert_eq!(techs.map.len(), 192);
        assert_eq!(techs.internal_names.len(), 192);
    }

    #[test]
    fn scout_cavalry_and_villager_resolve_like_pipeline_core_expects() {
        // Cross-checks `pipeline_core::units`'s own doctests (unit id 448 = Scout Cavalry, 83 =
        // Villager) — this crate's output must keep backing those FK lookups correctly.
        let units = build_unit_names().unwrap();
        assert_eq!(units.map.get("448"), Some(&"Scout Cavalry".to_string()));
        assert_eq!(units.map.get("83"), Some(&"Villager".to_string()));
        assert_eq!(units.internal_names.get("448"), Some(&"SCOUT".to_string()));
    }

    #[test]
    fn loom_resolves_like_pipeline_core_expects() {
        // Cross-checks `pipeline_core::techs`'s own doctest (tech id 22 = Loom).
        let techs = build_tech_names().unwrap();
        assert_eq!(techs.map.get("22"), Some(&"Loom".to_string()));
    }

    #[test]
    fn watched_techs_all_resolve() {
        // `replay::config::WATCHED_TECHS`'s exact ids (see `pipeline_core::techs`'s own test of
        // the same list) must all resolve to a name here too.
        let techs = build_tech_names().unwrap();
        for id in [22, 213, 249, 202, 203, 14, 13, 55, 182, 278] {
            assert!(
                techs.map.contains_key(&id.to_string()),
                "tech id {id} must resolve to a name"
            );
        }
    }
}
