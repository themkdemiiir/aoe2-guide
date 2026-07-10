//! Shape-parity gate against all five committed `src/data/*.json` files this crate derives:
//! `unit-stats.json`, `game-facts.json`, `unit-names.json`, `tech-names.json`, `icon-map.json`.
//!
//! `unit-stats`/`game-facts` are SHAPE-only — since the aoe2techtree redesign, this crate's VALUES
//! deliberately differ from the committed aalises-based files (aalises bugs the authoritative
//! source corrects; see the task report), so a value-equality assertion would be wrong there. What
//! must still hold is that the crate's output is a drop-in SHAPE for the site's Zod schema / TS
//! consumers.
//!
//! `unit-names`/`tech-names`/`icon-map` have no aalises predecessor to correct — both the
//! committed files and this crate derive straight from the same aoe2techtree base-game tree data —
//! so those three assert full VALUE equality too, not just shape.
//!
//! Tiers, per file:
//! 1. the REAL committed JSON deserializes into this crate's model type (proves the model still
//!    mirrors the site's live shape);
//! 2. the crate's freshly-generated JSON round-trips through the SAME model (proves the produced
//!    shape is compatible), with authoritative-value spot checks (or, for the three aalises-free
//!    files, full value-equality checks).

use refdata::model::{GameFactsDoc, GameNumber, IconMapDoc, NameMapDoc, UnitStatsDoc};

const COMMITTED_UNIT_STATS_JSON: &str = include_str!("../../../../src/data/unit-stats.json");
const COMMITTED_GAME_FACTS_JSON: &str = include_str!("../../../../src/data/game-facts.json");
const COMMITTED_UNIT_NAMES_JSON: &str = include_str!("../../../../src/data/unit-names.json");
const COMMITTED_TECH_NAMES_JSON: &str = include_str!("../../../../src/data/tech-names.json");
const COMMITTED_ICON_MAP_JSON: &str = include_str!("../../../../src/data/icon-map.json");

#[test]
fn committed_unit_stats_matches_the_doc_shape() {
    let doc: UnitStatsDoc = serde_json::from_str(COMMITTED_UNIT_STATS_JSON).expect(
        "committed src/data/unit-stats.json no longer matches refdata::model::UnitStatsDoc's shape",
    );
    assert_eq!(doc.units.len(), 90, "sanity: 90 canonical units");
    assert!(!doc.patch.is_empty());
}

#[test]
fn committed_game_facts_matches_the_doc_shape() {
    let doc: GameFactsDoc = serde_json::from_str(COMMITTED_GAME_FACTS_JSON).expect(
        "committed src/data/game-facts.json no longer matches refdata::model::GameFactsDoc's shape",
    );
    assert_eq!(doc.units.len(), 48, "sanity: 48 mapped units");
    assert!(!doc.source.sha.is_empty());
}

#[test]
fn generated_game_facts_is_a_drop_in_shape_with_authoritative_values() {
    let produced = refdata::game_facts::build_from_committed_reference_data()
        .expect("build_from_committed_reference_data must succeed");
    assert_eq!(produced.units.len(), 48);

    // Round-trips through the SAME model the committed file uses — shape-compatible drop-in.
    let json = serde_json::to_string(&produced).expect("serialize produced doc");
    let reparsed: GameFactsDoc =
        serde_json::from_str(&json).expect("produced JSON must re-parse into the model");
    assert_eq!(reparsed.units.len(), 48);

    // Authoritative spot checks: age/building from the tree, sparse cost, and the Archer's real
    // +3 Spearmen bonus (aalises listed null).
    let archer = &reparsed.units["archer"];
    assert_eq!(archer.age, "feudal");
    assert_eq!(archer.building, "Archery Range");
    assert_eq!(archer.attack_bonus.as_deref(), Some("+3 Spearmen"));
    assert!(reparsed.units["militia"].attack_bonus.is_none());
}

#[test]
fn generated_unit_stats_is_a_drop_in_shape_with_authoritative_values() {
    let produced = refdata::unit_stats::build_from_committed_reference_data()
        .expect("build_from_committed_reference_data must succeed");
    assert_eq!(produced.units.len(), 90);

    // The produced doc round-trips through the SAME model the committed file uses — i.e. it is a
    // shape-compatible drop-in (only values differ, which is the point of the redesign).
    let json = serde_json::to_string(&produced).expect("serialize produced doc");
    let reparsed: UnitStatsDoc =
        serde_json::from_str(&json).expect("produced JSON must re-parse into the model");
    assert_eq!(reparsed.units.len(), 90);

    // Authoritative-value spot check: the Archer HP/Range transpose that the old aalises pipeline
    // had to patch with STAT_CORRECTIONS is simply correct at the source now.
    let archer = reparsed.units.iter().find(|u| u.slug == "archer").unwrap();
    assert_eq!(archer.hp, 30);
    assert_eq!(archer.range, GameNumber(4.0));
}

