//! Reads the committed aoe2techtree source slices and resolves a display name -> authoritative
//! unit stats, the way `scripts/build-unit-descriptions.mjs` does: a unit's English display name
//! is the `<b>...</b>` span of its help string, keyed by `LanguageNameId + 21000`.
//!
//! Two committed slices (both `include_str!`'d at compile time, both sliced verbatim from
//! `.cache/aoe2-data/{data.json,strings-en.json}` at the pinned `AOE2TECHTREE_SHA`):
//!   * `reference-data/aoe2techtree-units.json`      — the whole `data.Unit` map (245 units)
//!   * `reference-data/aoe2techtree-unit-names.json` — `LanguageNameId+21000 -> help string`
//!
//! This is the SINGLE SOURCE OF TRUTH for unit stats — the aalises `units.csv` middleman (and its
//! `STAT_CORRECTIONS` patches against THIS very file) are gone. See the crate `lib.rs` doc.

use std::collections::HashMap;

use serde::Deserialize;

use crate::error::{RefdataError, Result};

/// The offset the game adds to a unit's `LanguageNameId` to reach its "Create <b>Name</b> (...)"
/// help string — verified in `scripts/build-unit-descriptions.mjs` (`LanguageNameId + 21000`).
const HELP_STRING_OFFSET: i64 = 21_000;

/// One `data.Unit` entry, as sliced into `reference-data/aoe2techtree-units.json`. Only the fields
/// this crate consumes are modeled; serde ignores the ~17 others (`Speed`, `LineOfSight`,
/// `Attacks`, ...) present in the slice — no `deny_unknown_fields` here, on purpose (the slice is a
/// faithful superset kept for future use, not a fixed schema this crate owns). Every numeric field
/// is `#[serde(default)]` so a non-combat game object in the 245-unit map that lacks e.g. `HP`
/// still deserializes (this crate only ever reads the stats of a unit it successfully resolves by
/// name, all of which are real combat units with real values).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawUnit {
    #[serde(rename = "ID", default)]
    pub id: i32,
    #[serde(rename = "LanguageNameId", default)]
    pub language_name_id: i64,
    #[serde(rename = "HP", default)]
    pub hp: i32,
    #[serde(rename = "Attack", default)]
    pub attack: i32,
    /// Fractional in the source for exactly two units (Fire Ship / Fast Fire Ship, `2.5`); every
    /// other unit is integral — see [`crate::model::GameNumber`] for how that round-trips to JSON.
    #[serde(rename = "Range", default)]
    pub range: f64,
    #[serde(rename = "MinRange", default)]
    pub min_range: f64,
    #[serde(rename = "Cost", default)]
    pub cost: RawCost,
    #[serde(rename = "TrainTime", default)]
    pub train_time: i32,
    #[serde(rename = "MeleeArmor", default)]
    pub melee_armor: i32,
    #[serde(rename = "PierceArmor", default)]
    pub pierce_armor: i32,
    /// Per-armour-class attack amounts (`{Amount, Class}`), including the unit's own base melee/
    /// pierce (classes 4/3). `game-facts.json`'s `attackBonus` is derived from the NON-base
    /// positive entries — see [`crate::game_facts`]. Empty for a unit with no `Attacks` array.
    #[serde(rename = "Attacks", default)]
    pub attacks: Vec<RawAttack>,
    /// The DAT internal name (e.g. `"ARCHR"`) — read only by [`crate::unit_tech_names`] (via
    /// [`load_unit_ids`]), documentation/debugging downstream, never used for lookups here.
    /// `#[serde(default)]` so an id lacking one still deserializes.
    #[serde(rename = "internal_name", default)]
    pub internal_name: Option<String>,
}

