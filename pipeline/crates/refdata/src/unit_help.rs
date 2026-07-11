//! Builds the intermediate `unit-descriptions.json` — parsed bilingual `description`/`upgrades`
//! text for each of the 90 [`crate::canonical_units::CANONICAL_UNITS`] — a strict Rust replacement
//! for the aoe2techtree-PARSING half of `scripts/build-unit-descriptions.mjs`.
//!
//! **Reuses [`crate::techtree::TechTree::resolve`] instead of re-implementing that script's own
//! candidate-resolution algorithm.** `build-unit-descriptions.mjs` groups `data.Unit` entries by
//! their SLUGIFIED help-string bold name and requires byte-identical help text across a group
//! before picking the lowest id; [`crate::techtree`] groups by the EXACT bold name and requires
//! stat-identity instead — a different grouping key and a different identity check. Verified (task
//! research, cross-checked against the real committed data) that for all 90 canonical units both
//! algorithms pick the exact same `language_name_id`, so reusing `techtree::resolve` here is a
//! faithful simplification, not a divergence — and it's the SAME resolved unit
//! [`crate::unit_stats`] already uses for its stats, so a unit's description and its stats are
//! always drawn from the identical source row. Per this crate's "reuse the shared parser, don't
//! write a second one" rule.
//!
//! **The YAML-writing half stays JS.** `scripts/build-unit-descriptions.mjs` becomes a thin
//! wrapper: for each `src/content/units/<slug>.yaml`, look up `slug` in `unit-descriptions.json`
//! and rewrite ONLY that file's `description`/`upgrades` fields (`slug`/`name`/`role`/`civ`/`line`/
//! `lineRank` pass through untouched, exactly as today) — `upgrades` is OMITTED from the YAML
//! entirely when [`UnitDescriptionEntry::upgrades`] is `None` (mirrors the current wrapper's
//! `...(en.upgrades !== null && { upgrades: ... })` spread).

use std::collections::{BTreeMap, HashMap};

use crate::canonical_units::CANONICAL_UNITS;
use crate::civ_help::Lang;
use crate::error::{RefdataError, Result};
use crate::model::{LocalizedString, NameMapProvenance, UnitDescriptionEntry, UnitDescriptionsDoc};
use crate::techtree::{self, TechTree, HELP_STRING_OFFSET};

/// `unit-descriptions.json`'s `provenance.note`.
const PROVENANCE_NOTE: &str = "Bilingual (EN+TR) unit description + upgrades text, parsed from \
aoe2techtree's per-unit 'help' strings (data/strings-en.json + data/strings-tr.json, keyed by \
LanguageNameId + 21000) with the same <b>name</b>/<br>body/<i>Upgrades: ...</i> extraction as the \
old scripts/build-unit-descriptions.mjs. Intermediate output — NOT a committed src/data/ file; \
consumed by build-unit-descriptions.mjs's thin YAML wrapper, which rewrites ONLY the description/ \
upgrades fields of each src/content/units/<slug>.yaml (see pipeline/crates/refdata/src/unit_help.rs's \
module doc for the exact wrapper contract).";

/// One unit's parsed help text in ONE language.
#[derive(Debug, PartialEq, Eq)]
struct UnitHelpText {
    body: String,
    upgrades: Option<String>,
}

/// Builds `unit-descriptions.json` from the committed reference data baked into this binary at
/// compile time — the entry point `main.rs` calls.
pub fn build_from_committed_reference_data() -> Result<UnitDescriptionsDoc> {
    build(&techtree::load()?, &techtree::load_help_strings_en()?, &techtree::load_help_strings_tr()?)
}

/// Builds the whole document from already-parsed sources. Pure function of its inputs — no
/// filesystem access — so tests can drive it with inline fixtures.
pub fn build(
    tech_tree: &TechTree,
    help_en: &HashMap<String, String>,
    help_tr: &HashMap<String, String>,
) -> Result<UnitDescriptionsDoc> {
    let mut units = BTreeMap::new();
    for &(display_name, slug) in CANONICAL_UNITS {
        let unit = tech_tree.resolve(display_name, slug)?;
        let help_id = (unit.language_name_id + HELP_STRING_OFFSET).to_string();

        let raw_en = help_en.get(&help_id).ok_or(RefdataError::UnitHelpStringMissing {
            slug,
            language_name_id: unit.language_name_id,
            lang: "EN",
        })?;
        let raw_tr = help_tr.get(&help_id).ok_or(RefdataError::UnitHelpStringMissing {
            slug,
            language_name_id: unit.language_name_id,
            lang: "TR",
        })?;

        let en = parse_unit_help(raw_en, Lang::En, slug)?;
        let tr = parse_unit_help(raw_tr, Lang::Tr, slug)?;
        if en.upgrades.is_some() != tr.upgrades.is_some() {
            return Err(RefdataError::UnitHelpUpgradesLangMismatch { slug });
        }

        let description = LocalizedString { en: en.body, tr: tr.body };
        let upgrades = match (en.upgrades, tr.upgrades) {
            (Some(en_upgrades), Some(tr_upgrades)) => {
                Some(LocalizedString { en: en_upgrades, tr: tr_upgrades })
            }
            _ => None,
        };
        units.insert(slug.to_string(), UnitDescriptionEntry { description, upgrades });
    }

    Ok(UnitDescriptionsDoc {
        provenance: NameMapProvenance {
            source: "SiegeEngineers/aoe2techtree".to_string(),
            sha: crate::AOE2TECHTREE_SHA.to_string(),
            note: PROVENANCE_NOTE.to_string(),
        },
        units,
    })
}

