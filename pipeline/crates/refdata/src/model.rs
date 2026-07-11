//! Typed serde mirrors of the five committed `src/data/*.json` files this crate derives:
//! `unit-stats.json`, `game-facts.json`, `unit-names.json`, `tech-names.json`, `icon-map.json` —
//! field-for-field, in the SAME declared order (serde_json serializes struct fields in declaration
//! order), so `serde_json::to_string_pretty` reproduces each committed file's shape.
//!
//! **`tests/shape_parity.rs` deserializes the REAL committed files into these exact types** — if
//! every field these types declare round-trips against the site's live JSON, the shapes are
//! provably compatible. Since the aoe2techtree redesign, the VALUES `unit_stats`/`game_facts`
//! produce deliberately DIFFER from the committed (aalises-based) files (unit-stats: ~68 units;
//! game-facts: authoritative age/cost/attackBonus) — those are aalises bugs the authoritative
//! source corrects (listed in the task report) — so only SHAPE parity is asserted for those two,
//! never value equality. `unit_tech_names`/`icon_map` have no aalises predecessor to correct, so
//! their shape-parity tests assert full VALUE equality against the committed files too.
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

// --- unit-names.json / tech-names.json ------------------------------------------------------

/// Shared doc shape for BOTH `unit-names.json` and `tech-names.json` — identical structure,
/// produced by [`crate::unit_tech_names::build_unit_names`] /
/// [`crate::unit_tech_names::build_tech_names`] respectively.
///
/// `map`/`internalNames` are `BTreeMap`s (alphabetical key order) — order isn't meaningful to the
/// sole consumers (`pipeline_core::units::parse_units` / `pipeline_core::techs::parse_techs`
/// deserialize straight into a `HashMap` and iterate unordered; same rationale as
/// [`GameFactsDoc::units`] above). The COMMITTED files happen to be numeric-ascending instead —
/// an artifact of the old JS generator's `Object.entries()` over an all-numeric-key object (the
/// JS spec enumerates integer-index-like keys in ascending numeric order); cosmetic only, never
/// asserted by the shape-parity tests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameMapDoc {
    pub provenance: NameMapProvenance,
    pub map: BTreeMap<String, String>,
    #[serde(rename = "internalNames")]
    pub internal_names: BTreeMap<String, String>,
}

/// `unit-names.json` / `tech-names.json`'s `provenance` block — deliberately just three fields
/// (unlike [`Source`]'s five): matches the committed files exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameMapProvenance {
    pub source: String,
    pub sha: String,
    pub note: String,
}

// --- icon-map.json ---------------------------------------------------------------------------

/// The whole `icon-map.json` document — four independently-sorted slug -> `/images/aoe2/...`
/// path maps. Produced by [`crate::icon_map::build`]. `BTreeMap`s here are NOT just "order
/// doesn't matter" (as above): `scripts/build-icon-map.mjs` explicitly `sortObject`s each bucket
/// alphabetically before writing, so `BTreeMap`'s natural order is a genuine, faithful match to
/// the committed file's actual byte order, not merely a shape-compatible substitute.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IconMapDoc {
    pub units: BTreeMap<String, String>,
    pub techs: BTreeMap<String, String>,
    pub buildings: BTreeMap<String, String>,
    pub civs: BTreeMap<String, String>,
}

// --- civilizations.json ------------------------------------------------------------------------

/// The whole `src/data/civilizations.json` document. Produced by [`crate::civilizations::build`].
/// Field order matches the committed file's own key order exactly (verified against
/// `src/data/civilizations.json` in `tests/shape_parity.rs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CivilizationsDoc {
    pub patch: String,
    pub civs: Vec<CivEntry>,
}

