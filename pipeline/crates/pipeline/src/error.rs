//! `pipeline::Error` — the closed failure surface of [`crate::compose::to_batch`].
//!
//! Every variant is a fail-loud rejection of something [`crate::compose::to_batch`] refuses to
//! guess: a replay/seed pair that disagree on which match they describe, a replay with no map
//! (impossible for a real captured game, so a sign of corrupt/unusual input), or a
//! [`RelicMatchType`] this crate doesn't yet know how to file under the DB `ladder` enum (see
//! [`crate::compose::ladder_for`]'s doc for why that last variant can exist at all despite only
//! two `RelicMatchType` values existing today).

use thiserror::Error;

use fetch::RelicMatchType;
use pipeline_core::MatchId;

/// [`crate::compose::to_batch`]'s result alias.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    /// The parsed replay's own `match_id` disagrees with the discovery seed it was paired with —
    /// almost certainly a caller bug (the wrong seed handed to the wrong replay). Never coerced to
    /// either side.
    #[error("parsed replay match_id {parsed} does not match discovery seed match_id {seed}")]
    MismatchedMatchId { parsed: MatchId, seed: MatchId },

    /// `matches.map_id` is `NOT NULL`, but `replay::parse` leaves `ParsedReplay.map_id` `None`
    /// when a replay's map id didn't resolve. A real captured game always has a map, so this
    /// signals a corrupt/unusual replay — never defaulted to a placeholder map id.
    #[error("match {0}: replay carries no map_id (matches.map_id is NOT NULL)")]
    MissingMapId(MatchId),

    /// The seed's [`RelicMatchType`] doesn't map to a known DB `ladder`. `RelicMatchType` is
    /// `#[non_exhaustive]` (Relic vocab this workspace doesn't control); this variant is the
    /// fail-loud arm the "no defaults, fail loud" rule requires for it — never guess a ladder, as
    /// a wrong one silently mislabels every match under it (the civ-id-map-vs-relic-civ-id-map
    /// bug, one level up).
    #[error("match {0}: relic match type {1:?} has no known db ladder mapping")]
    UnmappedMatchType(MatchId, RelicMatchType),
}
