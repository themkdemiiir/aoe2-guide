//! Attack/armour class id -> readable label, parsed from the committed
//! `reference-data/aoe2techtree-armor-classes.tsv` (SINGLE SOURCE OF TRUTH — read here, never
//! hand-typed). The labels ARE aoe2techtree's own tooltip vocabulary (its `attackAndArmorClasses`
//! table); see the tsv's header for provenance. Backs `game-facts.json`'s `attackBonus` labels.
//!
//! Structural parse mirrors `pipeline_core::maps::parse`: `splitn(2, '\t')`, `#`-comment and blank
//! lines skipped.

use std::collections::HashMap;

use crate::error::{RefdataError, Result};

/// Class id -> label lookup over the whole committed table.
#[derive(Debug, Clone, Default)]
pub struct ArmorClassTable(HashMap<i32, String>);

impl ArmorClassTable {
    /// Resolves a class id to its label, or [`RefdataError::UnknownAttackClass`] — a unit's attack
    /// citing a class absent from aoe2techtree's OWN class table is a fail-loud anomaly (never an
    /// unnamed/guessed bonus).
    pub fn label(&self, class: i32) -> Result<&str> {
        self.0
            .get(&class)
            .map(String::as_str)
            .ok_or(RefdataError::UnknownAttackClass { class })
    }
}

/// Parses `armor-classes.tsv`-shaped text: `class_id<TAB>label`, skipping `#`-comment and blank
/// lines. A row whose id isn't a valid integer is dropped (matches `pipeline_core::civs`'s "drop a
/// malformed key rather than panic" rule) — every committed row parses, exercised by the tests.
pub fn parse(tsv: &str) -> ArmorClassTable {
    let mut by_id = HashMap::new();
    for line in tsv.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut it = line.splitn(2, '\t');
        let (Some(id), Some(label)) = (it.next(), it.next()) else {
            continue;
        };
        let Ok(id) = id.trim().parse::<i32>() else {
            continue;
        };
        by_id.insert(id, label.trim().to_string());
    }
    ArmorClassTable(by_id)
}

/// Loads the real committed `aoe2techtree-armor-classes.tsv`, baked into the binary at compile
/// time.
pub fn load() -> ArmorClassTable {
    parse(include_str!(
        "../../../../reference-data/aoe2techtree-armor-classes.tsv"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_fixture_and_skips_comments() {
        let t = parse("# comment\n27\tSpearmen\n\n8\tMounted Units\n");
        assert_eq!(t.label(27).unwrap(), "Spearmen");
        assert_eq!(t.label(8).unwrap(), "Mounted Units");
    }

    #[test]
    fn unknown_class_fails_loud() {
        let t = parse("27\tSpearmen\n");
        match t.label(999).unwrap_err() {
            RefdataError::UnknownAttackClass { class } => assert_eq!(class, 999),
            other => panic!("expected UnknownAttackClass, got {other:?}"),
        }
    }

    #[test]
    fn real_table_has_the_authoritative_labels() {
        let t = load();
        assert_eq!(t.label(27).unwrap(), "Spearmen");
        assert_eq!(t.label(11).unwrap(), "All Buildings");
        assert_eq!(t.label(25).unwrap(), "Monastery Units");
        // `<abbr>` HTML must be stripped to the visible text.
        assert_eq!(t.label(8).unwrap(), "Mounted Units");
        assert_eq!(t.label(21).unwrap(), "Standard Buildings");
    }
}