/// Parses one unit's raw help string into `{ body, upgrades }` — a regex-free port of
/// `build-unit-descriptions.mjs`'s own inline `parseHelp`:
/// `"Create <b>Name</b> (‹cost›)<br>\n{body}<br>\n<i>Upgrades: ...</i>‹DEFAULT›..."`.
fn parse_unit_help(raw: &str, lang: Lang, slug: &'static str) -> Result<UnitHelpText> {
    // `raw.split("‹DEFAULT›")[0]` — everything before the game's per-difficulty-stat marker. Must
    // split on the WHOLE literal `"‹DEFAULT›"` string, not the bare `‹` character alone — the help
    // text also uses that same guillemet character for OTHER placeholders earlier in the string
    // (`‹cost›`, `‹hp›`, `‹attack›`, ...), which a single-character split would wrongly cut at.
    let cut = raw.split("‹DEFAULT›").next().unwrap_or(raw);

    if techtree::bold_span(cut).is_none() {
        return Err(RefdataError::UnitHelpNoBoldName { slug, lang: lang.label() });
    }
    let Some(first_br) = cut.find("<br>") else {
        return Err(RefdataError::UnitHelpNoBreakAfterTitle { slug, lang: lang.label() });
    };
    let mut rest = &cut[first_br + "<br>".len()..];

    let mut upgrades = None;
    if let Some((italic_start, italic_content)) = first_italic_span(rest) {
        let stripped = strip_tags(italic_content);
        let label = lang.upgrades_label();
        if !stripped.starts_with(label) {
            return Err(RefdataError::UnitHelpUpgradesLabelMismatch { slug, lang: lang.label(), label });
        }
        upgrades = Some(stripped[label.len()..].trim().to_string());
        rest = &rest[..italic_start];
    }

    let body = strip_tags(rest);
    if body.is_empty() {
        return Err(RefdataError::UnitHelpEmptyBody { slug, lang: lang.label() });
    }

    Ok(UnitHelpText { body, upgrades })
}

/// Finds the first `<i>...</i>` span, returning `(byte offset of the opening '<i>', inner
/// content)` — the offset is needed (unlike [`techtree::bold_span`], which only returns content)
/// because the caller must truncate `rest` to everything BEFORE the upgrades line.
fn first_italic_span(s: &str) -> Option<(usize, &str)> {
    let start = s.find("<i>")?;
    let content_start = start + "<i>".len();
    let end = s[content_start..].find("</i>")? + content_start;
    Some((start, &s[content_start..end]))
}

