//! Builds `unit-stats.json` from the authoritative aoe2techtree source
//! ([`crate::techtree`]) + the curated `reference-data/unit-lines.tsv`.
//!
//! Pipeline:
//! 1. resolve each [`CANONICAL_UNITS`] display name to its unit via [`TechTree::resolve`] (the
//!    game's own `<b>...</b>` help-string name → unit id, with a stat-identity guard for
//!    duplicate graphics/formation ids);
//! 2. walk `CANONICAL_UNITS` IN ORDER (the output array's order), mapping each unit's authoritative
//!    stats straight across (`HP→hp`, `Attack→attack`, `Range→range`, `MinRange→minRange`,
//!    `Cost→{food,wood,gold,stone}`, `TrainTime→trainTime`, `MeleeArmor→armorMelee`,
//!    `PierceArmor→armorPiercing`);
//! 3. attach the curated `line`/`lineRank` from `unit-lines.tsv`.
//!
//! **No `STAT_CORRECTIONS`** — the old crate's Archer (HP/Range transpose) and Longbowman patches
//! existed only to fix the aalises CSV *against this exact source*; sourcing directly from
//! aoe2techtree makes them meaningless and they're deleted. A canonical unit that fails to resolve
//! ([`RefdataError::UnitNotFound`]) or whose curated line is missing
//! ([`RefdataError::MissingUnitLine`]) is a hard stop — never a fabricated/zeroed placeholder
//! (this repo's "no defaults, fail loud" rule).

use crate::canonical_units::CANONICAL_UNITS;
use crate::error::Result;
use crate::model::{Cost, GameNumber, UnitEntry, UnitStatsDoc};
use crate::techtree::{self, RawCost, RawUnit, TechTree};
use crate::unit_lines::{self, UnitLineTable};

/// Hand-set patch label — the game data carries no display game-version string, so this preserves
/// the previously committed `unit-stats.json`'s label. Bump manually when re-syncing the source
/// slices to a newer patch.
pub const PATCH: &str = "v100.1.84";

/// Builds `unit-stats.json` from the committed reference data baked into this binary at compile
/// time — the entry point `main.rs` calls.
pub fn build_from_committed_reference_data() -> Result<UnitStatsDoc> {
    build(&techtree::load()?, &unit_lines::load())
}

/// Builds the whole `unit-stats.json` document from an already-parsed [`TechTree`] and
/// `unit-lines.tsv` table. Pure function of its two inputs — no filesystem access — so tests can
/// exercise it against small inline fixtures as well as the real committed data.
pub fn build(tech_tree: &TechTree, unit_lines_table: &UnitLineTable) -> Result<UnitStatsDoc> {
    let mut units = Vec::with_capacity(CANONICAL_UNITS.len());
    for &(display_name, slug) in CANONICAL_UNITS {
        let unit = tech_tree.resolve(display_name, slug)?;
        let line = unit_lines::require(unit_lines_table, slug)?;
        units.push(UnitEntry {
            slug: slug.to_string(),
            hp: unit.hp,
            attack: unit.attack,
            range: GameNumber(unit.range),
            min_range: GameNumber(unit.min_range),
            cost: cost_of(unit),
            train_time: unit.train_time,
            armor_melee: unit.melee_armor,
            armor_piercing: unit.pierce_armor,
            line: line.line.clone(),
            line_rank: line.line_rank,
        });
    }

    Ok(UnitStatsDoc {
        patch: PATCH.to_string(),
        units,
    })
}

/// Maps a resolved unit's already-zero-filled [`RawCost`] to the output [`Cost`] (identical shape;
/// this is a field move, not a transform).
fn cost_of(unit: &RawUnit) -> Cost {
    let RawCost {
        food,
        wood,
        gold,
        stone,
    } = unit.cost;
    Cost {
        food,
        wood,
        gold,
        stone,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_reference_data_produces_ninety_units_without_error() {
        let doc = build_from_committed_reference_data()
            .expect("build must succeed against the real committed reference data");
        assert_eq!(doc.units.len(), 90);
        assert_eq!(doc.patch, PATCH);
    }

    #[test]
    fn archer_uses_the_authoritative_untransposed_stats() {
        // The redesign's whole point: aoe2techtree Archer is HP 30 / Range 4 (aalises transposed
        // them to 4/30, which is why the old crate carried a STAT_CORRECTIONS patch — now gone).
        let doc = build_from_committed_reference_data().unwrap();
        let archer = doc.units.iter().find(|u| u.slug == "archer").unwrap();
        assert_eq!(archer.hp, 30);
        assert_eq!(archer.range, GameNumber(4.0));
    }

    #[test]
    fn fire_ship_range_is_the_fractional_source_value() {
        // The one place a range is non-integer — proves GameNumber carries 2.5 through, not a
        // truncated 2 (which is what the old aalises `parseInt` produced).
        let doc = build_from_committed_reference_data().unwrap();
        let fire_ship = doc.units.iter().find(|u| u.slug == "fire-ship").unwrap();
        assert_eq!(fire_ship.range, GameNumber(2.5));
    }

    #[test]
    fn missing_unit_line_fails_loud() {
        // Real techtree (every canonical unit resolves), but an EMPTY line table -> MissingUnitLine.
        let tt = techtree::load().unwrap();
        let err = build(&tt, &UnitLineTable::default()).unwrap_err();
        match err {
            crate::error::RefdataError::MissingUnitLine { slug } => assert_eq!(slug, "militia"),
            other => panic!("expected MissingUnitLine, got {other:?}"),
        }
    }
}
