//! Three small, hand-curated tables that replace `scripts/build-civilizations.mjs`'s dependency on
//! the aalises `civilizations.csv` for everything BEYOND what aoe2techtree's own data carries.
//! aoe2techtree's `data.json` civ entries have no expansion/region/army-type metadata at all (see
//! this crate's task report), so a civ's `region` has always ultimately come from a hand-curated
//! map in the JS generator (`REGION_OVERRIDE`), not a genuinely-parsed CSV column: that table
//! already covers all 53 real civs directly (`REGION_MAP`, the `expansion` CSV-column fallback it
//! sits in front of, is DEAD CODE against the current civ roster — verified by diffing
//! `REGION_OVERRIDE`'s keys against every `aoe2techtree-civs.json` civ), so only the override half
//! is ported here, as a genuinely-static Rust table instead of a runtime CSV read.
//!
//! [`UNIQUE_UNIT_OVERRIDE`] plays the same role for the one other place `civilizations.csv` fed
//! real information the aoe2techtree help text doesn't carry: 3 of 53 civs' help strings omit or
//! reorder a unique unit the CSV had right (Incas' Slinger, Hindustanis' Elephant Archer, and
//! Portuguese's Caravel/Organ-Gun order) — verified by diffing every civ's help-derived unique-unit
//! list against the committed `civilizations.json` (see the task report). Everywhere else (50/53
//! civs), the help-derived list is already byte-identical to committed, so no CSV parsing is
//! needed — just this tiny, fully-enumerated exception list.

use crate::error::{RefdataError, Result};

/// Civ slug -> region label. Mirrors `build-civilizations.mjs`'s `REGION_OVERRIDE` verbatim (the
/// `REGION_MAP`/`expansion` fallback it sat in front of is intentionally NOT ported — see the
/// module doc for why it's unreachable against the current civ roster).
pub const REGION_OVERRIDE: &[(&str, &str)] = &[
    ("aztecs", "Mesoamerican"),
    ("mayans", "Mesoamerican"),
    ("incas", "South American"),
    ("huns", "Central Asian"),
    ("mongols", "East Asian"),
    ("chinese", "East Asian"),
    ("japanese", "East Asian"),
    ("koreans", "East Asian"),
    ("byzantines", "Eastern Mediterranean"),
    ("persians", "Middle Eastern"),
    ("saracens", "Middle Eastern"),
    ("turks", "Middle Eastern"),
    ("teutons", "Central European"),
    ("celts", "Western European"),
    ("franks", "Western European"),
    ("britons", "Western European"),
    ("vikings", "Northern European"),
    ("goths", "Northern European"),
    ("slavs", "Eastern European"),
    ("bulgarians", "Eastern European"),
    ("bohemians", "Eastern European"),
    ("poles", "Eastern European"),
    ("lithuanians", "Eastern European"),
    ("cumans", "Central Asian"),
    ("tatars", "Central Asian"),
    ("berbers", "North African"),
    ("malians", "West African"),
    ("ethiopians", "East African"),
    ("malay", "Southeast Asian"),
    ("burmese", "Southeast Asian"),
    ("khmer", "Southeast Asian"),
    ("vietnamese", "Southeast Asian"),
    ("italians", "Southern European"),
    ("spanish", "Southern European"),
    ("portuguese", "Southern European"),
    ("sicilians", "Southern European"),
    ("burgundians", "Western European"),
    ("magyars", "Eastern European"),
    ("hindustanis", "South Asian"),
    ("dravidians", "South Asian"),
    ("bengalis", "South Asian"),
    ("gurjaras", "South Asian"),
    ("georgians", "Caucasian"),
    ("armenians", "Caucasian"),
    ("mapuche", "South American"),
    ("romans", "Ancient Mediterranean"),
    ("shu", "East Asian"),
    ("wei", "East Asian"),
    ("wu", "East Asian"),
    ("jurchens", "East Asian"),
    ("khitans", "East Asian"),
    ("tupi", "South American"),
    ("muisca", "South American"),
];

/// Looks up `slug`'s region, or [`RefdataError::MissingCivRegion`] — every real committed civ must
/// have a row here (see the module doc); a gap means the aoe2techtree civ roster grew and this
/// table needs a new entry, never a fabricated region.
pub fn region_for(slug: &str) -> Result<&'static str> {
    REGION_OVERRIDE
        .iter()
        .find(|(s, _)| *s == slug)
        .map(|(_, region)| *region)
        .ok_or_else(|| RefdataError::MissingCivRegion { slug: slug.to_string() })
}