/// Strips ANY `<...>` tag (content unrestricted — a regex-free port of `build-unit-descriptions.mjs`'s
/// own `stripTags`'s `/<[^>]+>/g`, more permissive than [`crate::civ_help`]'s letters-only tag
/// stripper, which ports a DIFFERENT source regex for a different help-string family). Also folds
/// literal `"<br>"` into a space first, then collapses all whitespace runs to one space and trims —
/// `str::split_whitespace().join(" ")` is exactly `.replace(/\s+/g, " ").trim()`.
fn strip_tags(s: &str) -> String {
    let with_br_as_space = s.replace("<br>", " ");
    let no_tags = strip_any_tags(&with_br_as_space);
    no_tags.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Strips `<...>` where the bracketed content is non-empty (mirrors `[^>]+`'s `>=1` requirement —
/// a bare `"<>"` is left untouched, matching the source regex exactly). `'<'`/`'>'` are both
/// single-byte ASCII, so every index this scans to is a valid UTF-8 char boundary regardless of
/// any non-ASCII bytes elsewhere in `s` (the tag CONTENT itself may be anything, incl. multi-byte
/// chars — only the two delimiters need to be found, via [`str::find`], which is boundary-safe).
fn strip_any_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let after_lt = &rest[lt + 1..];
        match after_lt.find('>') {
            Some(gt) if gt > 0 => rest = &after_lt[gt + 1..], // non-empty content — skip whole tag
            _ => {
                out.push('<');
                rest = after_lt;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARCHER_EN: &str = "Create <b>Archer</b> (‹cost›)<br>\n\
        All-purpose Foot Archer. Strong vs. units at long range. Weak vs. Skirmishers, \
        Mangonel-line and units at close range.<br>\n\
        <i>Upgrades: attack, range, armor (Blacksmith); attack, accuracy (University); accuracy, \
        to Crossbowman (Archery Range); creation speed (Castle); more resistant to Monks \
        (Monastery).</i>\u{2039}DEFAULT\u{203a}<br>\n\
        \u{2039}hp\u{203a} \u{2039}attack\u{203a} \u{2039}armor\u{203a} \u{2039}piercearmor\u{203a} \u{2039}range\u{203a}";

    #[test]
    fn parses_the_real_archer_help_string() {
        let out = parse_unit_help(ARCHER_EN, Lang::En, "archer").unwrap();
        assert_eq!(
            out.body,
            "All-purpose Foot Archer. Strong vs. units at long range. Weak vs. Skirmishers, \
             Mangonel-line and units at close range."
        );
        assert_eq!(
            out.upgrades.as_deref(),
            Some(
                "attack, range, armor (Blacksmith); attack, accuracy (University); accuracy, to \
                 Crossbowman (Archery Range); creation speed (Castle); more resistant to Monks \
                 (Monastery)."
            )
        );
    }

    #[test]
    fn no_bold_name_fails_loud() {
        match parse_unit_help("No bold here<br>\nbody", Lang::En, "ghost").unwrap_err() {
            RefdataError::UnitHelpNoBoldName { slug, .. } => assert_eq!(slug, "ghost"),
            other => panic!("expected UnitHelpNoBoldName, got {other:?}"),
        }
    }

    #[test]
    fn no_br_after_title_fails_loud() {
        match parse_unit_help("<b>Name</b> no break here", Lang::En, "ghost").unwrap_err() {
            RefdataError::UnitHelpNoBreakAfterTitle { slug, .. } => assert_eq!(slug, "ghost"),
            other => panic!("expected UnitHelpNoBreakAfterTitle, got {other:?}"),
        }
    }

    #[test]
    fn a_unit_with_no_upgrades_line_has_none() {
        let raw = "Create <b>Fishing Ship</b> (‹cost›)<br>\nGathers food from fish.\u{2039}DEFAULT\u{203a}";
        let out = parse_unit_help(raw, Lang::En, "fishing-ship").unwrap();
        assert_eq!(out.body, "Gathers food from fish.");
        assert_eq!(out.upgrades, None);
    }

    #[test]
    fn upgrades_label_mismatch_fails_loud() {
        let raw = "Create <b>X</b> (c)<br>\nbody<br>\n<i>Not the right label</i>";
        match parse_unit_help(raw, Lang::En, "x").unwrap_err() {
            RefdataError::UnitHelpUpgradesLabelMismatch { slug, .. } => assert_eq!(slug, "x"),
            other => panic!("expected UnitHelpUpgradesLabelMismatch, got {other:?}"),
        }
    }

    #[test]
    fn strip_tags_folds_br_to_space_and_collapses_whitespace() {
        assert_eq!(strip_tags("a<br>b<br>  c   d"), "a b c d");
        assert_eq!(strip_tags("<i>x</i> plain"), "x plain");
    }

    #[test]
    fn strip_any_tags_leaves_an_empty_bracket_pair_untouched() {
        assert_eq!(strip_any_tags("a<>b"), "a<>b");
        assert_eq!(strip_any_tags("a<x>b</x>c"), "abc");
    }

    #[test]
    fn real_reference_data_produces_ninety_units_matching_the_committed_yaml() {
        let doc = build_from_committed_reference_data()
            .expect("build must succeed against the real committed reference data");
        assert_eq!(doc.units.len(), 90);

        let archer = &doc.units["archer"];
        assert_eq!(
            archer.description.en,
            "All-purpose Foot Archer. Strong vs. units at long range. Weak vs. Skirmishers, \
             Mangonel-line and units at close range."
        );
        assert!(archer.upgrades.is_some());

        // Fishing Ship's help string DOES carry an <i>Upgrades: ...</i> line (Dock/University/
        // Monastery techs) — verified byte-identical to the committed fishing-ship.yaml `upgrades`.
        let fishing_ship = &doc.units["fishing-ship"];
        assert!(fishing_ship.upgrades.is_some());
    }
}