impl RawUnit {
    /// The stat fields that must match across two ids sharing a display name for the collision to
    /// be a safe "same unit, duplicate graphics/formation id" case (see [`TechTree::resolve`]).
    /// `ID`/`LanguageNameId` are deliberately excluded — those legitimately differ between the
    /// duplicates — and so are `attacks` (a graphics/formation dup can carry a differently-ordered
    /// but equivalent Attacks array; the 8 numeric stats already pin identity).
    fn stats_eq(&self, other: &RawUnit) -> bool {
        self.hp == other.hp
            && self.attack == other.attack
            && self.range == other.range
            && self.min_range == other.min_range
            && self.cost == other.cost
            && self.train_time == other.train_time
            && self.melee_armor == other.melee_armor
            && self.pierce_armor == other.pierce_armor
    }
}

/// One `data.Unit.Attacks` entry: `Amount` damage against armour `Class` (the id
/// `reference-data/aoe2techtree-armor-classes.tsv` names).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct RawAttack {
    #[serde(rename = "Class")]
    pub class: i32,
    #[serde(rename = "Amount")]
    pub amount: i32,
}

/// `data.Unit.Cost`, whose keys are capitalized in the source (`{"Food":..,"Gold":..}`) and sparse
/// (a resource a unit doesn't spend is simply absent). `#[serde(default)]` zero-fills the absent
/// ones, giving the always-four-keys shape `unit-stats.json` wants directly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct RawCost {
    #[serde(rename = "Food", default)]
    pub food: i32,
    #[serde(rename = "Wood", default)]
    pub wood: i32,
    #[serde(rename = "Gold", default)]
    pub gold: i32,
    #[serde(rename = "Stone", default)]
    pub stone: i32,
}

#[derive(Debug, Deserialize)]
struct UnitsDoc {
    units: HashMap<String, RawUnit>,
}

#[derive(Debug, Deserialize)]
struct NamesDoc {
    strings: HashMap<String, String>,
}

/// Authoritative unit stats keyed by resolved English display name. Built once via [`load`]/
/// [`parse`], then queried per canonical unit by [`resolve`](TechTree::resolve).
#[derive(Debug, Clone)]
pub struct TechTree {
    /// display name -> every unit id carrying it, sorted ascending (so "pick the lowest id" is
    /// `[0]`). A name maps to >1 id only for duplicate graphics/formation variants of the SAME
    /// unit — [`resolve`](TechTree::resolve) proves that via a stat-identity check.
    by_name: HashMap<String, Vec<RawUnit>>,
}

impl TechTree {
    /// Resolves a display name (the exact `<b>...</b>` text) to its authoritative stats.
    ///
    /// * no id -> [`RefdataError::UnitNotFound`] (the display name isn't in the source at all);
    /// * one id -> that unit;
    /// * many ids, all stat-identical -> the lowest-id one (a duplicate graphics/formation variant,
    ///   e.g. Spearman's `PKEMN`/`DPKEMN` — verified stat-identical, so the choice is immaterial);
    /// * many ids with DIFFERING stats -> [`RefdataError::AmbiguousUnit`] (fail loud — never guess
    ///   which of two genuinely different units the caller meant), mirroring
    ///   `build-unit-descriptions.mjs`'s "differing help text -> fail" guard, applied to stats here.
    pub fn resolve(&self, display_name: &'static str, slug: &'static str) -> Result<&RawUnit> {
        let units = self
            .by_name
            .get(display_name)
            .filter(|v| !v.is_empty())
            .ok_or(RefdataError::UnitNotFound { display_name, slug })?;

        let first = &units[0];
        if let Some(divergent) = units.iter().find(|u| !u.stats_eq(first)) {
            return Err(RefdataError::AmbiguousUnit {
                display_name,
                slug,
                ids: (first.id, divergent.id),
            });
        }
        Ok(first)
    }
}

