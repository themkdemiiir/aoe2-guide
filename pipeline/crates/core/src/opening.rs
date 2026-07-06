//! [`OpeningKind`] — the closed, cross-source-reconciled opening vocabulary backing
//! `match_players.opening_kind` (Postgres native enum `opening_kind`).
//!
//! ## Why this exists (final-review finding #1)
//! `match_players.opening` (kept, unchanged — see that column's own doc trail) is free TEXT
//! written by two producers with UN-RECONCILED vocabularies: the aoestats path writes its raw
//! archive label verbatim (lowercase `snake_case`, e.g. `"man_at_arms"`); the replay path
//! (`replay::derive::classify_opening`) writes a rich, human title-cased tag, sometimes a
//! composite (`"Scouts into Archers"`, `"Drush + Scouts"`, `"Fast Castle"`). `GROUP BY opening`
//! (`civ_meta_openings.sql`) treats these as different rows even when they describe the SAME real
//! strategy, silently splintering a pooled ranking the moment both sources coexist. [`OpeningKind`]
//! is the single closed target both producers cast into, so pooling is exact-match on a shared
//! small vocabulary instead of on two free-text dialects.
//!
//! ## The verified aoestats vocabulary (STEP 0, live DB, `match_players.opening`, aoestats-only
//! today — 108.3M rows, 93.3M `NULL`)
//! ```text
//! fast_castle   4,200,019      trash        1,099,506
//! unknown       3,589,570      fires          360,897
//! scouts        2,117,242      drush          238,166
//! archers       1,974,620      galleys        179,940
//! man_at_arms   1,092,875      towers         158,363
//! ```
//! Every one of those 9 REAL labels maps 1:1 onto an [`OpeningKind`] variant of the identical
//! name — no renaming, no reinterpretation, so the mapping is mechanical, not a guess. `"unknown"`
//! is aoestats' own explicit "tried to classify, couldn't" bucket — semantically the SAME honest
//! non-classification [`replay::derive::classify_opening`] represents as `None` — so it maps to
//! SQL `NULL`, never a 10th enum member (see [`crate::age`]'s "no defaults" rule: NULL for "nothing
//! honest to say", never a catch-all variant).
//!
//! ## `replay::derive::classify_opening`'s base tags (`replay::config::OPENER_LINES` +
//! `"Fast Castle"` + the `"Drush + "` prefix)
//! Six unit-line tags (`Scouts`/`Archers`/`Skirms`/`Eagles`/`M@A`/`Spears`), a `"Fast Castle"`
//! body, and a `"Drush + <body>"` composite. `Scouts`/`Archers`/`M@A` (renamed [`OpeningKind`]
//! variant: [`OpeningKind::ManAtArms`], matching aoestats' own spelling) and `Fast Castle` are the
//! SAME real strategies aoestats already names — reusing its exact vocabulary for those four (plus
//! `Drush`, see below) is the fix for the confirmed string-mismatch bug. `Skirms`/`Eagles`/`Spears`
//! have NO aoestats counterpart at all (aoestats' vocabulary above has no per-unit-line bucket
//! finer than `trash`) — kept as their OWN [`OpeningKind`] variants rather than folded into
//! `Trash`: aoestats never emits them, so there is no cross-source collision to avoid, and
//! asserting `trash == spears-or-skirms` would be an UNVERIFIED equivalence this crate has no
//! source for (no aoestats classifier source was found — see the task's STEP-0 report). Honesty
//! over completeness: two harmless unused-by-aoestats variants beat one fabricated equivalence.
//!
//! **`"Drush + X"` -> primary is `Drush`, not `X`.** aoestats has its OWN standalone `drush` label
//! (238,166 rows) that carries no information about what followed it — so the only way a replay's
//! `"Drush + Scouts"` can pool with aoestats' `drush` rows is if BOTH resolve to
//! [`OpeningKind::Drush`], regardless of the composite's body. This does lose the "into Scouts"
//! detail from the KIND (never from the rich `opening` TEXT, which still says the full composite).
//!
//! **`Trash`/`Fires`/`Galleys`/`Towers`** are aoestats-only [`OpeningKind`] variants (Trash Rush,
//! Fire Galley rush, Galley rush, Tower Rush — real, distinct, well-known AOE2 openings; see
//! `crate::opening`'s task report for the verification trail). `replay::derive::classify_opening`
//! does not detect any of these today (no building-construction or naval-unit signal in
//! `OPENER_LINES`), so no replay-derived match will ever carry one — that is an honest gap, not a
//! bug this enum needs to paper over: adding the variants now means a FUTURE replay classifier
//! extension has a slot to land in without another migration.

use std::fmt;

use serde::{Deserialize, Serialize};

