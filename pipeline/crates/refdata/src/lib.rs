//! `refdata` — strict Rust replacement for `scripts/build-units.mjs`, `scripts/build-game-facts.mjs`,
//! `scripts/build-unit-tech-names.mjs`, and `scripts/build-icon-map.mjs`. Derives
//! `src/data/unit-stats.json` (numeric unit stats), `src/data/game-facts.json` (per-unit
//! age/building/cost/attackBonus), `src/data/unit-names.json` + `src/data/tech-names.json` (game DAT
//! id -> English display name, for `pipeline-core`'s own `units`/`techs` dims), and
//! `src/data/icon-map.json` (slug -> `/images/aoe2/...` icon path) — ALL from the AUTHORITATIVE
//! aoe2techtree game data — no aalises anywhere.
//!
//! **Source: aoe2techtree, not the aalises CSV.** The earlier version of this crate read the
//! aalises `age-of-empires-II-api` `units.csv` and then patched it with a `STAT_CORRECTIONS`
//! table — but every one of those corrections existed only to fix aalises *against aoe2techtree*
//! (e.g. aalises transposed the Archer's HP/Range to 4/30; the real values are 30/4). That made
//! aoe2techtree the real source of truth all along, reached through a buggy middleman. This crate
//! now reads aoe2techtree directly (the SAME pinned `AOE2TECHTREE_SHA` the repo already uses for
//! civ text, unit/tech names, and icons), so the corrections are gone (~68 unit-stats units get
//! their genuinely-correct DE stats — see the task report's diff list).
//!
//! The four `game-facts.json` fields all come from aoe2techtree: `name`+`cost` from `data.Unit`
//! ([`techtree`]), `age`+`building` from the civ-mode of the 53 base-game tech trees
//! ([`unit_tree`]), and `attackBonus` from `data.Unit.Attacks` + aoe2techtree's own armour-class
//! vocabulary ([`armor_classes`]).
//!
//! Functional core, imperative shell (`pipeline/docs/rust-playbook.md`): every module here is pure
//! string-in/typed-doc-out — the only filesystem touch is the `include_str!`'d committed source
//! slices (`*::load`). All real output I/O (writing the JSON to `--out DIR`) lives in `main.rs` —
//! as does the ONE genuine input read this crate performs, [`icon_map`]'s live scan of
//! `public/images/aoe2/` (thousands of binary PNGs, which can't be `include_str!`'d; see that
//! module's doc for why this is a deliberate, narrow exception).
//!
//! Not to be confused with `pipeline_core`'s OWN `refdata` cargo feature (a different id space:
//! `pipeline_core::units` maps a replay's numeric `unit_id` → name for DB ingest; this crate maps
//! a display NAME → stats/facts keyed by content `slug`).
pub mod armor_classes;
pub mod canonical_units;
pub mod error;
pub mod game_facts;
pub mod icon_items;
pub mod icon_map;
pub mod model;
pub mod name_to_slug;
pub mod techs;
pub mod techtree;
pub mod tree_nodes;
pub mod unit_lines;
pub mod unit_stats;
pub mod unit_tech_names;
pub mod unit_tree;

pub use error::{RefdataError, Result};

/// Pinned upstream source SHA — mirrors `AOE2TECHTREE_SHA` in `scripts/lib/aoe2-sources.mjs` (the
/// same commit the repo pins for civ text, unit/tech names, and icons). Kept as a literal here
/// (this crate has no reason to depend on the `.mjs`): if the pin moves, bump both together AND
/// re-slice `reference-data/aoe2techtree-*.json` from that SHA at the same time.
pub const AOE2TECHTREE_SHA: &str = "b9d494df6921d4080df69b22f9dbb7a4d1dcd9f0";

#[cfg(test)]
mod tests {
    #[test]
    fn committed_source_slices_are_embedded_and_parse() {
        // The `include_str!`s resolve and the two slices deserialize + resolve a known unit.
        let tt = super::techtree::load().expect("committed slices must parse");
        assert!(
            tt.resolve("Knight", "knight").is_ok(),
            "a staple unit must resolve from the real committed source"
        );
    }
}
