//! Regex-free port of `scripts/lib/parse-help.mjs`'s `parseHelp()` — the generalized parser for
//! aoe2techtree civ help strings (EN + TR). A civ's help string lists its bonuses, unique unit(s),
//! unique techs, and team bonus, `<br>`-separated, with `•`-bulleted list items; section headers
//! differ per language (see [`Lang::civ_suffix`] and friends).
//!
//! **Regex-free, deliberately narrowed to the pinned dataset** (same rationale as
//! [`crate::icon_map`]'s own `slugify`): the source regex splits on `/<br\s*\/?>/i` (tolerating
//! `<br/>`, `<BR>`, `<br />`, ...), but every one of the committed
//! `reference-data/aoe2techtree-civs.json` help strings uses the single literal `"<br>"` — verified
//! by scanning the whole slice. [`split_br_lines`] therefore splits on that exact literal; a future
//! aoe2techtree re-slice using a different `<br>` spelling would just fail to split at that one
//! point (visible immediately as a garbled section), never silently corrupt another civ's output.

use crate::error::{RefdataError, Result};

/// Which locale's help-string vocabulary to parse against (mirrors `parse-help.mjs`'s `MARKERS`
/// table). No `#[non_exhaustive]` — this crate defines its own closed two-locale vocabulary (see
/// the playbook's "our own enums stay exhaustive" rule), unlike an enum mirroring an external
/// vocabulary this crate doesn't control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Tr,
}

impl Lang {
    /// The site-facing label used in [`RefdataError`] messages.
    pub fn label(self) -> &'static str {
        match self {
            Lang::En => "EN",
            Lang::Tr => "TR",
        }
    }

    /// The trailing "... civilization" / "... medeniyeti" suffix a line is checked against
    /// (`M.civ` in `parse-help.mjs`), matched case-insensitively at the END of the line.
    fn civ_suffix(self) -> &'static str {
        match self {
            Lang::En => "civilization",
            Lang::Tr => "medeniyeti",
        }
    }

    /// The "Unique Unit" section-header PREFIX (`M.unit`), matched case-insensitively at the
    /// START of the line.
    fn unit_prefix(self) -> &'static str {
        match self {
            Lang::En => "Unique Unit",
            Lang::Tr => "Özgün Birim",
        }
    }

    /// The "Unique Tech(s)" section-header PREFIX(es) (`M.tech`). TR accepts either spelling
    /// Byzantines' source uses ("Özel Teknoloji", a synonym of the usual "Özgün Teknoloji") — see
    /// `parse-help.mjs`'s own comment on this.
    fn tech_prefixes(self) -> &'static [&'static str] {
        match self {
            Lang::En => &["Unique Tech"],
            Lang::Tr => &["Özgün Teknoloji", "Özel Teknoloji"],
        }
    }

    /// The "Team Bonus" section-header PREFIX (`M.team`).
    fn team_prefix(self) -> &'static str {
        match self {
            Lang::En => "Team Bonus",
            Lang::Tr => "Takım Bonusu",
        }
    }

    /// The label an unpacked `<i>Upgrades: ...</i>`-style line is expected to start with — reused
    /// by [`crate::unit_help`] (a DIFFERENT help-string family, same two locales).
    pub fn upgrades_label(self) -> &'static str {
        match self {
            Lang::En => "Upgrades:",
            Lang::Tr => "Yükseltmeler:",
        }
    }
}

/// One parsed unique tech: `{ name, effect }` — `effect` is `""` when the source line has no
/// trailing `(...)` parenthetical the "find the LAST valid `(...)` at the line's end" split (see
/// [`split_name_effect`]) can extract (a genuine upstream-data quirk some civs' help strings carry
/// — e.g. Armenians' imperial tech nests a parenthetical INSIDE its effect text, which defeats that
/// split exactly as it defeats the source `.mjs`'s own regex; ported faithfully, not "fixed", so
/// this crate's output matches the committed `civilizations.json` byte-for-byte).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UniqueTechText {
    pub name: String,
    pub effect: String,
}

