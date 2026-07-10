//! Builds `game-facts.json` from aoe2techtree ALONE (no aalises): per-unit `{name, age, building,
//! cost, attackBonus}` for the 48 units in [`NAME_TO_SLUG`].
//!
//! Each field's source (all from the committed aoe2techtree slices):
//! * `name` — the `<b>...</b>` help-string display name ([`crate::techtree`]).
//! * `cost` — `data.Unit.Cost`, SPARSE (only resources the unit spends).
//! * `age` + `building` — the civ-count MODE across all 53 base-game tech trees
//!   ([`crate::unit_tree`]): age from the unit's grid row, building from its grid block.
//! * `attackBonus` — `data.Unit.Attacks`, each non-base positive `{Amount, Class}` rendered
//!   `+Amount <label>` (label from [`crate::armor_classes`], aoe2techtree's OWN class vocabulary),
//!   joined by `;`; `null` when the unit has no bonus. This is AUTHORITATIVE STRUCTURED text and
//!   deliberately differs from the old aalises editorial free-text (different labels, corrected
//!   values, and some null↔non-null flips — e.g. the Archer's real `+3 Spearmen`; see the task
//!   report's diff list).
//!
//! Classes 3 (Base Pierce) / 4 (Base Melee) are the unit's OWN attack, never a bonus — skipped.

use std::collections::BTreeMap;

use crate::armor_classes::{self, ArmorClassTable};
use crate::error::Result;
use crate::model::{AgeRank, GameFactsDoc, GameFactsUnit, Source};
use crate::name_to_slug::NAME_TO_SLUG;
use crate::techtree::{self, RawAttack, RawCost, RawUnit, TechTree};
use crate::unit_tree::{self, UnitTreeTable};

/// The two armour classes that carry a unit's OWN base attack (pierce / melee), never a bonus.
const BASE_ATTACK_CLASSES: [i32; 2] = [3, 4];

/// `_generated` marker for the produced file — points at THIS crate (the new generator), not the
/// old JS one. (The committed file still carries the old marker; the shape-parity test only checks
/// this field's TYPE, not its value.)
const GENERATED_MARKER: &str = "pipeline/crates/refdata — DO NOT EDIT BY HAND";

/// Builds `game-facts.json` from the committed reference slices baked into this binary at compile
/// time — the entry point `main.rs` calls.
pub fn build_from_committed_reference_data() -> Result<GameFactsDoc> {
    build(
        &techtree::load()?,
        &unit_tree::load()?,
        &armor_classes::load(),
    )
}

/// Builds the whole `game-facts.json` document from already-parsed sources. Pure function of its
/// inputs — no filesystem access — so tests can drive it with inline fixtures.
pub fn build(
    tech_tree: &TechTree,
    unit_tree: &UnitTreeTable,
    armor: &ArmorClassTable,
) -> Result<GameFactsDoc> {
    let mut units = BTreeMap::new();
    for &(display_name, slug) in NAME_TO_SLUG {
        let unit = tech_tree.resolve(display_name, slug)?;
        let age_building = unit_tree.require(unit.id, slug)?;
        units.insert(
            slug.to_string(),
            GameFactsUnit {
                name: display_name.to_string(),
                age: age_building.age.clone(),
                building: age_building.building.clone(),
                cost: sparse_cost(&unit.cost),
                attack_bonus: attack_bonus(unit, armor)?,
            },
        );
    }

    Ok(GameFactsDoc {
        generated: GENERATED_MARKER.to_string(),
        source: Source {
            repo: "SiegeEngineers/aoe2techtree".to_string(),
            file: "data/data.json + data/trees/*.json + js/techtree.js".to_string(),
            sha: crate::AOE2TECHTREE_SHA.to_string(),
            license: "MIT code / MS Game Content Usage Rules assets".to_string(),
            url: format!(
                "https://github.com/SiegeEngineers/aoe2techtree/tree/{}",
                crate::AOE2TECHTREE_SHA
            ),
        },
        age_rank: AgeRank {
            dark: 1,
            feudal: 2,
            castle: 3,
            imperial: 4,
        },
        units,
    })
}

