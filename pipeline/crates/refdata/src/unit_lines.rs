//! Unit slug -> `{ line, lineRank }`, parsed from the committed `reference-data/unit-lines.tsv`
//! (SINGLE SOURCE OF TRUTH for these two hand-curated fields — read here, never fabricated). See
//! that file's header comment for provenance and the `#`-comment/blank-line skip convention this
//! parser follows (mirrors `pipeline_core::maps::parse`'s structural parse: `splitn(3, '\t')`,
//! `#`-comment and blank lines skipped).

use std::collections::HashMap;

use crate::error::{RefdataError, Result};

/// One `unit-lines.tsv` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitLine {
    pub line: String,
    pub line_rank: i32,
}

/// Slug-keyed lookup over the whole `unit-lines.tsv` table.
#[derive(Debug, Clone, Default)]
pub struct UnitLineTable(HashMap<String, UnitLine>);

impl UnitLineTable {
    /// Looks up a unit's line/rank by content slug. `None` for a slug the table doesn't cover.
    pub fn get(&self, slug: &str) -> Option<&UnitLine> {
        self.0.get(slug)
    }
}

/// Parses `unit-lines.tsv`-shaped text: `slug<TAB>line<TAB>lineRank`, skipping `#`-comment and
/// blank lines. A row whose `lineRank` isn't a valid integer is dropped (matches
/// `pipeline_core::civs`'s "a malformed key is dropped rather than panicking the parse" rule) —
/// in practice every row in the committed file parses, exercised by this module's own tests.
pub fn parse(tsv: &str) -> UnitLineTable {
    let mut by_slug = HashMap::new();
    for line in tsv.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut it = line.splitn(3, '\t');
        let (Some(slug), Some(line_name), Some(rank_str)) = (it.next(), it.next(), it.next())
        else {
            continue;
        };
        let Ok(line_rank) = rank_str.trim().parse::<i32>() else {
            continue;
        };
        by_slug.insert(
            slug.trim().to_string(),
            UnitLine {
                line: line_name.trim().to_string(),
                line_rank,
            },
        );
    }
    UnitLineTable(by_slug)
}

/// Loads the real, committed `reference-data/unit-lines.tsv`, baked into the binary at compile
/// time.
pub fn load() -> UnitLineTable {
    parse(include_str!("../../../../reference-data/unit-lines.tsv"))
}

/// Looks up `slug` or returns [`RefdataError::MissingUnitLine`] — the "fail loud, no default"
/// entry point [`crate::unit_stats::build`] actually calls (see that module's doc for why a gap
/// here is a hard error rather than a fabricated `line`/`lineRank`).
pub fn require<'a>(table: &'a UnitLineTable, slug: &'static str) -> Result<&'a UnitLine> {
    table.get(slug).ok_or(RefdataError::MissingUnitLine { slug })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_fixture_and_skips_comments_and_blanks() {
        let t = parse("# comment\nmilitia\tmilitia-line\t1\n\nman-at-arms\tmilitia-line\t2\n");
        assert_eq!(
            t.get("militia"),
            Some(&UnitLine {
                line: "militia-line".to_string(),
                line_rank: 1
            })
        );
        assert_eq!(t.get("man-at-arms").unwrap().line_rank, 2);
        assert_eq!(t.get("nonexistent"), None);
    }

    #[test]
    fn real_file_covers_every_canonical_unit() {
        let table = load();
        for (display_name, slug) in crate::canonical_units::CANONICAL_UNITS {
            assert!(
                table.get(slug).is_some(),
                "unit-lines.tsv is missing a row for canonical unit {display_name:?} (slug {slug:?})"
            );
        }
    }

    #[test]
    fn real_file_has_the_militia_line_in_rank_order() {
        let table = load();
        assert_eq!(table.get("militia").unwrap().line_rank, 1);
        assert_eq!(table.get("champion").unwrap().line_rank, 5);
    }
}
