//! [`DiscoverySeed`] + [`RelicMatchType`] — the per-match facts a REPLAY can't carry, learned
//! from the discovery API.
//!
//! A parsed `.aoe2record` knows gameplay (players, civs, events, ages) but NOT `source`, `ladder`,
//! or `played_at` — those are properties of *how the match was discovered*, which only the Relic
//! API knows. The seed captures them at discovery time so 4d can supply them to `ingest` (whose
//! `NewMatch` requires all three). Keeping the seed + the Relic ladder vocab here — not in
//! `ingest` — keeps `fetch` decoupled: `fetch` produces seeds; the seed→DB `ladder`/`source`
//! mapping is 4d's job.

use chrono::{DateTime, Utc};
use thiserror::Error;

use pipeline_core::MatchId;

/// The classified RANKED match mode, in Relic's own vocabulary (which we don't control).
///
/// `#[non_exhaustive]` on purpose — this is the one place the playbook sanctions it: an enum
/// mirroring an external vocab we don't own, where a future Relic ladder id must force every match
/// site to acknowledge it rather than silently fall through. (Our OWN closed enums — `Age`,
/// `MatchStatus` — are deliberately exhaustive, the opposite choice.)
///
/// Deliberately only TWO variants, not four (`SoloRmRanked`/`Team2v2`/`Team3v3`/`Team4v4`): every
/// source below groups `matchtype_id` 7/8/9 as one undifferentiated "team RM ranked" set and never
/// pins which id is which team size. Splitting them into three named variants would be inventing an
/// unverified fact (see the repo's "source-derived only" rule) — a future task that sources the
/// exact id -> team-size mapping can split `TeamRmRanked` further then.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RelicMatchType {
    /// `matchtype_id` 6 — `SOLO_RM_RANKED`.
    SoloRmRanked,
    /// `matchtype_id` 7, 8, or 9 — `TEAM_RM_RANKED` (2v2/3v3/4v4, undifferentiated; see the enum
    /// doc).
    TeamRmRanked,
}

impl RelicMatchType {
    /// Classifies a `getRecentMatchHistory` match-history-entry's `matchtype_id` field. Fails loud
    /// on any id outside the known ranked set (6/7/8/9) — most `matchtype_id`s are NOT ranked RM
    /// (empire wars, death match, quick play, events, custom lobbies, ...), so this is expected to
    /// reject most rows, not a data-corruption signal on its own; the caller logs + skips (never
    /// silently coerces — the civ-id-bug lesson applied to ladder ids).
    ///
    /// Source: `scripts/data-pipeline/lib/relic-map.mjs` (`RM_1V1_LADDER = 6`,
    /// `RM_TEAM_LADDERS = {7, 8, 9}`) and `scripts/data-pipeline/lib/relic-api.mjs`'s
    /// `normalizeMatches` (`ladder: m.matchtype_id`, same comment: "6 = 1v1 RM, 7/8/9 = team RM,
    /// rest = other modes") — both read this exact field off this exact endpoint.
    ///
    /// NOT to be confused with `getLeaderBoard2`'s OWN, DIFFERENT `leaderboard_id` space (see
    /// `relic-api.mjs`'s `LEADERBOARD_1V1_RM = 3` / `LEADERBOARD_TEAM_RM = 4`, used only for
    /// paginating a ladder — a wholly separate id space this crate does not touch). Conflating the
    /// two would be exactly the civ-id-map.json vs relic-civ-id-map.json bug again, one level up.
    pub fn from_matchtype_id(matchtype_id: i32) -> Result<Self, UnknownMatchType> {
        match matchtype_id {
            6 => Ok(RelicMatchType::SoloRmRanked),
            7..=9 => Ok(RelicMatchType::TeamRmRanked),
            other => Err(UnknownMatchType(format!("matchtype_id {other}"))),
        }
    }