/// Parses the two slice texts into a [`TechTree`]. Exposed standalone (not only via [`load`]) so
/// tests can drive it with small inline fixtures. A unit whose `LanguageNameId+21000` has no help
/// string, or whose help string has no `<b>...</b>` span, is skipped (never errors) — exactly
/// `build-unit-descriptions.mjs`'s `continue`; those game objects simply aren't nameable units.
pub fn parse(units_json: &str, names_json: &str) -> Result<TechTree> {
    let units: UnitsDoc = serde_json::from_str(units_json)
        .map_err(|source| RefdataError::ParseSource { file: "aoe2techtree-units.json", source })?;
    let names: NamesDoc = serde_json::from_str(names_json).map_err(|source| {
        RefdataError::ParseSource { file: "aoe2techtree-unit-names.json", source }
    })?;

    let mut by_name: HashMap<String, Vec<RawUnit>> = HashMap::new();
    for unit in units.units.into_values() {
        let help_id = (unit.language_name_id + HELP_STRING_OFFSET).to_string();
        let Some(help) = names.strings.get(&help_id) else {
            continue;
        };
        let Some(name) = bold_span(help) else {
            continue;
        };
        by_name.entry(name.to_string()).or_default().push(unit);
    }
    for units in by_name.values_mut() {
        units.sort_by_key(|u| u.id);
    }
    Ok(TechTree { by_name })
}

/// Loads the two real committed slices, baked into the binary at compile time.
pub fn load() -> Result<TechTree> {
    parse(
        include_str!("../../../../reference-data/aoe2techtree-units.json"),
        include_str!("../../../../reference-data/aoe2techtree-unit-names.json"),
    )
}

/// Parses `aoe2techtree-units.json` ALONE into the full `data.Unit` id space (245 ids), keyed by
/// id rather than grouped by resolved display name. Unlike [`parse`]/[`TechTree::by_name`] (which
/// only covers units whose `LanguageNameId+21000` resolves to a help-string name), this covers
/// EVERY id `data.Unit` defines — [`crate::unit_tech_names`] needs that full space (mirroring
/// `scripts/build-unit-tech-names.mjs`'s `idInfo = dataJson.data.Unit` walk), since a unit's
/// tree-node display name (resolved separately, via [`crate::tree_nodes`]) is a DIFFERENT
/// resolution path than the help-string one `parse` uses. Exposed standalone (not only via
/// [`load_unit_ids`]) so tests can drive it with a small inline fixture.
pub fn parse_unit_ids(units_json: &str) -> Result<HashMap<i32, RawUnit>> {
    let units: UnitsDoc = serde_json::from_str(units_json)
        .map_err(|source| RefdataError::ParseSource { file: "aoe2techtree-units.json", source })?;
    Ok(units.units.into_values().map(|u| (u.id, u)).collect())
}

/// Loads the real committed `aoe2techtree-units.json` ALONE, baked into the binary at compile
/// time — the full unit DAT id space. See [`parse_unit_ids`].
pub fn load_unit_ids() -> Result<HashMap<i32, RawUnit>> {
    parse_unit_ids(include_str!(
        "../../../../reference-data/aoe2techtree-units.json"
    ))
}

