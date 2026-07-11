//! Builds `src/data/civilizations.json` (EN-only data layer — [`CivilizationsDoc`]) AND the
//! intermediate `civ-help-strings.json` ([`CivHelpStringsDoc`], bilingual) from the committed
//! aoe2techtree civ slice ([`crate::civs`]) + help-text parser ([`crate::civ_help`]) + the small
//! hand-curated region/uniqueUnits tables ([`crate::civ_region`]) — a strict Rust replacement for
//! the aoe2techtree-PARSING half of `scripts/build-civilizations.mjs`.
//!
//! **The YAML-writing half stays JS** (per this task's split — Rust never writes Astro content
//! YAML, only data JSON): `scripts/build-civilizations.mjs` becomes a THIN wrapper that reads
//! these two JSON outputs instead of re-parsing aoe2techtree/aalises itself. The wrapper contract:
//!
//! 1. **`civilizations.json`** ([`CivilizationsDoc`]) is written straight to `src/data/`
//!    unchanged — it already matches the committed shape field-for-field.
//! 2. For each `src/content/civilizations/<slug>.yaml`, the wrapper looks up `slug` in BOTH
//!    outputs and builds the YAML frontmatter as:
//!    * `name.en`/`bonuses.en`/`teamBonus.en`/`uniqueTechs.*.name`/`uniqueTechs.*.effect.en` — from
//!      `civilizations.json`'s matching [`CivEntry`] (title-cased `slug` for `name.en`, exactly as
//!      today).
//!    * `bonuses.tr`/`teamBonus.tr`/`uniqueTechs.*.effect.tr` — from `civ-help-strings.json`'s
//!      `civs[slug].tr` block ([`CivHelpLang`]). **`uniqueTechs.*.name.tr` must be the SAME EN name**
//!      (never `civ-help-strings.json`'s `tr.uniqueTechs[*].name`) — unique-tech proper nouns are
//!      allow-listed EN-only site-wide (see [`crate::model::CivHelpLang`]'s doc).
//!    * `tagline.en`/`tagline.tr` — hand-templated JS-side from `displayName` + `region`/
//!      `regionNoun` (EN) and `civ-help-strings.json`'s `tr.civType` + a small TR region-noun table
//!      (Turkish ablative forms, e.g. `"Batı Avrupa'dan"`) — these stay JS-only editorial copy,
//!      exactly as the current file's header comment already says ("Only editorial copy remains
//!      hand-written: the EN/TR region labels and tagline templates"); NOT reproduced in Rust.
//!    * `name.tr`/`strategy` — carried forward from the EXISTING committed YAML file, unchanged
//!      (never regenerated) — same as today.
//! 3. `civ-help-strings.json`'s `en` block is redundant with `civilizations.json` and mainly for
//!    debugging; the wrapper is not expected to read it.

use std::collections::{BTreeMap, HashMap};

use crate::civ_help::{self, HelpParse, Lang};
use crate::civ_region;
use crate::civs::{self, RawCiv};
use crate::error::{RefdataError, Result};
use crate::model::{
    CivEntry, CivHelpEntry, CivHelpLang, CivHelpStringsDoc, CivUniqueTech, CivUniqueTechs,
    CivilizationsDoc, NameMapProvenance,
};

/// `civ-help-strings.json`'s `provenance.note`.
const PROVENANCE_NOTE: &str = "Bilingual (EN+TR) civ bonus/unique-tech/unique-unit/team-bonus text, \
parsed from aoe2techtree's per-civ 'help' strings (data/strings-en.json + data/strings-tr.json, \
keyed by civ.help_string_id) with the same section-marker logic as the old \
scripts/lib/parse-help.mjs. Intermediate output — NOT a committed src/data/ file; consumed by \
scripts/build-civilizations.mjs's thin YAML wrapper (see pipeline/crates/refdata/src/civilizations.rs's \
module doc for the exact wrapper contract). uniqueTechs[*].name here is the (possibly-translated) \
name from THIS language's help text; the wrapper must still use the EN name for BOTH languages' \
YAML output (unique-tech proper nouns are allow-listed EN-only site-wide) — only effect is sourced \
per-language.";

/// Builds both documents from the committed reference data baked into this binary at compile
/// time — the entry point `main.rs` calls.
pub fn build_from_committed_reference_data() -> Result<(CivilizationsDoc, CivHelpStringsDoc)> {
    build(&civs::load()?)
}

