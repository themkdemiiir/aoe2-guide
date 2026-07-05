//! [`to_batch`] — the pure join of a parsed replay's gameplay with its discovery-seed metadata
//! into an [`ingest::ReplayBatch`]. See the crate doc for why this is the one place `replay` +
//! `fetch` + `ingest` compose.
//!
//! ## Ladder mapping (resolved from source, not guessed)
//! See [`ladder_for`]'s doc.
//!
//! ## Elo-fallback (resolved from source, not guessed)
//! The brief's plan was: "a parsed replay carries its own elo (post-game leaderboard block);
//! carry the API `newrating` in the discovery seed as the value for replays that didn't record a
//! post-game block" — i.e. per player, `elo = player.elo (post-game) OR the seed's rating`.
//!
//! Investigating `fetch::seed`/`fetch::api` to resolve which case applies:
//! - `fetch::api::normalize_recent` builds `DiscoverySeed.new_rating` via
//!   `let me = m.matchhistorymember.iter().find(|x| x.profile_id == profile_id.0); let new_rating
//!   = me.and_then(|x| x.newrating.or(x.oldrating));` — i.e. the discovery API's
//!   `matchhistorymember` array DOES carry a rating per participant, but `normalize_recent`
//!   immediately collapses it down to the rating for exactly ONE profile: the `profile_id`
//!   parameter `discover_recent`/`discover_recent_at` was called with (the "queried"/tracked
//!   profile, matched via `x.profile_id == profile_id.0`). So: **only one player's rating is
//!   available** — the brief's second branch.
//! - BUT: `fetch::seed::DiscoverySeed` (`match_id`, `match_type`, `played_at`, `new_rating`) does
//!   NOT retain *which* profile that rating belongs to — the `profile_id` used to select "me" is
//!   a parameter of `discover_recent`, never carried onto the `DiscoverySeed` value itself. The
//!   brief's instruction for this branch — "apply the fallback ONLY to that player (match by
//!   profile_id)" — therefore cannot be implemented against `to_batch`'s prescribed two-argument
//!   signature `(parsed: ParsedReplay, seed: DiscoverySeed)`: there is no join key in either input
//!   that identifies which `ParsedReplay` player the seed's `new_rating` is for.
//! - Resolution: rather than guess (e.g. "apply it to whichever player is missing post-game
//!   elo", which silently mis-assigns a rating in a 1v1 whenever it's a coin flip, or blindly
//!   apply to player 1) — the same "no defaults, fail loud" rule that produced the
//!   civ-id-map-vs-relic-civ-id-map bug fix — `to_batch` uses ONLY `player.elo` (the replay's own
//!   post-game leaderboard block, confirmed by `replay::postgame::collect_leaderboard_elo` to be
//!   populated per-player for every player in the match's recorded leaderboard, not just one) and
//!   leaves `elo = None` when that's absent. `seed.new_rating` is currently UNUSED by `to_batch`.
//! - Flagged follow-up: `fetch::DiscoverySeed` should grow a `profile_id: ProfileId` field (the
//!   "queried" profile `new_rating` is for), after which a revised `to_batch` can match it against
//!   `ParsedReplay.players` by `profile_id` and apply the fallback correctly for exactly that one
//!   player.

use fetch::{DiscoverySeed, RelicMatchType};
use ingest::{
    Ladder, MatchSource, NewMatch, NewMatchPlayer, NewReplayAge, NewReplayEvent, ReplayBatch,
};
use replay::ParsedReplay;

use crate::error::{Error, Result};

/// Maps [`RelicMatchType`] (Relic's own ranked-mode vocabulary) to the DB `ladder` enum.
///
/// Evidence (`fetch::seed::RelicMatchType`'s doc + `from_matchtype_id`, itself sourced from
/// `scripts/data-pipeline/lib/relic-map.mjs`'s `RM_1V1_LADDER = 6` / `RM_TEAM_LADDERS = {7, 8,
/// 9}`): `matchtype_id` 6 is `SOLO_RM_RANKED` -> [`Ladder::OneVOne`]; 7/8/9 are `TEAM_RM_RANKED`
/// (2v2/3v3/4v4, deliberately undifferentiated — see that enum's doc for why splitting them would
/// be inventing an unsourced fact) -> [`Ladder::Team`].
///
/// `RelicMatchType` is `#[non_exhaustive]` (a Relic vocabulary this workspace doesn't control), so
/// the compiler REQUIRES a wildcard arm here even though only two variants exist today — that arm
/// returns [`Error::UnmappedMatchType`] rather than guessing. There is currently no way to
/// construct a third `RelicMatchType` value from outside `fetch` (unit-only variants are still
/// only the ones actually declared), so that arm is not exercisable by a live unit test today; it
/// exists so a future `fetch` variant is a fail-loud runtime rejection here instead of a silent
/// ladder mislabel, and so removing the arm is a compile error the moment a third variant lands.
fn ladder_for(match_id: pipeline_core::MatchId, match_type: RelicMatchType) -> Result<Ladder> {
    match match_type {
        RelicMatchType::SoloRmRanked => Ok(Ladder::OneVOne),
        RelicMatchType::TeamRmRanked => Ok(Ladder::Team),
        other => Err(Error::UnmappedMatchType(match_id, other)),
    }
}