/// Region label -> EN noun phrase for tagline prose (`civilizations.json`'s `regionNoun` field).
/// Mirrors `build-civilizations.mjs`'s `REGION_NOUN` verbatim. A region absent here (shouldn't
/// happen — every [`REGION_OVERRIDE`] value has a row) falls back to the region label itself,
/// exactly like the source's `REGION_NOUN[entry.region] ?? entry.region`.
pub const REGION_NOUN: &[(&str, &str)] = &[
    ("Ancient Mediterranean", "the Ancient Mediterranean"),
    ("Ancient Middle Eastern", "the Ancient Middle East"),
    ("Caucasian", "the Caucasus"),
    ("Central Asian", "Central Asia"),
    ("Central European", "Central Europe"),
    ("East African", "East Africa"),
    ("East Asian", "East Asia"),
    ("Eastern European", "Eastern Europe"),
    ("Eastern Mediterranean", "the Eastern Mediterranean"),
    ("Mesoamerican", "Mesoamerica"),
    ("Middle Eastern", "the Middle East"),
    ("North African", "North Africa"),
    ("Northern European", "Northern Europe"),
    ("South American", "South America"),
    ("South Asian", "South Asia"),
    ("Southeast Asian", "Southeast Asia"),
    ("Southern European", "Southern Europe"),
    ("West African", "West Africa"),
    ("Western European", "Western Europe"),
];

/// `region`'s noun phrase, or `region` itself unchanged if this table has no row for it (matches
/// `?? entry.region` — never fails loud; `regionNoun` is presentation-only, not a source-of-truth
/// fact worth stopping the build over).
pub fn region_noun_for(region: &str) -> String {
    REGION_NOUN
        .iter()
        .find(|(r, _)| *r == region)
        .map(|(_, noun)| noun.to_string())
        .unwrap_or_else(|| region.to_string())
}

/// Civ slug -> its FULL `uniqueUnits` list, for the 3 civs (of 53) whose committed
/// `civilizations.json` value the help-string-derived list alone can't reproduce — see the module
/// doc. Order matters (matches the committed file exactly): Incas/Hindustanis need one extra unit
/// the help text omits entirely (Slinger / Elephant Archer, both real aalises-sourced facts — the
/// in-game help string is simply incomplete for these two); Portuguese's help text lists its two
/// units in the OPPOSITE order the committed file uses. Every other civ's help-derived list is
/// already byte-identical to committed (verified civ-by-civ against `src/data/civilizations.json`
/// in the task's research — see the task report), so this list is deliberately exhaustive, not a
/// fallback pattern to extend casually: a new civ needing an entry here means its help text is
/// ALSO missing/reordering something real, worth a comment explaining why, same as these three.
pub const UNIQUE_UNIT_OVERRIDE: &[(&str, &[&str])] = &[
    ("incas", &["kamayuk", "slinger"]),
    ("hindustanis", &["elephant-archer", "imperial-camel-rider", "ghulam"]),
    ("portuguese", &["caravel", "organ-gun"]),
];

/// Looks up `slug`'s override list, if any.
pub fn unique_unit_override_for(slug: &str) -> Option<&'static [&'static str]> {
    UNIQUE_UNIT_OVERRIDE.iter().find(|(s, _)| *s == slug).map(|(_, units)| *units)
}

/// Ports `build-civilizations.mjs`'s `fixSpecialty`: cleans up a civ's raw parsed `civType` into
/// the `specialty` field. On the current 53 real civ-type values this is close to a no-op (no
/// source string currently carries the `Calvary`/`Civilzation` typos or an `Archers` plural this
/// cleans up — verified against the whole committed slice), but every rule is ported for fidelity
/// with the source script, not just the ones presently exercised.
pub fn fix_specialty(raw: &str) -> String {
    let no_typos = replace_ci(raw, "Calvary", "Cavalry");
    let no_civilzation = remove_word_ci(&no_typos, "Civilzation");
    let trimmed = no_civilzation.trim();

    let joined = split_ci(trimmed, " and ")
        .map(title_case_words)
        .collect::<Vec<_>>()
        .join(" and ");

    let singular_archers = replace_whole_word(&joined, "Archers", "Archer");
    if singular_archers == "Cavalry Infantry" {
        "Cavalry and Infantry".to_string()
    } else {
        singular_archers
    }
}

