//! `refdata` — strict Rust replacement for `scripts/build-units.mjs`. Derives
//! `src/data/unit-stats.json` (numeric unit stats) from the AUTHORITATIVE aoe2techtree game data.
//!
//! (`game-facts.json` stays owned by its JS generator for now: its `age`/`building` come from the
//! per-civ tech-tree structure, not `data.Unit`, and its `attackBonus` is editorial free-text — a
//! proper aoe2techtree port of it is a separate future task, out of scope here.)
//!
//! **Source: aoe2techtree `data.Unit`, not the aalises CSV.** The earlier version of this crate
//! read the aalises `age-of-empires-II-api` `units.csv` and then patched it with a
//! `STAT_CORRECTIONS` table — but every one of those corrections existed only to fix aalises
//! *against aoe2techtree* (e.g. aalises transposed the Archer's HP/Range to 4/30; the real values
//! are 30/4). That made aoe2techtree the real source of truth all along, reached through a buggy
//! middleman. This crate now reads aoe2techtree directly (the SAME pinned `AOE2TECHTREE_SHA` the
//! repo already uses for civ text, unit/tech names, and icons), so the corrections are gone and
//! ~68 units get their genuinely-correct DE stats (the aalises file was stale/wrong — see the task
//! report's diff list).
//!
//! Functional core, imperative shell (`pipeline/docs/rust-playbook.md`): every module here is pure
//! string-in/typed-doc-out — the only filesystem touch is the `include_str!`'d committed source
//! slices ([`techtree::load`]/[`unit_lines::load`]). All real output I/O (writing the JSON to
//! `--out DIR`) lives in `main.rs`.
//!
//! Not to be confused with `pipeline_core`'s OWN `refdata` cargo feature (a different id space:
//! `pipeline_core::units` maps a replay's numeric `unit_id` → name for DB ingest; this crate maps
//! a display NAME → HP/attack/cost STATS keyed by content `slug`).
pub mod canonical_units;
pub mod error;
pub mod model;
pub mod techtree;
pub mod unit_lines;
pub mod unit_stats;

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
