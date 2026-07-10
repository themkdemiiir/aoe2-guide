//! This crate's one `thiserror` enum (playbook: "one thiserror enum per crate in `error.rs`").
//!
//! Every variant is a "fail loud, no default" case per this repo's standing rule
//! (`feedback_no_default_values`): a build-time reference-data generator must stop, never emit a
//! fabricated/guessed value, when its authoritative source doesn't cleanly cover something its
//! canonical unit list expects.
//!
//! No filesystem/output-serialization variants here (playbook principle 2: a typed variant must
//! earn its keep by something downstream matching on it) — the library half is pure
//! string-in/typed-doc-out; the actual output file writes happen in `main.rs`, wrapped in plain
//! `anyhow::Context`. The one deserialization variant below IS typed, because it's the library
//! reading its OWN committed source slices, and `unit_stats` treats "source slice corrupt"
//! distinctly from "a specific unit couldn't be resolved".

pub type Result<T> = std::result::Result<T, RefdataError>;

#[derive(Debug, thiserror::Error)]
pub enum RefdataError {
    /// A committed aoe2techtree source slice (`reference-data/aoe2techtree-*.json`) failed to
    /// deserialize — a structural corruption of the pinned source, caught at the boundary.
    #[error("failed to parse committed source slice {file}")]
    ParseSource {
        file: &'static str,
        #[source]
        source: serde_json::Error,
    },

    /// A [`crate::canonical_units::CANONICAL_UNITS`] display name resolved to NO unit in the
    /// aoe2techtree source — either the pinned slice changed, or a display name drifted from the
    /// game's own `<b>...</b>` name. (This is where the aalises-vs-game name mismatches surfaced:
    /// aalises "Camel"/"Arbalest" vs the game's "Camel Rider"/"Arbalester" — resolved by using the
    /// game names in `CANONICAL_UNITS`.)
    #[error("no aoe2techtree unit resolves to display name {display_name:?} (slug {slug:?})")]
    UnitNotFound {
        display_name: &'static str,
        slug: &'static str,
    },

    /// A display name resolved to two or more unit ids whose STATS differ — genuinely different
    /// units sharing a name, which the crate must never silently disambiguate. (Stat-IDENTICAL
    /// duplicates — graphics/formation variants like Spearman's `PKEMN`/`DPKEMN` — are fine and
    /// resolve to the lowest id; only a real stat divergence trips this.)
    #[error("display name {display_name:?} (slug {slug:?}) maps to units {ids:?} with differing stats")]
    AmbiguousUnit {
        display_name: &'static str,
        slug: &'static str,
        /// The lowest-id unit and the first divergent one — enough to point a human at the conflict
        /// without listing every duplicate.
        ids: (i32, i32),
    },

    /// A [`crate::canonical_units::CANONICAL_UNITS`] slug has no row in
    /// `reference-data/unit-lines.tsv` — that curated table (the upgrade-line grouping, which is
    /// NOT in the game data) must cover every canonical unit or the generated `line`/`lineRank`
    /// would have to be fabricated.
    #[error("reference-data/unit-lines.tsv has no row for canonical unit slug {slug:?}")]
    MissingUnitLine { slug: &'static str },
}