/// `w.charAt(0).toUpperCase() + w.slice(1).toLowerCase()`, applied to each whitespace-separated
/// word (hyphens are NOT a split point — see the doc, this is load-bearing: e.g. Mapuche's
/// `"Counter-Units"` title-cases to `"Counter-units"`, matching the committed file exactly, because
/// the JS only lowercases the tail of the WHOLE whitespace-delimited token, hyphen included).
fn title_case_words(s: &str) -> String {
    s.split_whitespace().map(title_case_word).collect::<Vec<_>>().join(" ")
}

fn title_case_word(w: &str) -> String {
    let mut chars = w.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out: String = first.to_uppercase().collect();
    out.push_str(&chars.as_str().to_lowercase());
    out
}

/// Case-insensitive substring replace-all — a regex-free port of `.replace(/needle/gi, repl)`
/// (no word-boundary anchoring, matching the source's plain `/gi` flags). `needle` is always plain
/// ASCII in every caller (`"Calvary"`), so ASCII-only case-folding is both sufficient AND, unlike
/// full Unicode `to_lowercase()`, guaranteed to preserve `s`'s byte length exactly — the indices
/// found in the lowered copy stay valid byte offsets into the ORIGINAL string even if `s` itself
/// carries non-ASCII bytes elsewhere.
fn replace_ci(s: &str, needle: &str, repl: &str) -> String {
    if needle.is_empty() {
        return s.to_string();
    }
    let lower = s.to_ascii_lowercase();
    let needle_lower = needle.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    let mut rest_lower = lower.as_str();
    while let Some(idx) = rest_lower.find(&needle_lower) {
        out.push_str(&rest[..idx]);
        out.push_str(repl);
        rest = &rest[idx + needle.len()..];
        rest_lower = &rest_lower[idx + needle.len()..];
    }
    out.push_str(rest);
    out
}

/// Removes a whole word (ASCII `\b`-bounded) case-insensitively, along with any immediately
/// preceding whitespace — a regex-free port of `.replace(/\s*\bWORD\b/gi, "")`. Same ASCII-only
/// case-folding rationale as [`replace_ci`] (`word` is always plain ASCII).
fn remove_word_ci(s: &str, word: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let word_lower = word.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let bytes = s.as_bytes();
    let lower_bytes = lower.as_bytes();
    while i < bytes.len() {
        if lower_bytes[i..].starts_with(word_lower.as_bytes())
            && is_word_boundary(bytes, i)
            && is_word_boundary(bytes, i + word.len())
        {
            // Drop any whitespace already pushed onto `out` immediately before this word.
            let trimmed_len = out.trim_end_matches(|c: char| c.is_whitespace()).len();
            out.truncate(trimmed_len);
            i += word.len();
            continue;
        }
        // Push exactly one char (handles multi-byte UTF-8 correctly).
        let ch = s[i..].chars().next().expect("i is a valid char boundary");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// A regex-free port of `\b` at byte offset `idx`: true iff the byte immediately before `idx`
/// and the byte immediately at/after `idx` are on OPPOSITE sides of the word/non-word divide
/// (treating "past either end of the string" as non-word, matching `\b`'s own start/end-of-string
/// rule). `before == after` (both word chars, e.g. mid-word, OR both non-word chars, e.g. between
/// two spaces) is NEVER a boundary — only a true transition is.
fn is_word_boundary(bytes: &[u8], idx: usize) -> bool {
    let is_word_byte = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let before = idx.checked_sub(1).map(|i| is_word_byte(bytes[i])).unwrap_or(false);
    let after = bytes.get(idx).map(|&b| is_word_byte(b)).unwrap_or(false);
    before != after
}

/// Splits on a literal, case-insensitive separator — a regex-free port of `.split(/\s+and\s+/i)`,
/// narrowed to the single literal `" and "` (with single spaces) since every real civType value
/// uses exactly that spacing (verified against the whole committed slice) — same "narrow to the
/// pinned dataset" precedent as [`crate::civ_help`]'s `<br>` splitter. ASCII-only case-folding is
/// safe here too — `sep` (`" and "`) is plain ASCII, same rationale as [`replace_ci`].
fn split_ci<'a>(s: &'a str, sep: &str) -> impl Iterator<Item = &'a str> {
    let sep_lower = sep.to_ascii_lowercase();
    let lower = s.to_ascii_lowercase();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find(&sep_lower) {
        let idx = search_from + rel;
        parts.push(&s[start..idx]);
        start = idx + sep.len();
        search_from = start;
    }
    parts.push(&s[start..]);
    parts.into_iter()
}

