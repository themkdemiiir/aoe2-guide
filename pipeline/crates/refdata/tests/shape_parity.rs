//! Shape-parity gate against the committed `src/data/{unit-stats,game-facts}.json`. SHAPE only —
//! since the aoe2techtree redesign, this crate's VALUES deliberately differ from the committed
//! aalises-based files (aalises bugs the authoritative source corrects; see the task report), so a
//! value-equality assertion would be wrong. What must still hold is that the crate's output is a
//! drop-in SHAPE for the site's Zod schema / TS consumers.
//!
//! Tiers, per file:
//! 1. the REAL committed JSON deserializes into this crate's model type (proves the model still
//!    mirrors the site's live shape);
//! 2. the crate's freshly-generated JSON round-trips through the SAME model (proves the produced
//!    shape is compatible), with authoritative-value spot checks.

use refdata::model::{GameFactsDoc, GameNumber, UnitStatsDoc};

const COMMITTED_UNIT_STATS_JSON: &str = include_str!("../../../../src/data/unit-stats.json");
const COMMITTED_GAME_FACTS_JSON: &str = include_str!("../../../../src/data/game-facts.json");

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