#[test]
fn committed_unit_names_matches_the_doc_shape() {
    let doc: NameMapDoc = serde_json::from_str(COMMITTED_UNIT_NAMES_JSON).expect(
        "committed src/data/unit-names.json no longer matches refdata::model::NameMapDoc's shape",
    );
    assert_eq!(doc.map.len(), 238, "sanity: 238 resolved units");
    assert_eq!(doc.internal_names.len(), 238);
    assert!(!doc.provenance.sha.is_empty());
}

#[test]
fn committed_tech_names_matches_the_doc_shape() {
    let doc: NameMapDoc = serde_json::from_str(COMMITTED_TECH_NAMES_JSON).expect(
        "committed src/data/tech-names.json no longer matches refdata::model::NameMapDoc's shape",
    );
    assert_eq!(doc.map.len(), 192, "sanity: 192 resolved techs");
    assert_eq!(doc.internal_names.len(), 192);
}

#[test]
fn committed_icon_map_matches_the_doc_shape() {
    let doc: IconMapDoc = serde_json::from_str(COMMITTED_ICON_MAP_JSON).expect(
        "committed src/data/icon-map.json no longer matches refdata::model::IconMapDoc's shape",
    );
    assert_eq!(doc.units.len(), 278, "sanity: 278 unit icons");
    assert_eq!(doc.techs.len(), 242, "sanity: 242 tech icons");
    assert_eq!(doc.buildings.len(), 45, "sanity: 45 building icons");
    assert_eq!(doc.civs.len(), 59, "sanity: 59 civ icons");
}

#[test]
fn generated_unit_names_and_tech_names_are_value_identical_to_the_committed_files() {
    // Unlike unit-stats/game-facts (deliberately different VALUES post-redesign — see this file's
    // module doc), unit-tech-names has no aalises predecessor to correct: both the committed files
    // and this crate derive straight from the same aoe2techtree base-game tree data, so values
    // must match EXACTLY, not just shape.
    let committed_units: NameMapDoc = serde_json::from_str(COMMITTED_UNIT_NAMES_JSON).unwrap();
    let produced_units = refdata::unit_tech_names::build_unit_names()
        .expect("build_unit_names must succeed against the committed reference data");
    assert_eq!(produced_units.map, committed_units.map);
    assert_eq!(produced_units.internal_names, committed_units.internal_names);

    let committed_techs: NameMapDoc = serde_json::from_str(COMMITTED_TECH_NAMES_JSON).unwrap();
    let produced_techs = refdata::unit_tech_names::build_tech_names()
        .expect("build_tech_names must succeed against the committed reference data");
    assert_eq!(produced_techs.map, committed_techs.map);
    assert_eq!(produced_techs.internal_names, committed_techs.internal_names);
}

#[test]
fn generated_icon_map_is_value_identical_to_the_committed_file() {
    // Same rationale as the unit/tech-names value-identity test above: icon-map.json has no
    // aalises predecessor either, so a real `public/images/aoe2/` scan must reproduce the
    // committed file's content exactly, not just its shape.
    use std::collections::{BTreeMap, HashSet};
    use std::fs;

    let assets_dir = refdata::icon_map::default_assets_dir();
    let scan = |sub: &str| -> HashSet<String> {
        fs::read_dir(assets_dir.join(sub))
            .expect("assets dir must exist in this checkout")
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|f| f.strip_suffix(".png").map(str::to_string))
            .collect()
    };
    let civs: BTreeMap<String, String> = fs::read_dir(assets_dir.join("Civs"))
        .expect("Civs dir must exist")
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|f| f.ends_with(".png"))
        .map(|f| (f.trim_end_matches(".png").to_lowercase(), format!("/images/aoe2/Civs/{f}")))
        .collect();
    let inventory = refdata::icon_map::AssetInventory {
        unit_ids: scan("Unit"),
        tech_ids: scan("Tech"),
        building_ids: scan("Building"),
        civs,
    };

    let committed: IconMapDoc = serde_json::from_str(COMMITTED_ICON_MAP_JSON).unwrap();
    let produced = refdata::icon_map::build(&inventory).expect("icon_map::build must succeed");
    assert_eq!(produced, committed);
}

#[test]
fn generated_unit_stats_serializes_whole_ranges_as_ints_and_fractional_as_floats() {
    // GameNumber's raison d'être: keep the committed integer style for whole ranges while staying
    // faithful to the only fractional one (Fire Ship 2.5).
    let produced = refdata::unit_stats::build_from_committed_reference_data().unwrap();
    let json = serde_json::to_string_pretty(&produced).unwrap();
    // A whole range renders as a bare int (no `.0`)...
    assert!(
        json.contains("\"range\": 4,"),
        "whole ranges must serialize as integers"
    );
    // ...and the one fractional range renders as a float.
    assert!(
        json.contains("\"range\": 2.5,"),
        "Fire Ship's 2.5 range must serialize as a float"
    );
}