/// Joins one parsed replay's gameplay with its discovery seed into an [`ingest::ReplayBatch`]
/// (one match + its players/events/ages). Pure — no IO, no fabricated values. See the module doc
/// for the resolved ladder-mapping and elo-fallback decisions.
///
/// # Errors
/// - [`Error::MismatchedMatchId`] if `parsed.match_id != seed.match_id`.
/// - [`Error::MissingMapId`] if `parsed.map_id` is `None` (`matches.map_id` is `NOT NULL`).
/// - [`Error::UnmappedMatchType`] if `seed.match_type` has no known DB `ladder`.
pub fn to_batch(parsed: ParsedReplay, seed: DiscoverySeed) -> Result<ReplayBatch> {
    if parsed.match_id != seed.match_id {
        return Err(Error::MismatchedMatchId {
            parsed: parsed.match_id,
            seed: seed.match_id,
        });
    }

    let map_id = parsed.map_id.ok_or(Error::MissingMapId(parsed.match_id))?;
    let ladder = ladder_for(parsed.match_id, seed.match_type)?;
    let match_id = parsed.match_id;

    let new_match = NewMatch {
        match_id,
        source: MatchSource::Replay,
        ladder,
        map_id,
        build: parsed.build,
        // Not known here — a replay never records its own patch version; a later producer
        // (aoestats ETL, or a patch-index join) fills this in.
        patch: None,
        played_at: seed.played_at,
        duration_ms: Some(parsed.duration_ms),
        n_players: parsed.n_players,
    };

    let players = parsed
        .players
        .into_iter()
        .map(|p| NewMatchPlayer {
            match_id,
            profile_id: p.profile_id,
            civ_id: p.civ_id,
            // Elo-fallback (resolved): only the replay's own post-game elo. `seed.new_rating` is
            // NOT applied — see the module doc's "Elo-fallback" section for why.
            elo: p.elo,
            won: p.won,
            // The parser doesn't emit these yet — leave null, never fabricate.
            opening: None,
            feudal_t: None,
            castle_t: None,
            imperial_t: None,
        })
        .collect();

    let events = parsed
        .events
        .into_iter()
        .map(|e| NewReplayEvent {
            match_id,
            profile_id: e.profile_id,
            player_number: e.player_number,
            t_ms: e.t_ms,
            kind: e.kind,
            target_id: e.target_id,
            amount: e.amount,
            detail: e.detail,
        })
        .collect();

    let ages = parsed
        .ages
        .into_iter()
        .map(|a| NewReplayAge {
            match_id,
            profile_id: a.profile_id,
            civ_id: a.civ_id,
            won: a.won,
            age: a.age,
            uptime_ms: a.uptime_ms,
            villagers: Some(a.villagers),
            military: Some(a.military),
            n_buildings: Some(a.n_buildings),
            n_research: Some(a.n_research),
        })
        .collect();

    Ok(ReplayBatch {
        matches: vec![new_match],
        players,
        events,
        ages,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use pipeline_core::{Age, GameCivId, MatchId, ProfileId};
    use replay::{ReplayAge, ReplayEvent, ReplayPlayer};

    use super::*;

    fn sample_seed(
        match_id: i64,
        match_type: RelicMatchType,
        new_rating: Option<i32>,
    ) -> DiscoverySeed {
        DiscoverySeed {
            match_id: MatchId(match_id),
            match_type,
            played_at: Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap(),
            new_rating,
        }
    }

    fn sample_player(
        profile_id: i64,
        civ_id: i32,
        won: Option<bool>,
        elo: Option<i32>,
    ) -> ReplayPlayer {
        ReplayPlayer {
            player_number: 1,
            profile_id: ProfileId(profile_id),
            civ_id: GameCivId(civ_id),
            name: "Player".to_owned(),
            team: 1,
            color: 1,
            won,
            elo,
        }
    }

    fn sample_event(profile_id: i64) -> ReplayEvent {
        ReplayEvent {
            profile_id: Some(ProfileId(profile_id)),
            player_number: 1,
            t_ms: 1_000,
            kind: "build".to_owned(),
            target_id: Some(42),
            amount: None,
            detail: None,
        }
    }

    fn sample_age(profile_id: i64, civ_id: i32) -> ReplayAge {
        ReplayAge {
            profile_id: ProfileId(profile_id),
            civ_id: GameCivId(civ_id),
            won: Some(true),
            age: Age::Feudal,
            uptime_ms: 60_000,
            villagers: 20,
            military: 0,
            n_buildings: 5,
            n_research: 2,
        }
    }

    fn sample_parsed(
        match_id: i64,
        map_id: Option<i32>,
        players: Vec<ReplayPlayer>,
    ) -> ParsedReplay {
        let n_players = players.len() as i16;
        ParsedReplay {
            match_id: MatchId(match_id),
            build: Some(101_101),
            map_id,
            duration_ms: 1_800_000,
            n_players,
            players,
            events: vec![sample_event(5001)],
            ages: vec![sample_age(5001, 1)],
        }
    }

    #[test]
    fn to_batch_maps_full_replay_and_seed_into_expected_batch() {
        let players = vec![
            sample_player(5001, 1, Some(true), Some(1650)),
            sample_player(5002, 2, Some(false), Some(1590)),
        ];
        let parsed = sample_parsed(1001, Some(9), players);
        let seed = sample_seed(1001, RelicMatchType::SoloRmRanked, Some(1700));

        let batch = to_batch(parsed, seed.clone()).expect("valid replay+seed must map");

        assert_eq!(batch.matches.len(), 1);
        let m = &batch.matches[0];
        assert_eq!(m.match_id, MatchId(1001));
        assert_eq!(m.source, MatchSource::Replay);
        assert_eq!(
            m.ladder,
            Ladder::OneVOne,
            "matchtype 6 (SoloRmRanked) -> 1v1"
        );
        assert_eq!(m.map_id, 9);
        assert_eq!(
            m.played_at, seed.played_at,
            "played_at comes from the seed, not the replay"
        );
        assert_eq!(m.build, Some(101_101));
        assert_eq!(m.duration_ms, Some(1_800_000));
        assert_eq!(m.n_players, 2);
        assert_eq!(m.patch, None, "patch is not known at this layer");

        assert_eq!(batch.players.len(), 2);
        assert_eq!(batch.players[0].match_id, MatchId(1001));
        assert_eq!(batch.players[0].elo, Some(1650));
        assert_eq!(batch.players[1].elo, Some(1590));
        assert!(
            batch.players.iter().all(|p| p.opening.is_none()
                && p.feudal_t.is_none()
                && p.castle_t.is_none()
                && p.imperial_t.is_none()),
            "the parser doesn't emit opening/timings yet — must stay null, never fabricated"
        );

        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0].match_id, MatchId(1001));
        assert_eq!(batch.events[0].kind, "build");

        assert_eq!(batch.ages.len(), 1);
        assert_eq!(batch.ages[0].match_id, MatchId(1001));
        assert_eq!(batch.ages[0].villagers, Some(20));
    }

    #[test]
    fn to_batch_maps_team_ranked_to_team_ladder() {
        let parsed = sample_parsed(2002, Some(11), vec![sample_player(6001, 3, None, None)]);
        let seed = sample_seed(2002, RelicMatchType::TeamRmRanked, None);

        let batch = to_batch(parsed, seed).expect("valid replay+seed must map");
        assert_eq!(
            batch.matches[0].ladder,
            Ladder::Team,
            "matchtype 7/8/9 (TeamRmRanked) -> team"
        );
    }

    #[test]
    fn to_batch_fails_loud_on_missing_map_id() {
        let parsed = sample_parsed(3003, None, vec![sample_player(7001, 1, None, None)]);
        let seed = sample_seed(3003, RelicMatchType::SoloRmRanked, None);

        let err = to_batch(parsed, seed).unwrap_err();
        assert_eq!(
            err,
            Error::MissingMapId(MatchId(3003)),
            "a real replay always has a map — None must fail loud, never default"
        );
    }

    #[test]
    fn to_batch_fails_loud_on_mismatched_match_id() {
        let parsed = sample_parsed(4004, Some(9), vec![sample_player(8001, 1, None, None)]);
        let seed = sample_seed(9999, RelicMatchType::SoloRmRanked, None);

        let err = to_batch(parsed, seed).unwrap_err();
        assert_eq!(
            err,
            Error::MismatchedMatchId {
                parsed: MatchId(4004),
                seed: MatchId(9999),
            }
        );
    }

    #[test]
    fn elo_fallback_prefers_post_game_elo_and_never_fabricates_from_seed_rating() {
        // Player 1 has a post-game elo -> used verbatim, even though the seed carries a
        // DIFFERENT rating (proves post-game wins, is never overwritten by the seed).
        // Player 2 has NO post-game elo -> stays None: `DiscoverySeed` carries no `profile_id`,
        // so `to_batch` cannot safely attribute `seed.new_rating` to a *specific* player without
        // guessing (see the module doc's resolved elo-fallback decision) — honest absence, never
        // a fabricated value.
        let players = vec![
            sample_player(5001, 1, Some(true), Some(1650)),
            sample_player(5002, 2, Some(false), None),
        ];
        let parsed = sample_parsed(5005, Some(9), players);
        let seed = sample_seed(5005, RelicMatchType::SoloRmRanked, Some(9999));

        let batch = to_batch(parsed, seed).expect("valid replay+seed must map");
        assert_eq!(
            batch.players[0].elo,
            Some(1650),
            "post-game elo wins over the seed's rating"
        );
        assert_eq!(
            batch.players[1].elo, None,
            "no post-game elo and no safe attribution of seed.new_rating -> honest None"
        );
    }
}