/// One of the twelve real, closed opening/rush strategies [`OpeningKind`] pools
/// `match_players.opening`'s two producers onto. See the module doc for the full provenance +
/// reconciliation trail (STEP 0's live-DB vocabulary, the aoestats-verbatim renames, and the
/// deliberately-NOT-asserted `trash`/`spears`/`skirms` equivalence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpeningKind {
    Scouts,
    Archers,
    Skirms,
    Eagles,
    /// DB string `"man_at_arms"` — aoestats' own spelling; `replay::config::OPENER_LINES`'s `M@A`
    /// tag reconciles onto this SAME variant (see the module doc).
    ManAtArms,
    Spears,
    FastCastle,
    Drush,
    Trash,
    Fires,
    Galleys,
    Towers,
}

impl OpeningKind {
    /// The exact lowercase `snake_case` string this variant reads/writes as in the
    /// `opening_kind` Postgres enum — MUST match both the `#[serde(rename_all = "snake_case")]`
    /// wire form and the DB value (verified by [`tests::round_trips_every_variant`]).
    pub const fn as_db_str(self) -> &'static str {
        match self {
            OpeningKind::Scouts => "scouts",
            OpeningKind::Archers => "archers",
            OpeningKind::Skirms => "skirms",
            OpeningKind::Eagles => "eagles",
            OpeningKind::ManAtArms => "man_at_arms",
            OpeningKind::Spears => "spears",
            OpeningKind::FastCastle => "fast_castle",
            OpeningKind::Drush => "drush",
            OpeningKind::Trash => "trash",
            OpeningKind::Fires => "fires",
            OpeningKind::Galleys => "galleys",
            OpeningKind::Towers => "towers",
        }
    }
}

impl fmt::Display for OpeningKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_db_str())
    }
}

/// An opening-kind string outside the closed twelve-value vocabulary (see the module doc for why
/// this must fail loud rather than silently default/drop/guess).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "unknown opening_kind {0:?} (must be one of: scouts, archers, skirms, eagles, man_at_arms, \
     spears, fast_castle, drush, trash, fires, galleys, towers)"
)]
pub struct UnknownOpeningKind(pub String);

impl TryFrom<&str> for OpeningKind {
    type Error = UnknownOpeningKind;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "scouts" => Ok(OpeningKind::Scouts),
            "archers" => Ok(OpeningKind::Archers),
            "skirms" => Ok(OpeningKind::Skirms),
            "eagles" => Ok(OpeningKind::Eagles),
            "man_at_arms" => Ok(OpeningKind::ManAtArms),
            "spears" => Ok(OpeningKind::Spears),
            "fast_castle" => Ok(OpeningKind::FastCastle),
            "drush" => Ok(OpeningKind::Drush),
            "trash" => Ok(OpeningKind::Trash),
            "fires" => Ok(OpeningKind::Fires),
            "galleys" => Ok(OpeningKind::Galleys),
            "towers" => Ok(OpeningKind::Towers),
            other => Err(UnknownOpeningKind(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [OpeningKind; 12] = [
        OpeningKind::Scouts,
        OpeningKind::Archers,
        OpeningKind::Skirms,
        OpeningKind::Eagles,
        OpeningKind::ManAtArms,
        OpeningKind::Spears,
        OpeningKind::FastCastle,
        OpeningKind::Drush,
        OpeningKind::Trash,
        OpeningKind::Fires,
        OpeningKind::Galleys,
        OpeningKind::Towers,
    ];

    #[test]
    fn round_trips_every_variant() {
        for kind in ALL {
            let s = kind.as_db_str();
            assert_eq!(kind.to_string(), s);
            assert_eq!(OpeningKind::try_from(s), Ok(kind));
            assert_eq!(serde_json::to_string(&kind).unwrap(), format!("\"{s}\""));
            assert_eq!(
                serde_json::from_str::<OpeningKind>(&format!("\"{s}\"")).unwrap(),
                kind
            );
        }
    }

    /// The four aoestats-only labels this crate does NOT assert any replay-tag equivalence for —
    /// see the module doc's "Trash/Fires/Galleys/Towers" note. Pinned here so an accidental future
    /// rename/merge of one of these variants is a visible diff, not a silent vocabulary change.
    #[test]
    fn aoestats_only_labels_are_their_own_variants() {
        assert_eq!(OpeningKind::try_from("trash"), Ok(OpeningKind::Trash));
        assert_eq!(OpeningKind::try_from("fires"), Ok(OpeningKind::Fires));
        assert_eq!(OpeningKind::try_from("galleys"), Ok(OpeningKind::Galleys));
        assert_eq!(OpeningKind::try_from("towers"), Ok(OpeningKind::Towers));
    }

    #[test]
    fn try_from_fails_loud_on_unknown_opening_kind() {
        assert_eq!(
            OpeningKind::try_from("unknown"),
            Err(UnknownOpeningKind("unknown".to_owned())),
            "aoestats' own 'unknown' label is NOT a 13th variant — it maps to SQL NULL at the \
             call site (see the module doc), never a guessed/fabricated enum member"
        );
        assert_eq!(
            OpeningKind::try_from("Scouts"),
            Err(UnknownOpeningKind("Scouts".to_owned())),
            "must be case-sensitive lowercase, matching the DB values exactly"
        );
    }
}
