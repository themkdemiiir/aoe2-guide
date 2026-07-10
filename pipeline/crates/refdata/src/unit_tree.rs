//! Per-unit canonical AGE + training BUILDING, derived from the committed
//! `reference-data/aoe2techtree-unit-tree.json` (SINGLE SOURCE OF TRUTH — read here). That slice is
//! the raw tech-tree observations across all 53 base-game civ trees; the derivation (age banding +
//! cross-civ mode) lives HERE, in Rust:
//!
//! * each observation is `[unit_id, building, grid_row, civ_count]`;
//! * a unit's AGE is `grid_row / 2` mapped to `[dark, feudal, castle, imperial]` — the tech tree's
//!   8 rows are four 2-row age bands (verified: Militia at row 1 = dark, Man-at-Arms row 2 =
//!   feudal, Long Swordsman row 4 = castle, Champion row 7 = imperial);
//! * the canonical `(building, age)` is the MODE across civs (summed `civ_count`), which resolves
//!   civ-specific building reskins (Donjon/Krepost/Settlement/Fortified Church) and layout variance
//!   to the standard value. Verified against the committed `game-facts.json`: 47/48 age+building
//!   match (the 1 diff — transport-ship dark vs the old aalises feudal — is authoritative: the Dock
//!   is a Dark-age building). See the slice's `provenance` note.

use std::collections::HashMap;

use serde::Deserialize;

use crate::error::{RefdataError, Result};

/// The four age bands a `grid_row / 2` indexes into.
const AGE_BANDS: [&str; 4] = ["dark", "feudal", "castle", "imperial"];

/// One unit's resolved canonical facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgeBuilding {
    /// `"dark" | "feudal" | "castle" | "imperial"`.
    pub age: String,
    /// The training building's display name (e.g. `"Archery Range"`).
    pub building: String,
}

/// unit_id -> canonical [`AgeBuilding`], the per-unit mode already computed by [`parse`].
#[derive(Debug, Clone, Default)]
pub struct UnitTreeTable(HashMap<i32, AgeBuilding>);

impl UnitTreeTable {
    /// Resolves a unit id, or [`RefdataError::MissingAgeBuilding`] — a canonical unit with no tree
    /// observation is a fail-loud gap (the pinned tree slice changed), never a fabricated age.
    pub fn require(&self, unit_id: i32, slug: &'static str) -> Result<&AgeBuilding> {
        self.0
            .get(&unit_id)
            .ok_or(RefdataError::MissingAgeBuilding { slug, unit_id })
    }
}

#[derive(Debug, Deserialize)]
struct TreeDoc {
    /// `[unit_id, building, grid_row, civ_count]` — a fixed-shape tuple row (serde deserializes a
    /// JSON array into this via the tuple struct below).
    observations: Vec<Observation>,
}

/// One `[unit_id, building, grid_row, civ_count]` observation row.
#[derive(Debug, Deserialize)]
struct Observation(i32, String, usize, u32);

/// Parses the tree-observation slice and computes each unit's canonical `(building, age)` as the
/// civ-count-weighted mode of `(building, grid_row / 2)`. Exposed standalone (not only via [`load`])
/// so tests can drive it with inline fixtures.
pub fn parse(json: &str) -> Result<UnitTreeTable> {
    let doc: TreeDoc = serde_json::from_str(json).map_err(|source| RefdataError::ParseSource {
        file: "aoe2techtree-unit-tree.json",
        source,
    })?;

    // unit_id -> (building, age_band) -> summed civ_count
    let mut tally: HashMap<i32, HashMap<(String, usize), u32>> = HashMap::new();
    for Observation(unit_id, building, row, count) in doc.observations {
        let band = (row / 2).min(AGE_BANDS.len() - 1);
        *tally
            .entry(unit_id)
            .or_default()
            .entry((building, band))
            .or_insert(0) += count;
    }

    let mut by_id = HashMap::with_capacity(tally.len());
    for (unit_id, counts) in tally {
        // Mode: the (building, band) with the most civ_count. Ties broken deterministically by the
        // smaller (building, band) so a given committed slice always yields the same result. Each
        // `counts` map is non-empty by construction (an entry only exists once an observation was
        // tallied into it), so `max_by` is always `Some`; the `if let` keeps this `pub fn`
        // panic-free per the playbook rather than `.expect()`-ing that invariant.
        if let Some(((building, band), _)) =
            counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        {
            by_id.insert(
                unit_id,
                AgeBuilding {
                    age: AGE_BANDS[band].to_string(),
                    building,
                },
            );
        }
    }
    Ok(UnitTreeTable(by_id))
}

/// Loads the real committed `aoe2techtree-unit-tree.json`, baked into the binary at compile time.
pub fn load() -> Result<UnitTreeTable> {
    parse(include_str!(
        "../../../../reference-data/aoe2techtree-unit-tree.json"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_the_mode_and_bands_rows_to_ages() {
        // Militia (id 74) seen at Barracks row 1 (dark) in 3 civs; Man-at-Arms (75) at row 2
        // (feudal). A minority reskin observation (Settlement) must lose to the Barracks mode.
        let json = r#"{"observations":[
            [74,"Barracks",1,53],
            [75,"Barracks",2,50],
            [75,"Settlement",2,3]
        ]}"#;
        let t = parse(json).unwrap();
        assert_eq!(
            t.require(74, "militia").unwrap(),
            &AgeBuilding { age: "dark".into(), building: "Barracks".into() }
        );
        let maa = t.require(75, "man-at-arms").unwrap();
        assert_eq!(maa.age, "feudal");
        assert_eq!(maa.building, "Barracks", "the Barracks mode must beat the Settlement reskin");
    }

    #[test]
    fn same_age_split_across_two_rows_still_modes_correctly() {
        // Two-Handed Swordsman at Barracks rows 6 AND 7 (both imperial) — banding must merge them
        // so imperial wins over a stray castle observation.
        let json = r#"{"observations":[
            [473,"Barracks",6,30],
            [473,"Barracks",7,20],
            [473,"Barracks",4,5]
        ]}"#;
        let t = parse(json).unwrap();
        assert_eq!(t.require(473, "two-handed-swordsman").unwrap().age, "imperial");
    }

    #[test]
    fn missing_unit_fails_loud() {
        let t = parse(r#"{"observations":[]}"#).unwrap();
        match t.require(999, "ghost").unwrap_err() {
            RefdataError::MissingAgeBuilding { slug, unit_id } => {
                assert_eq!(slug, "ghost");
                assert_eq!(unit_id, 999);
            }
            other => panic!("expected MissingAgeBuilding, got {other:?}"),
        }
    }

    #[test]
    fn real_slice_resolves_known_units() {
        let t = load().expect("committed unit-tree slice must parse");
        // Archer (id 4) -> Archery Range, feudal.
        let archer = t.require(4, "archer").unwrap();
        assert_eq!(archer.building, "Archery Range");
        assert_eq!(archer.age, "feudal");
        // Militia (id 74) -> Barracks, dark.
        let militia = t.require(74, "militia").unwrap();
        assert_eq!(militia.building, "Barracks");
        assert_eq!(militia.age, "dark");
    }
}