/// One civ's EN-only data-layer entry — everything `scripts/build-civilizations.mjs`'s thin YAML
/// wrapper needs beyond the bilingual [`CivHelpStringsDoc`] (name/tagline/strategy stay hand-authored
/// or templated JS-side; see that crate module's doc for the full wrapper contract).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CivEntry {
    pub slug: String,
    pub region: String,
    pub specialty: String,
    #[serde(rename = "uniqueUnits")]
    pub unique_units: Vec<String>,
    #[serde(rename = "civBonuses")]
    pub civ_bonuses: Vec<String>,
    #[serde(rename = "teamBonus")]
    pub team_bonus: String,
    #[serde(rename = "uniqueTechs")]
    pub unique_techs: CivUniqueTechs,
    #[serde(rename = "regionNoun")]
    pub region_noun: String,
}

/// A civ's two unique techs — always exactly Castle + Imperial (every real AoE2 civ has both).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CivUniqueTechs {
    pub castle: CivUniqueTech,
    pub imperial: CivUniqueTech,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CivUniqueTech {
    pub name: String,
    pub effect: String,
}

// --- civ-help-strings.json (intermediate — NOT a committed src/data/ file) ----------------------

/// The whole intermediate `civ-help-strings.json` document [`crate::civilizations::build`] also
/// produces — the bilingual half of a civ's parsed help text that [`CivilizationsDoc`] (EN-only)
/// doesn't carry, keyed by content SLUG (matching `src/content/civilizations/<slug>.yaml`, unlike
/// [`crate::civs::RawCiv`]'s aoe2techtree-internal-name keying) so the JS YAML wrapper can look a
/// civ up directly by filename. See `crate::civilizations`'s module doc for the full wrapper
/// contract (which fields the wrapper is expected to use vs. carried for reference only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CivHelpStringsDoc {
    pub provenance: NameMapProvenance,
    pub civs: BTreeMap<String, CivHelpEntry>,
}

/// One civ's bilingual parsed help text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CivHelpEntry {
    pub en: CivHelpLang,
    pub tr: CivHelpLang,
}

/// One civ's help text parsed in ONE language. `civType`/`civBonuses`/`teamBonus` are the EN's
/// direct source for [`CivEntry`]'s `specialty`/`civBonuses`/`teamBonus` (so the `en` block here is
/// carried for reference/debugging — the wrapper already has those, pre-derived, in
/// `civilizations.json`); the `tr` block's `civBonuses`/`teamBonus`/`civType` are the ONLY place
/// those TR strings exist and are what the wrapper is actually expected to consume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CivHelpLang {
    #[serde(rename = "civType")]
    pub civ_type: String,
    #[serde(rename = "civBonuses")]
    pub civ_bonuses: Vec<String>,
    #[serde(rename = "teamBonus")]
    pub team_bonus: String,
    /// Always exactly `[castle, imperial]` — see [`crate::civ_help::require_parsed`]. `name` here
    /// is the (possibly-translated) name straight from THIS language's help text — the wrapper
    /// must NOT use `tr.uniqueTechs[*].name` for the YAML's `tr` name field: unique-tech proper
    /// nouns are allow-listed to stay EN-only site-wide (`audit-yaml-translations`'s `name`/`term`/
    /// unique-tech-name allow-list — see the repo's `CLAUDE.md`), so the wrapper must always take
    /// the name from `civilizations.json`'s (EN) `uniqueTechs` for BOTH languages, using only
    /// `effect` from here.
    #[serde(rename = "uniqueTechs")]
    pub unique_techs: Vec<CivUniqueTech>,
    #[serde(rename = "uniqueUnitNames")]
    pub unique_unit_names: Vec<String>,
}

// --- unit-descriptions.json (intermediate — NOT a committed src/data/ file) ---------------------

/// The whole intermediate `unit-descriptions.json` document [`crate::unit_help::build`] produces —
/// parsed bilingual `description`/`upgrades` text per canonical unit, keyed by content slug (the
/// `src/content/units/<slug>.yaml` filename). See that module's doc for the wrapper contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitDescriptionsDoc {
    pub provenance: NameMapProvenance,
    pub units: BTreeMap<String, UnitDescriptionEntry>,
}

/// One unit's parsed bilingual description/upgrades text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitDescriptionEntry {
    pub description: LocalizedString,
    /// `None` (JSON `null`) when the unit's help string has no `<i>Upgrades: ...</i>` line at all
    /// (e.g. Fishing Ship) — mirrors the YAML schema's `upgrades` being an OPTIONAL field, never an
    /// empty-string placeholder.
    pub upgrades: Option<LocalizedString>,
}