/// Builds both documents from an already-parsed civ table. Pure function of its input — no
/// filesystem access — so tests can drive it with small inline fixtures as well as the real
/// committed data.
pub fn build(civs_by_name: &HashMap<String, RawCiv>) -> Result<(CivilizationsDoc, CivHelpStringsDoc)> {
    // Sorted by content SLUG (not the aoe2techtree map's arbitrary key order) — matches
    // `build-civilizations.mjs`'s own `[...allCivSlugs].sort()` walk, which is why the committed
    // `civilizations.json`'s `civs` array is itself alphabetical by slug.
    let mut slugged: Vec<(String, &RawCiv)> =
        civs_by_name.iter().map(|(name, civ)| (civ_slug(name), civ)).collect();
    slugged.sort_by(|a, b| a.0.cmp(&b.0));

    let mut civ_entries = Vec::with_capacity(slugged.len());
    let mut help_entries = BTreeMap::new();

    for (slug, civ) in slugged {
        let en = civ_help::require_parsed(&civ.help_en, Lang::En, &slug, civ.help_string_id)?;
        let tr = civ_help::require_parsed(&civ.help_tr, Lang::Tr, &slug, civ.help_string_id)?;

        let region = civ_region::region_for(&slug)?.to_string();
        let region_noun = civ_region::region_noun_for(&region);
        let specialty = civ_region::fix_specialty(&en.civ_type);
        let unique_units = resolve_unique_units(&slug, &en.unique_unit_names)?;
        let unique_techs = to_civ_unique_techs(&en);

        civ_entries.push(CivEntry {
            slug: slug.clone(),
            region,
            specialty,
            unique_units,
            civ_bonuses: en.civ_bonuses.clone(),
            team_bonus: en.team_bonus.clone(),
            unique_techs,
            region_noun,
        });

        help_entries.insert(slug, CivHelpEntry { en: to_help_lang(&en), tr: to_help_lang(&tr) });
    }

    let civilizations = CivilizationsDoc {
        // Same literal the committed file already carries — see `unit_stats::PATCH`'s doc (this
        // crate's other "hand-set patch label" constant); the game data has no display-version
        // field, so both documents share the one hand-set label rather than duplicating it.
        patch: crate::unit_stats::PATCH.to_string(),
        civs: civ_entries,
    };
    let help_strings = CivHelpStringsDoc {
        provenance: NameMapProvenance {
            source: "SiegeEngineers/aoe2techtree".to_string(),
            sha: crate::AOE2TECHTREE_SHA.to_string(),
            note: PROVENANCE_NOTE.to_string(),
        },
        civs: help_entries,
    };
    Ok((civilizations, help_strings))
}

/// Indexes `[0]`/`[1]` directly — safe because every caller only ever passes a [`HelpParse`] that
/// has already been through [`civ_help::require_parsed`], which guarantees `unique_techs.len() ==
/// 2` (never fewer) before returning `Ok`.
fn to_civ_unique_techs(en: &HelpParse) -> CivUniqueTechs {
    CivUniqueTechs {
        castle: to_civ_unique_tech(&en.unique_techs[0]),
        imperial: to_civ_unique_tech(&en.unique_techs[1]),
    }
}

fn to_civ_unique_tech(t: &civ_help::UniqueTechText) -> CivUniqueTech {
    CivUniqueTech { name: t.name.clone(), effect: t.effect.clone() }
}

fn to_help_lang(parsed: &HelpParse) -> CivHelpLang {
    CivHelpLang {
        civ_type: parsed.civ_type.clone(),
        civ_bonuses: parsed.civ_bonuses.clone(),
        team_bonus: parsed.team_bonus.clone(),
        unique_techs: parsed.unique_techs.iter().map(to_civ_unique_tech).collect(),
        unique_unit_names: parsed.unique_unit_names.clone(),
    }
}

/// Resolves a civ's final `uniqueUnits` list: the 3 civs in
/// [`civ_region::UNIQUE_UNIT_OVERRIDE`] use that curated list verbatim; every other civ uses its
/// help-derived names, [`civ_slug`]'d and de-duplicated in order (mirrors
/// `build-civilizations.mjs`'s aalises∪help union, minus the now-unreachable aalises half — see
/// `civ_region`'s module doc for why that's safe against the current civ roster). An empty result
/// either way is a fail-loud [`crate::error::RefdataError::NoCivUniqueUnit`], never an empty list.
fn resolve_unique_units(slug: &str, help_names: &[String]) -> Result<Vec<String>> {
    if let Some(overridden) = civ_region::unique_unit_override_for(slug) {
        return Ok(overridden.iter().map(|s| s.to_string()).collect());
    }
    let mut units = Vec::new();
    for name in help_names {
        let unit_slug = civ_slug(name);
        if !unit_slug.is_empty() && !units.contains(&unit_slug) {
            units.push(unit_slug);
        }
    }
    if units.is_empty() {
        return Err(RefdataError::NoCivUniqueUnit { slug: slug.to_string() });
    }
    Ok(units)
}

