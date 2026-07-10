//! Typed serde mirrors of the committed `src/data/unit-stats.json` and `src/data/game-facts.json`
//! — field-for-field, in the SAME declared order (serde_json serializes struct fields in
//! declaration order), so `serde_json::to_string_pretty` reproduces each committed file's shape.
//!
//! **`tests/shape_parity.rs` deserializes the REAL committed files into these exact types** — if
//! every field these types declare round-trips against the site's live JSON, the shapes are
//! provably compatible. Since the aoe2techtree redesign, the VALUES this crate produces
//! deliberately DIFFER from the committed (aalises-based) files (unit-stats: ~68 units; game-facts:
//! authoritative age/cost/attackBonus) — those are aalises bugs the authoritative source corrects
//! (listed in the task report) — so only SHAPE parity is asserted, never value equality.
//!
//! `#[serde(deny_unknown_fields)]` on every FIXED-schema struct: a committed file gaining a new
//! top-level key this crate hasn't modeled yet must fail the parity test loud, never silently
//! ignore it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, Serializer};

/// A game stat that is USUALLY an integer but is occasionally fractional (only `range` — Fire Ship
/// / Fast Fire Ship are `2.5`; every other range and all min-ranges are whole). Backed by `f64`,
/// but serialized as a JSON INTEGER when the value is whole (`0`, `4`) and a float only when it
/// truly is (`2.5`) — so the emitted `unit-stats.json` keeps the committed file's integer style
/// for the 88 whole cases instead of printing `0.0`/`4.0`, while staying faithful to the two
/// genuinely-fractional source values (the site's Zod schema is `z.number()`, so both forms are
/// valid `number`s to the consumer).
#[derive(Debug, Clone, Copy)]
pub struct GameNumber(pub f64);

impl GameNumber {
    /// Largest magnitude an `f64` still represents every integer exactly (`2^53`) — beyond it the
    /// `as i64` narrowing below would silently round, so fall back to float. No game stat comes
    /// remotely close; this is belt-and-suspenders.
    const INT_EXACT_LIMIT: f64 = 9_007_199_254_740_992.0;
}

impl Serialize for GameNumber {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        if self.0.fract() == 0.0 && self.0.abs() < Self::INT_EXACT_LIMIT {
            serializer.serialize_i64(self.0 as i64)
        } else {
            serializer.serialize_f64(self.0)
        }
    }
}

impl<'de> Deserialize<'de> for GameNumber {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        // serde_json parses a bare JSON integer (`4`) into `f64` fine, so the committed file's
        // integer ranges deserialize without special-casing.
        Ok(GameNumber(f64::deserialize(d)?))
    }
}

impl PartialEq for GameNumber {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

// --- unit-stats.json -----------------------------------------------------------------------

/// The whole `unit-stats.json` document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitStatsDoc {
    /// Hand-set label (the game data carries no display game-version field) — see
    /// [`crate::unit_stats::PATCH`].
    pub patch: String,
    pub units: Vec<UnitEntry>,
}

/// One unit's numeric stats, keyed by `slug` (the `src/content/units/<slug>.yaml` key).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitEntry {
    pub slug: String,
    pub hp: i32,
    pub attack: i32,
    /// `f64`-backed but integer-serialized when whole — see [`GameNumber`] (Fire Ship's `2.5` is
    /// the only fractional range in the whole set).
    pub range: GameNumber,
    #[serde(rename = "minRange")]
    pub min_range: GameNumber,
    pub cost: Cost,
    #[serde(rename = "trainTime")]
    pub train_time: i32,
    #[serde(rename = "armorMelee")]
    pub armor_melee: i32,
    #[serde(rename = "armorPiercing")]
    pub armor_piercing: i32,
    /// Upgrade-line id (e.g. `"militia-line"`) — hand-curated, from
    /// `reference-data/unit-lines.tsv`, not derivable from the game data (the aoe2techtree
    /// `data.Unit` has no upgrade-line concept).
    pub line: String,
    /// Rank within `line`, 1-based (not always starting at 1 — see `unit-lines.tsv`'s doc).
    #[serde(rename = "lineRank")]
    pub line_rank: i32,
}

/// A unit's training cost — always all four keys, zero-defaulted: aoe2techtree's `data.Unit.Cost`
/// is a sparse capitalized dict (`{"Wood":25,"Gold":45}`), and [`crate::techtree::RawCost`]'s
/// `#[serde(default)]` zero-fills the absent resources into this fixed four-key shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cost {
    pub food: i32,
    pub wood: i32,
    pub gold: i32,
    pub stone: i32,
}

// --- game-facts.json ------------------------------------------------------------------------

/// The whole `game-facts.json` document (`{ _generated, _source, ageRank, units }`). Fully sourced
/// from aoe2techtree now — see [`crate::game_facts`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameFactsDoc {
    #[serde(rename = "_generated")]
    pub generated: String,
    #[serde(rename = "_source")]
    pub source: Source,
    #[serde(rename = "ageRank")]
    pub age_rank: AgeRank,
    /// Keyed by content slug — a data-keyed map, not a fixed field set. `BTreeMap` (alphabetical):
    /// key order isn't meaningful to the sole consumer (`src/pages/[lang]/units/[unit].astro` does
    /// a keyed lookup, never iterates).
    pub units: BTreeMap<String, GameFactsUnit>,
}

/// `_source` provenance block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub repo: String,
    pub file: String,
    pub sha: String,
    pub license: String,
    pub url: String,
}

/// `ageRank`: fixed four-age ordinal, never data-driven.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgeRank {
    pub dark: i32,
    pub feudal: i32,
    pub castle: i32,
    pub imperial: i32,
}

/// One unit's age/building/cost/attack-bonus facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameFactsUnit {
    /// The authoritative aoe2techtree display name (differs from the old aalises name for
    /// `Arbalester`/`Camel Rider`/`Heavy Camel Rider`).
    pub name: String,
    /// `"dark" | "feudal" | "castle" | "imperial"`, derived from the tech tree (see
    /// [`crate::unit_tree`]). Plain `String`, not a closed enum — this is a one-shot JSON export,
    /// not a `match`-driven DB column (playbook principle 2).
    pub age: String,
    /// Training building display name, from the tech tree.
    pub building: String,
    /// SPARSE cost — only the resources this unit spends appear as keys (mirrors `data.Unit.Cost`),
    /// so a `BTreeMap`, not the always-four-keys [`Cost`].
    pub cost: BTreeMap<String, i32>,
    /// `None` (JSON `null`) when the unit has no attack bonus, else `+N label;...` from
    /// `data.Unit.Attacks` + the armour-class labels (see [`crate::game_facts`]). Authoritative
    /// structured text — NOT aalises's editorial free-text.
    #[serde(rename = "attackBonus")]
    pub attack_bonus: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spot-check that `deny_unknown_fields` is actually wired (same style as `export`'s own
    /// model test) — proves an unmodeled new key fails the parity test loud rather than being
    /// silently dropped by serde's default lenient behavior.
    #[test]
    fn deny_unknown_fields_is_actually_wired_on_a_fixed_schema_struct() {
        let json = r#"{ "food": 60, "wood": 0, "gold": 20, "stone": 0, "unexpectedNewKey": true }"#;
        let err = serde_json::from_str::<Cost>(json).unwrap_err();
        assert!(
            err.to_string().contains("unknown field"),
            "expected an unknown-field error, got: {err}"
        );
    }
}