/// A bilingual string pair — the exact `{ en, tr }` shape `src/content/config.ts`'s
/// `localizedString` Zod schema (and every `{en,tr}` YAML field) expects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalizedString {
    pub en: String,
    pub tr: String,
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

    #[test]
    fn name_map_doc_round_trips_a_minimal_fixture() {
        let json = r#"{
            "provenance": { "source": "x", "sha": "y", "note": "z" },
            "map": { "4": "Archer" },
            "internalNames": { "4": "ARCHR" }
        }"#;
        let doc: NameMapDoc = serde_json::from_str(json).expect("must parse");
        assert_eq!(doc.map.get("4"), Some(&"Archer".to_string()));
        assert_eq!(doc.internal_names.get("4"), Some(&"ARCHR".to_string()));
    }

    #[test]
    fn icon_map_doc_round_trips_a_minimal_fixture() {
        let json = r#"{
            "units": { "archer": "/images/aoe2/Unit/17.png" },
            "techs": {},
            "buildings": {},
            "civs": { "britons": "/images/aoe2/Civs/Britons.png" }
        }"#;
        let doc: IconMapDoc = serde_json::from_str(json).expect("must parse");
        assert_eq!(doc.units.get("archer"), Some(&"/images/aoe2/Unit/17.png".to_string()));
    }

    #[test]
    fn civilizations_doc_round_trips_a_minimal_fixture_with_camel_case_keys() {
        let json = r#"{
            "patch": "v100.1.84",
            "civs": [{
                "slug": "britons",
                "region": "Western European",
                "specialty": "Foot Archer",
                "uniqueUnits": ["longbowman"],
                "civBonuses": ["Shepherds work +25% faster"],
                "teamBonus": "Archery Ranges work +10% faster",
                "uniqueTechs": {
                    "castle": { "name": "Yeomen", "effect": "x" },
                    "imperial": { "name": "Warwolf", "effect": "y" }
                },
                "regionNoun": "Western Europe"
            }]
        }"#;
        let doc: CivilizationsDoc = serde_json::from_str(json).expect("must parse");
        assert_eq!(doc.civs[0].unique_units, vec!["longbowman".to_string()]);
        assert_eq!(doc.civs[0].unique_techs.castle.name, "Yeomen");
    }

    #[test]
    fn civ_help_strings_doc_round_trips_a_minimal_fixture_with_camel_case_keys() {
        let json = r#"{
            "provenance": { "source": "x", "sha": "y", "note": "z" },
            "civs": {
                "britons": {
                    "en": {
                        "civType": "Foot Archer", "civBonuses": [], "teamBonus": "",
                        "uniqueTechs": [], "uniqueUnitNames": []
                    },
                    "tr": {
                        "civType": "Yaya Okçu", "civBonuses": [], "teamBonus": "",
                        "uniqueTechs": [], "uniqueUnitNames": []
                    }
                }
            }
        }"#;
        let doc: CivHelpStringsDoc = serde_json::from_str(json).expect("must parse");
        assert_eq!(doc.civs["britons"].tr.civ_type, "Yaya Okçu");
    }

    #[test]
    fn unit_descriptions_doc_round_trips_a_minimal_fixture() {
        let json = r#"{
            "provenance": { "source": "x", "sha": "y", "note": "z" },
            "units": {
                "archer": {
                    "description": { "en": "a", "tr": "b" },
                    "upgrades": { "en": "c", "tr": "d" }
                },
                "fishing-ship": {
                    "description": { "en": "e", "tr": "f" },
                    "upgrades": null
                }
            }
        }"#;
        let doc: UnitDescriptionsDoc = serde_json::from_str(json).expect("must parse");
        assert_eq!(doc.units["archer"].upgrades.as_ref().unwrap().en, "c");
        assert!(doc.units["fishing-ship"].upgrades.is_none());
    }
}