/// Extracts the first `<b>...</b>` span's inner text — a regex-free port of the source scripts'
/// `.match(/<b>(.+?)<\/b>/)` (kept regex-free per `pipeline-core`'s convention). `None` when the
/// string has no such span.
fn bold_span(help: &str) -> Option<&str> {
    let start = help.find("<b>")? + "<b>".len();
    let end = help[start..].find("</b>")? + start;
    Some(&help[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNITS: &str = r#"{"provenance":{},"units":{
        "4":{"ID":4,"LanguageNameId":5083,"HP":30,"Attack":4,"Range":4,"MinRange":0,
             "Cost":{"Wood":25,"Gold":45},"TrainTime":35,"MeleeArmor":0,"PierceArmor":0},
        "93":{"ID":93,"LanguageNameId":5075,"HP":45,"Attack":3,"Range":0,"MinRange":0,
              "Cost":{"Food":35,"Wood":25},"TrainTime":22,"MeleeArmor":0,"PierceArmor":0},
        "1786":{"ID":1786,"LanguageNameId":5075,"HP":45,"Attack":3,"Range":0,"MinRange":0,
                "Cost":{"Food":35,"Wood":25},"TrainTime":22,"MeleeArmor":0,"PierceArmor":0}
    }}"#;
    const NAMES: &str = r#"{"provenance":{},"strings":{
        "26083":"Create <b>Archer</b> (‹cost›)<br>\nAll-purpose Foot Archer.",
        "26075":"Create <b>Spearman</b> (‹cost›)<br>\nMedium infantry."
    }}"#;

    #[test]
    fn resolves_a_unique_name_to_its_stats() {
        let tt = parse(UNITS, NAMES).unwrap();
        let archer = tt.resolve("Archer", "archer").unwrap();
        assert_eq!(archer.hp, 30);
        assert_eq!(archer.range, 4.0);
        assert_eq!(archer.cost, RawCost { food: 0, wood: 25, gold: 45, stone: 0 });
    }

    #[test]
    fn stat_identical_duplicate_ids_resolve_to_the_lowest_id() {
        // Spearman has ids 93 and 1786 (a graphics/formation duplicate) with identical stats.
        let tt = parse(UNITS, NAMES).unwrap();
        let spearman = tt.resolve("Spearman", "spearman").unwrap();
        assert_eq!(spearman.id, 93, "the lowest id wins when stats are identical");
    }

    #[test]
    fn ambiguous_differing_stats_fail_loud() {
        // Same name, DIFFERENT hp -> must not silently pick one.
        let units = r#"{"units":{
            "10":{"ID":10,"LanguageNameId":1,"HP":30,"Attack":4},
            "11":{"ID":11,"LanguageNameId":1,"HP":99,"Attack":4}
        }}"#;
        let names = r#"{"strings":{"21001":"Create <b>Clash</b> (x)<br>\nx"}}"#;
        let tt = parse(units, names).unwrap();
        match tt.resolve("Clash", "clash").unwrap_err() {
            RefdataError::AmbiguousUnit { ids, .. } => assert_eq!(ids, (10, 11)),
            other => panic!("expected AmbiguousUnit, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_name_fails_loud() {
        let tt = parse(UNITS, NAMES).unwrap();
        match tt.resolve("Nonexistent", "nonexistent").unwrap_err() {
            RefdataError::UnitNotFound { .. } => {}
            other => panic!("expected UnitNotFound, got {other:?}"),
        }
    }

    #[test]
    fn bold_span_extracts_the_name() {
        assert_eq!(bold_span("Create <b>Archer</b> (x)"), Some("Archer"));
        assert_eq!(bold_span("no bold here"), None);
    }

    #[test]
    fn parse_unit_ids_covers_the_full_id_space_including_unnamed_units() {
        // Unlike `parse`/`by_name`, `parse_unit_ids` keeps every id — even one whose
        // LanguageNameId has no matching help string in NAMES (there is none here at all).
        let ids = parse_unit_ids(UNITS).unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(ids[&4].hp, 30);
        assert_eq!(ids[&93].id, 93);
    }

    #[test]
    fn real_unit_ids_slice_has_the_full_245_id_space_with_internal_names() {
        let ids = load_unit_ids().expect("committed slice must parse");
        assert_eq!(ids.len(), 245);
        assert_eq!(ids[&4].internal_name.as_deref(), Some("ARCHR"));
    }

    #[test]
    fn real_slices_load_and_resolve_a_known_unit() {
        let tt = load().expect("committed slices must parse");
        let archer = tt.resolve("Archer", "archer").expect("Archer resolves");
        // The whole point of the redesign: aoe2techtree has HP 30 / Range 4 (aalises had them
        // transposed to 4/30, which is why the old crate needed a STAT_CORRECTIONS patch).
        assert_eq!(archer.hp, 30);
        assert_eq!(archer.range, 4.0);
    }
}