/// Replaces a whole word (ASCII `\b`-bounded), case-SENSITIVE (matches the source's `/\bArchers\b/g`
/// — no `i` flag).
fn replace_whole_word(s: &str, word: &str, repl: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(word.as_bytes())
            && is_word_boundary(bytes, i)
            && is_word_boundary(bytes, i + word.len())
        {
            out.push_str(repl);
            i += word.len();
            continue;
        }
        let ch = s[i..].chars().next().expect("i is a valid char boundary");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_for_resolves_a_known_civ() {
        assert_eq!(region_for("britons").unwrap(), "Western European");
        assert_eq!(region_for("armenians").unwrap(), "Caucasian");
    }

    #[test]
    fn region_for_an_unknown_slug_fails_loud() {
        match region_for("atlanteans").unwrap_err() {
            RefdataError::MissingCivRegion { slug } => assert_eq!(slug, "atlanteans"),
            other => panic!("expected MissingCivRegion, got {other:?}"),
        }
    }

    #[test]
    fn region_override_covers_every_real_committed_civ() {
        // Cross-checked against reference-data/aoe2techtree-civs.json in the task's research: all
        // 53 civs resolve directly, so the REGION_MAP/expansion fallback this table replaces was
        // already dead code.
        let civs = crate::civs::load().unwrap();
        assert_eq!(civs.len(), 53);
        assert_eq!(REGION_OVERRIDE.len(), 53);
    }

    #[test]
    fn region_noun_for_known_and_unknown_regions() {
        assert_eq!(region_noun_for("Caucasian"), "the Caucasus");
        assert_eq!(region_noun_for("Nowhere"), "Nowhere");
    }

    #[test]
    fn fix_specialty_title_cases_and_preserves_hyphen_tail_lowercase() {
        // The real Mapuche value — hyphen tail stays lowercase, matching the committed file.
        assert_eq!(fix_specialty("Cavalry and Counter-Units "), "Cavalry and Counter-units");
    }

    #[test]
    fn fix_specialty_passes_through_already_clean_values() {
        assert_eq!(fix_specialty("Foot Archer "), "Foot Archer");
        assert_eq!(fix_specialty("Infantry and Monk "), "Infantry and Monk");
        assert_eq!(fix_specialty("Cavalry Archer "), "Cavalry Archer");
    }

    #[test]
    fn fix_specialty_fixes_the_calvary_typo_and_strips_civilzation() {
        assert_eq!(fix_specialty("Calvary and Siege Civilzation"), "Cavalry and Siege");
    }

    #[test]
    fn fix_specialty_singularizes_archers() {
        assert_eq!(fix_specialty("Archers"), "Archer");
    }

    #[test]
    fn fix_specialty_does_not_match_archers_as_a_substring() {
        // Word-boundary correctness: "Archers" embedded in a longer word must NOT be replaced.
        assert_eq!(fix_specialty("Archersmith"), "Archersmith");
        assert_eq!(fix_specialty("FootArchers"), "Footarchers");
    }

    #[test]
    fn is_word_boundary_is_false_between_two_non_word_bytes() {
        // `\b` never matches where NEITHER side is a word char (e.g. between two spaces) — a real
        // bug found during review had an over-broad formula that returned true here too. Dead in
        // practice for every current caller (each always has a guaranteed-word-char on one side —
        // the literal word being matched), but this locks in the general contract regardless.
        let bytes = b"a  b"; // positions 2/3 are both spaces
        assert!(!is_word_boundary(bytes, 2));
    }

    #[test]
    fn fix_specialty_expands_cavalry_infantry_special_case() {
        assert_eq!(fix_specialty("cavalry infantry"), "Cavalry and Infantry");
    }

    #[test]
    fn unique_unit_override_covers_exactly_the_three_known_gap_civs() {
        assert_eq!(UNIQUE_UNIT_OVERRIDE.len(), 3);
        assert_eq!(unique_unit_override_for("incas"), Some(&["kamayuk", "slinger"][..]));
        assert_eq!(unique_unit_override_for("britons"), None);
    }
}