/// Ports `build-civilizations.mjs`'s (one and only) `slugify`: lowercase, drop ASCII apostrophes,
/// collapse every run of remaining non-`[a-z0-9]` characters to a single hyphen, trim leading/
/// trailing hyphens. Used for BOTH civ names (`"Hindustanis"` -> `"hindustanis"`) AND unique-unit
/// display names (`"Elephant Archer"` -> `"elephant-archer"`) — the source script reuses the same
/// function for both, so this does too.
pub(crate) fn civ_slug(name: &str) -> String {
    let no_apostrophes: String = name.chars().filter(|&c| c != '\'').collect();
    let lower = no_apostrophes.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut in_run = false;
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            in_run = false;
        } else if !in_run {
            out.push('-');
            in_run = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civ_slug_lowercases_and_hyphenates() {
        assert_eq!(civ_slug("Hindustanis"), "hindustanis");
        assert_eq!(civ_slug("Elephant Archer"), "elephant-archer");
        assert_eq!(civ_slug("Imperial Camel Rider"), "imperial-camel-rider");
    }

    #[test]
    fn civ_slug_strips_apostrophes_and_trims_hyphens() {
        assert_eq!(civ_slug("  O'Brien's "), "obriens");
    }

    #[test]
    fn real_reference_data_produces_fifty_three_civs_in_slug_order() {
        let (civilizations, help) = build_from_committed_reference_data()
            .expect("build must succeed against the real committed reference data");
        assert_eq!(civilizations.civs.len(), 53);
        assert_eq!(help.civs.len(), 53);
        assert_eq!(civilizations.patch, crate::unit_stats::PATCH);

        let slugs: Vec<&str> = civilizations.civs.iter().map(|c| c.slug.as_str()).collect();
        let mut sorted = slugs.clone();
        sorted.sort_unstable();
        assert_eq!(slugs, sorted, "civs array must be slug-sorted, matching the committed file");
    }

    #[test]
    fn britons_matches_the_committed_civilizations_json_entry() {
        let (civilizations, help) = build_from_committed_reference_data().unwrap();
        let britons = civilizations.civs.iter().find(|c| c.slug == "britons").unwrap();
        assert_eq!(britons.region, "Western European");
        assert_eq!(britons.region_noun, "Western Europe");
        assert_eq!(britons.specialty, "Foot Archer");
        assert_eq!(britons.unique_units, vec!["longbowman"]);
        assert_eq!(
            britons.civ_bonuses,
            vec![
                "Shepherds work +25% faster",
                "Town Centers cost -50% wood starting in Castle Age",
                "Foot Archers +1/+2 range in Castle/Imperial Age",
            ]
        );
        assert_eq!(britons.team_bonus, "Archery Ranges work +10% faster");
        assert_eq!(britons.unique_techs.castle.name, "Yeomen");
        assert_eq!(
            britons.unique_techs.castle.effect,
            "Foot Archers and Skirmisher-line +1 range; Watch Tower-line +2 attack"
        );
        assert_eq!(britons.unique_techs.imperial.name, "Warwolf");

        let britons_help = &help.civs["britons"];
        assert_eq!(britons_help.tr.civ_type, "Yaya Okçu");
        assert!(!britons_help.tr.civ_bonuses.is_empty());
        assert_eq!(britons_help.tr.civ_bonuses.len(), britons.civ_bonuses.len());
    }

    #[test]
    fn incas_and_hindustanis_and_portuguese_use_the_curated_override() {
        let (civilizations, _) = build_from_committed_reference_data().unwrap();
        let by_slug = |slug: &str| {
            civilizations.civs.iter().find(|c| c.slug == slug).unwrap().unique_units.clone()
        };
        assert_eq!(by_slug("incas"), vec!["kamayuk", "slinger"]);
        assert_eq!(by_slug("hindustanis"), vec!["elephant-archer", "imperial-camel-rider", "ghulam"]);
        assert_eq!(by_slug("portuguese"), vec!["caravel", "organ-gun"]);
    }

    #[test]
    fn armenians_imperial_tech_reproduces_the_committed_nested_paren_quirk() {
        // Armenians' imperial tech effect text nests a parenthetical, which defeats the
        // "trailing (...)" split exactly like the source .mjs's own regex (see civ_help's doc) —
        // this crate must reproduce that byte-for-byte, not "fix" it.
        let (civilizations, _) = build_from_committed_reference_data().unwrap();
        let armenians = civilizations.civs.iter().find(|c| c.slug == "armenians").unwrap();
        assert_eq!(
            armenians.unique_techs.imperial.name,
            "Fereters (Infantry (except Spearman-line) +30 HP; Warrior Priests heal +100% faster)"
        );
        assert_eq!(armenians.unique_techs.imperial.effect, "");
    }
}
