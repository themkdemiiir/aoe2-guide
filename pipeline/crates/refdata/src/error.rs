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
//! `anyhow::Context`. The [`RefdataError::ParseSource`] variant IS typed, because it's the library
//! reading its OWN committed source slices, and a caller (`unit_stats`/`game_facts`) treats "source
//! slice corrupt" distinctly from "a specific unit couldn't be resolved".

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

    /// A [`crate::name_to_slug::NAME_TO_SLUG`] unit resolved by name but has no observation in
    /// `reference-data/aoe2techtree-unit-tree.json` — the pinned tree slice changed, or the unit
    /// isn't trained at any building. Fail loud rather than fabricate an age/building.
    #[error("unit {unit_id} (slug {slug:?}) has no age/building observation in the tree slice")]
    MissingAgeBuilding {
        slug: &'static str,
        unit_id: i32,
    },

    /// A unit's `Attacks` cites an armour class absent from
    /// `reference-data/aoe2techtree-armor-classes.tsv` (aoe2techtree's OWN class table) — a real
    /// anomaly (new class id), never an unnamed/guessed bonus label.
    #[error("attack class {class} is not in the committed armor-classes table")]
    UnknownAttackClass { class: i32 },

    /// A `reference-data/aoe2techtree-tree-nodes.json` observation row carries a `use_type` other
    /// than `"Unit"`/`"Tech"` — the committed slice is built to only ever contain those two (see
    /// its `provenance.note`), so this means the slice itself is corrupt or was regenerated with
    /// different filtering.
    #[error("tree-node observation for node_id {node_id} has unexpected use_type {use_type:?}")]
    UnknownTreeNodeUseType { node_id: i32, use_type: String },

    /// The same `(use_type, node_id)` resolves to two DIFFERENT names across
    /// `reference-data/aoe2techtree-tree-nodes.json`'s base-game civ observations — genuinely
    /// conflicting per-civ tree data, which must never be silently disambiguated. Mirrors
    /// `scripts/build-unit-tech-names.mjs`'s own `throw` on the same condition (see
    /// [`crate::tree_nodes`]'s module doc).
    #[error(
        "tree node {node_id} ({use_type}) has conflicting names across civs: {first:?} vs {second:?}"
    )]
    ConflictingTreeNodeName {
        node_id: i32,
        use_type: String,
        first: String,
        second: String,
    },

    /// A civ's `help_en`/`help_tr` string (from `reference-data/aoe2techtree-civs.json`) parsed to
    /// zero civ bonuses — [`crate::civ_help::parse_help`]'s own "not real civ help text" signal
    /// (mirrors `scripts/lib/parse-help.mjs`'s `return out.civBonuses.length ? out : null`). Fail
    /// loud rather than emit a civ with an empty bonus list.
    #[error("civ {civ_key:?} has no parseable {lang} help text (help_string_id {help_string_id})")]
    CivHelpUnparseable {
        civ_key: String,
        lang: &'static str,
        help_string_id: i64,
    },

    /// A civ's EN or TR help text parsed to a unique-tech count other than exactly 2 (every AoE2
    /// civ has a Castle-Age and an Imperial-Age unique tech) — mirrors
    /// `scripts/build-civilizations.mjs`'s own `[FATAL] ... expected 2` guard.
    #[error("civ {civ_key:?} {lang} help has {count} unique tech(s), expected 2")]
    CivUniqueTechCount {
        civ_key: String,
        lang: &'static str,
        count: usize,
    },

    /// A civ resolved from [`crate::civs`] has no row in [`crate::civ_region::REGION_OVERRIDE`] —
    /// that curated table (originally the aalises `civilizations.csv` `expansion` column, now a
    /// small hand-curated replacement — see that module's doc) must cover every civ the committed
    /// `aoe2techtree-civs.json` slice carries, or the generated `region` would have to be
    /// fabricated.
    #[error("no entry for civ slug {slug:?} in the curated REGION_OVERRIDE table")]
    MissingCivRegion { slug: String },

    /// A civ's help-derived unique-unit name(s), plus the small curated
    /// [`crate::civ_region::UNIQUE_UNIT_OVERRIDE`] supplement, still resolved to an EMPTY list —
    /// mirrors `scripts/build-civilizations.mjs`'s own `[FATAL] ... no unique unit from any
    /// source` guard.
    #[error("civ {slug:?} resolved to zero unique units from any source")]
    NoCivUniqueUnit { slug: String },

    /// A [`crate::canonical_units::CANONICAL_UNITS`] unit's `language_name_id` has no EN or TR
    /// help string in the committed `aoe2techtree-unit-names(-tr).json` slices — the pinned slice
    /// changed, or this unit's help text was never sliced in.
    #[error("unit {slug:?} (language_name_id {language_name_id}) has no {lang} help string in the committed slice")]
    UnitHelpStringMissing {
        slug: &'static str,
        language_name_id: i64,
        lang: &'static str,
    },

    /// A unit's help string has no `<b>...</b>` title span — mirrors
    /// `scripts/build-unit-descriptions.mjs`'s `fail("no <b>name</b> in ... help string")`.
    #[error("unit {slug:?} {lang} help string has no <b>name</b> span")]
    UnitHelpNoBoldName { slug: &'static str, lang: &'static str },

    /// A unit's help string has no `<br>` after its title line — mirrors
    /// `build-unit-descriptions.mjs`'s `fail("no <br> after title line")`.
    #[error("unit {slug:?} {lang} help string has no <br> after the title line")]
    UnitHelpNoBreakAfterTitle { slug: &'static str, lang: &'static str },

    /// A unit's `<i>...</i>` upgrades line doesn't start with the expected `"Upgrades:"` /
    /// `"Yükseltmeler:"` label — mirrors `build-unit-descriptions.mjs`'s own label-prefix guard.
    #[error("unit {slug:?} {lang} upgrades line does not start with the expected label {label:?}")]
    UnitHelpUpgradesLabelMismatch {
        slug: &'static str,
        lang: &'static str,
        label: &'static str,
    },

    /// A unit's help body is empty after stripping HTML tags — mirrors
    /// `build-unit-descriptions.mjs`'s `fail("empty ... help body")`.
    #[error("unit {slug:?} {lang} help body is empty after stripping tags")]
    UnitHelpEmptyBody { slug: &'static str, lang: &'static str },

    /// A unit's `<i>Upgrades: ...</i>` line is present in one language's help string but absent
    /// in the other — mirrors `build-unit-descriptions.mjs`'s
    /// `fail("upgrades line present in one language but not the other")`.
    #[error("unit {slug:?} has an upgrades line in one language but not the other")]
    UnitHelpUpgradesLangMismatch { slug: &'static str },
}