    /// Reconstruct the ranked mode from the DB `ladder_kind` label (`'1v1'` / `'team'`) — the
    /// inverse of `pipeline::compose::ladder_for`'s `RelicMatchType -> Ladder` mapping. Used by the
    /// replay BACKFILL (`pipeline backfill`), which rebuilds a [`DiscoverySeed`] for a match ALREADY
    /// in Postgres (whose `ladder` was set at aoestats-import time) rather than from a fresh
    /// `getRecentMatchHistory` discovery call, so it must turn the stored ladder back into the
    /// `RelicMatchType` that `to_batch` consumes. This constructor lives HERE (not in `pipeline`)
    /// precisely because `RelicMatchType` is `#[non_exhaustive]`, so its variants cannot be
    /// constructed outside this crate. Fails loud on any label outside the DB `ladder_kind` enum's
    /// two values — the same no-defaults/no-guess discipline as [`Self::from_matchtype_id`] (a new
    /// ladder must be added deliberately, never silently coerced).
    pub fn from_db_ladder(ladder: &str) -> Result<Self, UnknownMatchType> {
        match ladder {
            "1v1" => Ok(RelicMatchType::SoloRmRanked),
            "team" => Ok(RelicMatchType::TeamRmRanked),
            other => Err(UnknownMatchType(format!("db ladder_kind {other}"))),
        }
    }
}

/// A `matchtype_id` outside the known ranked-RM vocabulary. Carries a human-readable description
/// of the rejected value (see [`RelicMatchType::from_matchtype_id`]).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("no ranked relic match type for {0}")]
pub struct UnknownMatchType(pub String);

/// One discovered match, ready to be handed to 4d alongside its (later) parsed replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoverySeed {
    /// The Relic match id (shared key with the manifest + the eventual `matches` row).
    pub match_id: MatchId,
    /// The classified ranked mode. 4d maps this to the DB `ladder` enum (`'1v1'`/`'team'`).
    pub match_type: RelicMatchType,
    /// When the match was played (Relic `completiontime`) — the `matches.played_at` value.
    pub played_at: DateTime<Utc>,
    /// The discovery-time rating (Relic `newrating`, falling back to `oldrating`). The elo
    /// FALLBACK for a replay whose post-game block is absent — `None` when the API had neither.
    pub new_rating: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matchtype_id_classifies_ranked_and_fails_loud_otherwise() {
        assert_eq!(
            RelicMatchType::from_matchtype_id(6),
            Ok(RelicMatchType::SoloRmRanked)
        );
        for team in [7, 8, 9] {
            assert_eq!(
                RelicMatchType::from_matchtype_id(team),
                Ok(RelicMatchType::TeamRmRanked)
            );
        }
        // Unranked / unknown ids never silently map — they fail loud. 2 and 18 are REAL
        // matchtype_ids probed off a live profile's non-ranked-RM automatch history (see
        // `discovery.rs`'s ported fixture) — not fabricated.
        for other in [0, 1, 2, 5, 10, 18, -1] {
            assert!(
                RelicMatchType::from_matchtype_id(other).is_err(),
                "matchtype_id {other} must not classify as a ranked mode"
            );
        }
    }

    #[test]
    fn from_db_ladder_inverts_ladder_for_and_fails_loud_otherwise() {
        // The two live DB `ladder_kind` values (see migration
        // `m20260705_000001_create_enums.rs`) invert `compose::ladder_for`'s forward mapping.
        assert_eq!(
            RelicMatchType::from_db_ladder("1v1"),
            Ok(RelicMatchType::SoloRmRanked)
        );
        assert_eq!(
            RelicMatchType::from_db_ladder("team"),
            Ok(RelicMatchType::TeamRmRanked)
        );
        // Anything outside the enum's two labels fails loud — never a guessed mode. Includes the
        // Relic `matchtype_id` numbers, which belong to a DIFFERENT vocabulary and must not leak in.
        for bad in ["", "1V1", "Team", "2v2", "3v3", "6", "solo", "ranked"] {
            assert!(
                RelicMatchType::from_db_ladder(bad).is_err(),
                "db ladder {bad:?} must not classify as a ranked mode"
            );
        }
    }
}