/// The whole parsed help string.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HelpParse {
    pub civ_type: String,
    pub civ_bonuses: Vec<String>,
    pub team_bonus: String,
    pub unique_techs: Vec<UniqueTechText>,
    pub unique_unit_names: Vec<String>,
}

/// Parses a civ's raw help string. Returns `None` when zero civ bonuses were found — mirrors
/// `parse-help.mjs`'s `return out.civBonuses.length ? out : null` (its own "this wasn't really civ
/// help text" signal).
pub fn parse_help(raw: &str, lang: Lang) -> Option<HelpParse> {
    let mut out = HelpParse::default();
    let mut section = Section::Bonuses;

    for raw_line in split_br_lines(raw) {
        let line = strip_letter_tags(&raw_line).trim().to_string();
        if line.is_empty() {
            continue;
        }

        if out.civ_type.is_empty() {
            if let Some(stripped) = strip_suffix_ci(&line, lang.civ_suffix()) {
                out.civ_type = stripped.trim().to_string();
                continue;
            }
        }
        if starts_with_ci(&line, lang.unit_prefix()) {
            section = Section::Unit;
            continue;
        }
        if lang.tech_prefixes().iter().any(|p| starts_with_ci(&line, p)) {
            section = Section::Techs;
            continue;
        }
        if starts_with_ci(&line, lang.team_prefix()) {
            section = Section::Team;
            continue;
        }

        let has_bullet = line.starts_with('•');
        let text = strip_bullet_prefix(&line).trim().to_string();
        match section {
            Section::Bonuses if has_bullet => out.civ_bonuses.push(text),
            Section::Bonuses => {}
            Section::Unit => {
                for part in text.split(',') {
                    let name = strip_trailing_parenthetical(part.trim());
                    if !name.is_empty() {
                        out.unique_unit_names.push(name);
                    }
                }
            }
            Section::Team => {
                out.team_bonus =
                    if out.team_bonus.is_empty() { text } else { format!("{} {text}", out.team_bonus) };
            }
            Section::Techs if has_bullet => {
                let (name, effect) = split_name_effect(&text);
                out.unique_techs.push(UniqueTechText { name, effect });
            }
            Section::Techs => {}
        }
    }

    if out.civ_bonuses.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Parses `raw` and requires it to be non-`None` with exactly 2 unique techs — the shape every
/// real AoE2 civ's help text has. Fails loud with [`RefdataError::CivHelpUnparseable`] /
/// [`RefdataError::CivUniqueTechCount`] otherwise, mirroring `build-civilizations.mjs`'s own
/// `[FATAL]` guards.
pub fn require_parsed(raw: &str, lang: Lang, civ_key: &str, help_string_id: i64) -> Result<HelpParse> {
    let parsed = parse_help(raw, lang).ok_or_else(|| RefdataError::CivHelpUnparseable {
        civ_key: civ_key.to_string(),
        lang: lang.label(),
        help_string_id,
    })?;
    if parsed.unique_techs.len() != 2 {
        return Err(RefdataError::CivUniqueTechCount {
            civ_key: civ_key.to_string(),
            lang: lang.label(),
            count: parsed.unique_techs.len(),
        });
    }
    Ok(parsed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Bonuses,
    Unit,
    Techs,
    Team,
}

/// Splits `s` on the literal `"<br>"` — see the module doc for why this is narrower than the
/// source regex's `/<br\s*\/?>/i` and why that's safe for the pinned dataset.
fn split_br_lines(s: &str) -> Vec<String> {
    s.split("<br>").map(str::to_string).collect()
}

/// Strips `<b>`/`</b>`/`<i>`/`</i>`-style tags — letters-only tag NAME, optional leading `/` — a
/// regex-free port of `/<\/?[a-z]+>/gi`. Unlike [`crate::unit_help`]'s tag stripper (which accepts
/// ANY characters inside `<...>`, ported from `build-unit-descriptions.mjs`'s own, more permissive,
/// `/<[^>]+>/g`), this ONE only strips a tag whose name is purely ASCII letters — matching
/// `parse-help.mjs` exactly. On the real committed civ-help slice the only tags present are
/// `<b>`/`</b>` (verified by scanning it), so the two stripping rules agree in practice; kept
/// distinct here for source fidelity.
fn strip_letter_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let after_lt = &rest[lt + 1..];
        match letter_tag_len(after_lt) {
            Some(len) => rest = &after_lt[len..],
            None => {
                out.push('<');
                rest = after_lt;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Given the text immediately after an already-consumed `'<'`, returns the BYTE LENGTH of a valid
/// `[/]letters+>` tag body (optional leading `/`, one-or-more ASCII letters, then `>` — INCLUDING
/// that trailing `>`), or `None` if `after` doesn't start with one. All three delimiters (`/`,
/// ASCII letters, `>`) are single-byte ASCII, so every intermediate index this scans to is a valid
/// UTF-8 char boundary regardless of any non-ASCII bytes elsewhere in `after`.
fn letter_tag_len(after: &str) -> Option<usize> {
    let bytes = after.as_bytes();
    let mut i = 0;
    if bytes.first() == Some(&b'/') {
        i += 1;
    }
    let letters_start = i;
    while bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
        i += 1;
    }
    if i == letters_start {
        return None; // no letters — `[a-z]+` requires at least one
    }
    if bytes.get(i) == Some(&b'>') { Some(i + 1) } else { None }
}

/// Case-insensitive "does `line` end with `suffix`" — a regex-free port of `` /suffix$/i ``. `Some`
/// carries the part of `line` BEFORE the matched suffix (unstripped, caller trims).
fn strip_suffix_ci<'a>(line: &'a str, suffix: &str) -> Option<&'a str> {
    if line.len() < suffix.len() {
        return None;
    }
    let split_at = line.len() - suffix.len();
    // `suffix` is always plain ASCII (both `"civilization"` and `"medeniyeti"`), so byte-length
    // slicing is safe regardless of any multi-byte UTF-8 earlier in `line` — `split_at` only needs
    // to land on a char boundary, which it does here because the byte-suffix we compare against
    // is itself the tail of a valid `&str` slice (see the `get` below, which returns `None` — never
    // panics — if that boundary assumption were ever violated by non-ASCII input).
    let tail = line.get(split_at..)?;
    if tail.eq_ignore_ascii_case(suffix) {
        line.get(..split_at)
    } else {
        None
    }
}

/// Case-insensitive "does `line` start with `prefix`" — a regex-free port of `` /^prefix/i ``.
/// ASCII-only comparison is sufficient: every `unit_prefix`/`tech_prefixes`/`team_prefix` literal
/// is either plain ASCII (EN) or Turkish text compared via full Unicode lowercasing (TR needs the
/// diacritics — `Ö`, `ü`, `ı`), so this delegates to `to_lowercase()` rather than
/// `eq_ignore_ascii_case`, unlike [`strip_suffix_ci`] (whose two suffix literals ARE plain ASCII).
fn starts_with_ci(line: &str, prefix: &str) -> bool {
    let mut line_chars = line.chars().flat_map(char::to_lowercase);
    let prefix_chars = prefix.chars().flat_map(char::to_lowercase);
    for pc in prefix_chars {
        match line_chars.next() {
            Some(lc) if lc == pc => continue,
            _ => return false,
        }
    }
    true
}

/// Strips a leading `"• "` bullet (bullet + optional following whitespace) — a regex-free port of
/// `.replace(/^•\s*/, "")`. Returns the rest of the line unchanged if there's no leading bullet
/// (matches the source's no-op-on-mismatch `replace` semantics).
fn strip_bullet_prefix(line: &str) -> &str {
    let Some(rest) = line.strip_prefix('•') else {
        return line;
    };
    rest.trim_start()
}

/// Strips a trailing `"(...)"` parenthetical (content may be EMPTY) — a regex-free port of
/// `.replace(/\s*\([^)]*\)\s*$/, "")`, used for unique-unit lines like `"Iron Pagoda (Cavalry)"` ->
/// `"Iron Pagoda"`. See [`split_name_effect`] for the sibling case where the parenthetical content
/// must be extracted (not just dropped) and must be non-empty.
fn strip_trailing_parenthetical(s: &str) -> String {
    let trimmed = s.trim_end();
    let Some(open_idx) = find_unnested_trailing_paren_open(trimmed) else {
        return s.trim().to_string();
    };
    trimmed[..open_idx].trim_end().to_string()
}

/// Splits `"Name (Effect)"` into `("Name", "Effect")` — a regex-free port of
/// `text.match(/^(.+?)\s*\(([^)]+)\)\s*$/)`. Returns `(text, "")` when no such trailing
/// parenthetical exists OR its content is EMPTY OR it's preceded by nothing (an empty `name`) —
/// every one of those is a non-match for the source regex too (`[^)]+` requires >=1 content char;
/// `(.+?)` requires >=1 name char). See [`find_unnested_trailing_paren_open`] for why a NESTED
/// parenthetical (e.g. Armenians' imperial tech, `"Fereters (Infantry (except Spearman-line) +30
/// HP...)"`) also falls through to this no-match case — faithfully, since the source regex's
/// `[^)]+` can't cross the inner `)` either.
fn split_name_effect(text: &str) -> (String, String) {
    let trimmed = text.trim_end();
    let Some(open_idx) = find_unnested_trailing_paren_open(trimmed) else {
        return (text.trim().to_string(), String::new());
    };
    let name = trimmed[..open_idx].trim_end();
    let effect = &trimmed[open_idx + 1..trimmed.len() - 1];
    if name.is_empty() || effect.is_empty() {
        return (text.trim().to_string(), String::new());
    }
    (name.to_string(), effect.to_string())
}

/// The shared search behind [`strip_trailing_parenthetical`]/[`split_name_effect`]: `trimmed` (no
/// trailing whitespace) must end with `')'`; this then scans BACKWARD from just before that closing
/// paren looking for the matching `'('` — succeeding only if NO other `')'` is crossed first (i.e.
/// the parenthetical's content contains no nested `)`, matching `[^)]+`/`[^)]*`'s exclusion) and at
/// least one character remains before it (a non-empty `name`, `(.+?)`'s `>=1` requirement). Returns
/// the BYTE INDEX of the opening `'('` on success.
fn find_unnested_trailing_paren_open(trimmed: &str) -> Option<usize> {
    if !trimmed.ends_with(')') {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let close_idx = bytes.len() - 1;
    let mut i = close_idx;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b')' => return None, // a nested close paren before any open — no valid split
            b'(' => return if i == 0 { None } else { Some(i) },
            _ => continue,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRITONS_EN: &str = "Foot Archer civilization<br>\n<br>\n\
        • Shepherds work +25% faster<br>\n\
        • Town Centers cost -50% wood starting in Castle Age<br>\n\
        • Foot Archers +1/+2 range in Castle/Imperial Age <br>\n<br>\n\
        <b>Unique Unit:</b> <br>\nLongbowman (Foot Archer)<br>\n<br>\n\
        <b>Unique Techs:</b> <br>\n\
        • Yeomen (Foot Archers and Skirmisher-line +1 range; Watch Tower-line +2 attack)<br>\n\
        • Warwolf (Trebuchets deal blast damage and are more accurate)<br>\n<br>\n\
        <b>Team Bonus:</b> <br>\nArchery Ranges work +10% faster";

    #[test]
    fn parses_the_real_britons_help_string() {
        let out = parse_help(BRITONS_EN, Lang::En).expect("must parse");
        assert_eq!(out.civ_type, "Foot Archer");
        assert_eq!(
            out.civ_bonuses,
            vec![
                "Shepherds work +25% faster",
                "Town Centers cost -50% wood starting in Castle Age",
                "Foot Archers +1/+2 range in Castle/Imperial Age",
            ]
        );
        assert_eq!(out.unique_unit_names, vec!["Longbowman"]);
        assert_eq!(
            out.unique_techs,
            vec![
                UniqueTechText {
                    name: "Yeomen".into(),
                    effect: "Foot Archers and Skirmisher-line +1 range; Watch Tower-line +2 attack"
                        .into()
                },
                UniqueTechText {
                    name: "Warwolf".into(),
                    effect: "Trebuchets deal blast damage and are more accurate".into()
                },
            ]
        );
        assert_eq!(out.team_bonus, "Archery Ranges work +10% faster");
    }

    #[test]
    fn parses_the_tr_variant_with_diacritic_markers() {
        let tr = "Yaya Okçu medeniyeti<br>\n<br>\n\
            • Çobanlar %25 daha hızlı çalışır<br>\n<br>\n\
            <b>Özgün Birim:</b> <br>\nUzunyay Okçusu (Yaya Okçu)<br>\n<br>\n\
            <b>Özgün Teknolojiler:</b> <br>\n\
            • Yeomen (Menzil +1)<br>\n\
            • Warwolf (Hasar artar)<br>\n<br>\n\
            <b>Takım Bonusu:</b> <br>\nOkçuluk Binası %10 daha hızlı çalışır";
        let out = parse_help(tr, Lang::Tr).expect("must parse");
        assert_eq!(out.civ_type, "Yaya Okçu");
        assert_eq!(out.civ_bonuses, vec!["Çobanlar %25 daha hızlı çalışır"]);
        assert_eq!(out.unique_unit_names, vec!["Uzunyay Okçusu"]);
        assert_eq!(out.unique_techs.len(), 2);
        assert_eq!(out.team_bonus, "Okçuluk Binası %10 daha hızlı çalışır");
    }

    #[test]
    fn byzantines_tr_ozel_teknoloji_synonym_is_recognized() {
        let tr = "Savunma medeniyeti<br>\n• Bonus<br>\n<b>Özel Teknolojiler:</b> <br>\n• X (Y)";
        let out = parse_help(tr, Lang::Tr).expect("must parse");
        assert_eq!(out.unique_techs, vec![UniqueTechText { name: "X".into(), effect: "Y".into() }]);
    }

    #[test]
    fn a_string_with_no_bullets_parses_to_none() {
        assert_eq!(parse_help("Just prose, no bullets<br>\nMore prose", Lang::En), None);
    }

    #[test]
    fn nested_parenthetical_falls_through_to_name_only_like_armenians_imperial_tech() {
        // The exact real-data quirk this crate must reproduce byte-for-byte (see the module doc
        // and split_name_effect's doc) — a REAL committed value, not a synthetic edge case.
        let text = "Fereters (Infantry (except Spearman-line) +30 HP; Warrior Priests heal +100% faster)";
        let (name, effect) = split_name_effect(text);
        assert_eq!(name, text);
        assert_eq!(effect, "");
    }

    #[test]
    fn empty_parenthetical_content_is_not_a_match() {
        assert_eq!(split_name_effect("Name ()"), ("Name ()".to_string(), String::new()));
    }

    #[test]
    fn multiple_trailing_parens_split_at_the_last_one() {
        assert_eq!(split_name_effect("Name (a) (b)"), ("Name (a)".to_string(), "b".to_string()));
    }

    #[test]
    fn strip_trailing_parenthetical_handles_a_unit_class_suffix() {
        assert_eq!(strip_trailing_parenthetical("Iron Pagoda (Cavalry)"), "Iron Pagoda");
        assert_eq!(strip_trailing_parenthetical("Grenadier"), "Grenadier");
    }

    #[test]
    fn real_committed_slice_parses_every_civ_with_two_unique_techs() {
        let civs = crate::civs::load().expect("committed civs slice must parse");
        for (key, civ) in &civs {
            let en = require_parsed(&civ.help_en, Lang::En, key, civ.help_string_id)
                .unwrap_or_else(|e| panic!("{key} EN: {e}"));
            let tr = require_parsed(&civ.help_tr, Lang::Tr, key, civ.help_string_id)
                .unwrap_or_else(|e| panic!("{key} TR: {e}"));
            assert!(!en.civ_type.is_empty(), "{key}: EN civ_type must be non-empty");
            assert!(!tr.civ_type.is_empty(), "{key}: TR civ_type must be non-empty");
        }
        assert_eq!(civs.len(), 53);
    }

    #[test]
    fn real_britons_civ_matches_the_inline_fixture() {
        let civs = crate::civs::load().unwrap();
        let britons = &civs["Britons"];
        let out = parse_help(&britons.help_en, Lang::En).unwrap();
        assert_eq!(out.civ_type, "Foot Archer");
        assert_eq!(out.unique_unit_names, vec!["Longbowman"]);
    }
}