/// Sparse cost: only the non-zero resources the unit spends (the source `Cost` can carry an
/// explicit `"Food": 0` — e.g. Monk — which is dropped here to match `game-facts.json`'s "omit zero
/// resources" shape, distinct from `unit-stats.json`'s always-four-keys cost).
fn sparse_cost(cost: &RawCost) -> BTreeMap<String, i32> {
    let mut out = BTreeMap::new();
    for (key, value) in [
        ("food", cost.food),
        ("wood", cost.wood),
        ("gold", cost.gold),
        ("stone", cost.stone),
    ] {
        if value != 0 {
            out.insert(key.to_string(), value);
        }
    }
    out
}

/// Renders the unit's attack BONUSES as `+Amount <label>;...` in the source `Attacks` order, or
/// `None` when there are none. Base-attack classes (3/4) and non-positive amounts are skipped; an
/// unknown class fails loud (`RefdataError::UnknownAttackClass`) rather than emitting an unnamed
/// bonus.
fn attack_bonus(unit: &RawUnit, armor: &ArmorClassTable) -> Result<Option<String>> {
    let mut parts = Vec::new();
    for &RawAttack { class, amount } in &unit.attacks {
        if amount <= 0 || BASE_ATTACK_CLASSES.contains(&class) {
            continue;
        }
        parts.push(format!("+{amount} {}", armor.label(class)?));
    }
    Ok(if parts.is_empty() {
        None
    } else {
        Some(parts.join(";"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_reference_data_produces_forty_eight_units() {
        let doc = build_from_committed_reference_data()
            .expect("build must succeed against the real committed reference data");
        assert_eq!(doc.units.len(), 48);
        assert_eq!(
            doc.age_rank,
            AgeRank { dark: 1, feudal: 2, castle: 3, imperial: 4 }
        );
    }

    #[test]
    fn archer_facts_are_authoritative() {
        let doc = build_from_committed_reference_data().unwrap();
        let archer = &doc.units["archer"];
        assert_eq!(archer.name, "Archer");
        assert_eq!(archer.age, "feudal");
        assert_eq!(archer.building, "Archery Range");
        assert_eq!(archer.cost.get("wood"), Some(&25));
        assert_eq!(archer.cost.get("gold"), Some(&45));
        assert_eq!(archer.cost.get("food"), None, "sparse: no zero-valued food key");
        // Authoritative: the Archer really does +3 vs Spearmen (aalises listed null).
        assert_eq!(archer.attack_bonus.as_deref(), Some("+3 Spearmen"));
    }

    #[test]
    fn a_unit_with_no_bonus_is_null() {
        let doc = build_from_committed_reference_data().unwrap();
        assert_eq!(doc.units["militia"].attack_bonus, None);
        assert_eq!(doc.units["knight"].attack_bonus, None);
        assert_eq!(doc.units["monk"].attack_bonus, None);
    }

    #[test]
    fn transport_ship_age_is_the_authoritative_dark() {
        // The Dock is a Dark-age building; the tree places Transport Ship in Dark across all 53
        // civs (aalises said feudal — the one age divergence, and it's an authoritative correction).
        let doc = build_from_committed_reference_data().unwrap();
        assert_eq!(doc.units["transport-ship"].age, "dark");
    }

    #[test]
    fn attack_bonus_skips_base_classes_and_renders_labels() {
        // Minimal fixture: a fake unit with base pierce (class 3) + a bonus vs Spearmen (class 27).
        let armor = armor_classes::parse("27\tSpearmen\n");
        let unit = RawUnit {
            id: 4,
            language_name_id: 0,
            hp: 30,
            attack: 4,
            range: 4.0,
            min_range: 0.0,
            cost: RawCost::default(),
            train_time: 35,
            melee_armor: 0,
            pierce_armor: 0,
            attacks: vec![
                RawAttack { class: 3, amount: 4 },  // base pierce — skipped
                RawAttack { class: 27, amount: 3 }, // bonus vs Spearmen
                RawAttack { class: 15, amount: 0 }, // zero — skipped
            ],
        };
        assert_eq!(attack_bonus(&unit, &armor).unwrap().as_deref(), Some("+3 Spearmen"));
    }
}
