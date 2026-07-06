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
//!
//! ## Build-order + age-up-completion derivation (Phase A enrichment, `task-enrichA`)
//! `replay::derive(&parsed)` (pure, ported from `analyzer`'s opening/age-timing algorithm — see
//! its module doc) computes each player's `opening`/`opening_kind`/`feudal_t`/`castle_t`/
//! `imperial_t` from the replay's own events, BEFORE `parsed.players`/`parsed.events` are consumed
//! below. Results are matched back onto each `NewMatchPlayer` by `profile_id`. `opening_kind`
//! (final-review finding #1) is the closed, cross-source-reconciled counterpart to the rich
//! `opening` string — see `pipeline_core::opening`'s module doc.
//!
//! ## Unit composition derivation (Phase B enrichment, `task-enrichB`)
//! The SAME `replay::derive(&parsed)` call also fills each `PlayerSummary.units` (per-unit
//! `trained` totals — see that module's doc for the honest "queued, not surviving" metric
//! definition). Since Phase B, `replay::derive` is fallible (a `train` event's unit_id/summed
//! amount could in principle overflow `i32` — unreachable with real data, but never silently
//! narrowed), so its `Err` is mapped to [`Error::UnitComposition`] and propagated with `?`. Each
//! `PlayerSummary`'s `units` become `NewMatchPlayerUnit` rows on `ReplayBatch.player_units`,
//! matched to their player by the SAME `profile_id` key `summaries` is already built on — no
//! separate join needed.
//!
//! ## APM derivation (Phase C enrichment, `task-enrichC`)
//! The SAME `replay::derive(&parsed)` call also fills each `PlayerSummary.apm` (commands-per-
//! minute — see that module's doc for the ported formula and the replay-only/never-`None`
//! basis). Matched onto `NewMatchPlayer.apm` by the SAME `profile_id` key as `opening`/the age
//! timings; the replay path always supplies `Some(..)`.
//!
//! ## Watched-tech CLICK timings derivation (Phase D enrichment, `task-enrichD`)
//! The SAME `replay::derive(&parsed)` call also fills each `PlayerSummary.techs` (per-watched-tech
//! first-research CLICK ms — see that module's "Tech-timings basis" doc note for the click-not-
//! completion rationale, distinct from `feudal_t`/`castle_t`/`imperial_t` above). Flattened into
//! `ReplayBatch.player_techs` (one `NewMatchPlayerTech` row per watched tech ACTUALLY researched),
//! matched to its player by the SAME `profile_id` key `player_units` uses — built in the SAME pass
//! as `player_units` (not a second separate `summaries.into_iter()`, which would try to consume
//! the already-moved `summaries` map twice).

use std::collections::HashMap;

use fetch::{DiscoverySeed, RelicMatchType};
use ingest::{
    Ladder, MatchSource, NewMatch, NewMatchPlayer, NewMatchPlayerTech, NewMatchPlayerUnit,
    NewReplayAge, NewReplayEvent, ReplayBatch,
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

    // Phase A + B enrichment (see the module doc) — computed from `&parsed` before
    // `parsed.players`/`parsed.events` are moved into the DTOs below. `derive` is fallible since
    // Phase B (unit_id/summed-amount `i32` overflow, unreachable with real data but never
    // silently narrowed); mapped to `Error::UnitComposition` rather than `replay::Error` so this
    // crate keeps its own closed failure surface.
    let summaries: HashMap<pipeline_core::ProfileId, replay::PlayerSummary> =
        replay::derive(&parsed)
            .map_err(|e| Error::UnitComposition(match_id, e))?
            .into_iter()
            .map(|s| (s.profile_id, s))
            .collect();

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
        .map(|p| {
            // Structurally always `Some` — `summaries` was built from this SAME `parsed.players`
            // set above — but matched by key (not index/zip) so a future change to either side
            // can't silently misalign player i with summary j. `None` (never fabricated) for any
            // field this player's summary didn't classify/reach.
            let derived = summaries.get(&p.profile_id);
            NewMatchPlayer {
                match_id,
                profile_id: p.profile_id,
                civ_id: p.civ_id,
                // Elo-fallback (resolved): only the replay's own post-game elo. `seed.new_rating`
                // is NOT applied — see the module doc's "Elo-fallback" section for why.
                elo: p.elo,
                won: p.won,
                // Phase A enrichment — see the module doc's "Build-order + age-up-completion
                // derivation" section. `feudal_t`/`castle_t`/`imperial_t` are COMPLETION seconds
                // (`replay::derive`'s doc), matching the aoestats path's `*_age_uptime` columns.
                opening: derived.and_then(|s| s.opening.clone()),
                // Closes final-review finding #1 — the closed counterpart to `opening` above (see
                // `pipeline_core::opening`'s module doc); `derived.and_then` mirrors `opening`'s
                // own None-propagation exactly (both come from the SAME `PlayerSummary`).
                opening_kind: derived.and_then(|s| s.opening_kind),
                feudal_t: derived.and_then(|s| s.feudal_t),
                castle_t: derived.and_then(|s| s.castle_t),
                imperial_t: derived.and_then(|s| s.imperial_t),
                // Phase C enrichment — see the module doc's "APM derivation" section. `derived`
                // is only `None` when `p.profile_id` structurally can't miss a summary (see the
                // comment above); `s.apm` itself is always `Some(..)` for a replay-derived
                // player (never fabricated `None` — see `replay::derive`'s module doc).
                apm: derived.and_then(|s| s.apm),
            }
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

    // Phase B enrichment (see the module doc's "Unit composition derivation" section) — consumes
    // `summaries` (its `players` borrow above already ended), flattening each player's
    // `units: Vec<(GameUnitId, i32)>` into one `NewMatchPlayerUnit` row per DISTINCT unit_id, and
    // (Phase D, see the module doc's "Watched-tech CLICK timings derivation" section) each
    // player's `techs: Vec<(TechId, i32)>` into one `NewMatchPlayerTech` row per watched tech
    // ACTUALLY researched. Both come from the SAME `summaries` map, built in ONE pass — a second
    // `summaries.into_iter()` call would try to consume the already-moved map twice, a compile
    // error. `profile_id` comes from `summaries`' own key, so both are already matched per-player
    // — no separate join.
    let mut player_units: Vec<NewMatchPlayerUnit> = Vec::new();
    let mut player_techs: Vec<NewMatchPlayerTech> = Vec::new();
    for (profile_id, summary) in summaries {
        player_units.extend(
            summary
                .units
                .into_iter()
                .map(|(unit_id, trained)| NewMatchPlayerUnit {
                    match_id,
                    profile_id,
                    unit_id,
                    trained,
                }),
        );
        player_techs.extend(
            summary
                .techs
                .into_iter()
                .map(|(tech_id, t_ms)| NewMatchPlayerTech {
                    match_id,
                    profile_id,
                    tech_id,
                    t_ms,
                }),
        );
    }

    Ok(ReplayBatch {
        matches: vec![new_match],
        players,
        events,
        ages,
        player_units,
        player_techs,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use pipeline_core::{Age, GameCivId, GameUnitId, MatchId, ProfileId, TechId};
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

    /// **Warning:** hardcodes `player_number: 1` for EVERY player it builds — fine for the
    /// single-player-number fixtures that use it, but do NOT reuse it for attribution-sensitive
    /// multi-player tests (`replay::derive` attributes events to a player by `player_number`, so
    /// two players sharing `player_number: 1` would silently collapse onto the same derived
    /// summary). Build literal `ReplayPlayer`s with distinct `player_number`s instead — see
    /// `to_batch_flows_derived_units_into_player_units` below for the pattern.
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
                && p.opening_kind.is_none()
                && p.feudal_t.is_none()
                && p.castle_t.is_none()
                && p.imperial_t.is_none()),
            "this sample carries no research events at all (only sample_event's `build` action) \
             -> derive() has nothing to classify, so opening/opening_kind/timings honestly stay \
             null; see `to_batch_flows_derived_opening_and_completion_timings_into_match_player` \
             for the populated case"
        );
        assert!(
            batch.players.iter().all(|p| p.apm.is_some()),
            "apm (Phase C) is always Some for a replay-derived player, unlike \
             opening/timings/units above — see `to_batch_flows_derived_units_into_player_units` \
             for the exact-value case"
        );

        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0].match_id, MatchId(1001));
        assert_eq!(batch.events[0].kind, "build");

        assert_eq!(batch.ages.len(), 1);
        assert_eq!(batch.ages[0].match_id, MatchId(1001));
        assert_eq!(batch.ages[0].villagers, Some(20));

        assert!(
            batch.player_units.is_empty(),
            "this sample's only event is a `build` action, not `train` -> no unit composition \
             to derive; see `to_batch_flows_derived_units_into_player_units` for the populated case"
        );
        assert!(
            batch.player_techs.is_empty(),
            "this sample's only event is a `build` action, not `research` -> no watched tech \
             to derive; see `to_batch_flows_derived_techs_into_player_techs` for the populated case"
        );
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

    #[test]
    fn to_batch_flows_derived_opening_and_completion_timings_into_match_player() {
        // Phase A enrichment (task-enrichA): a replay whose events DO carry an age-up research
        // + a feudal-window unit train must come out of `to_batch` with a populated opening and
        // a COMPLETION (not click) feudal_t — proving `replay::derive` is actually wired in.
        let mut parsed = sample_parsed(
            6006,
            Some(9),
            vec![sample_player(5001, 2, Some(true), Some(1650))], // civ_id 2 = franks (baseline)
        );
        parsed.events = vec![
            ReplayEvent {
                profile_id: Some(ProfileId(5001)),
                player_number: 1,
                t_ms: 600_000, // Feudal click at 10:00
                kind: "research".to_owned(),
                target_id: Some(101),
                amount: None,
                detail: None,
            },
            ReplayEvent {
                profile_id: Some(ProfileId(5001)),
                player_number: 1,
                t_ms: 620_000, // Scouts opened just after Feudal
                kind: "train".to_owned(),
                target_id: Some(448),
                amount: Some(1),
                detail: None,
            },
        ];
        let seed = sample_seed(6006, RelicMatchType::SoloRmRanked, None);

        let batch = to_batch(parsed, seed).expect("valid replay+seed must map");

        let p = &batch.players[0];
        assert_eq!(p.opening.as_deref(), Some("Scouts"));
        assert_eq!(
            p.opening_kind,
            Some(pipeline_core::OpeningKind::Scouts),
            "opening_kind (final-review finding #1) flows through to_batch alongside opening"
        );
        // COMPLETION, not click: 600.0s click + 130.0s baseline Feudal research = 730.0s. If
        // this ever regresses to storing the raw click (600.0), this assertion catches it.
        let feudal_t = p.feudal_t.expect("feudal was reached — must not be None");
        assert!(
            (feudal_t - 730.0).abs() < 0.01,
            "feudal_t={feudal_t} must be click+research (730.0), not raw click (600.0)"
        );
        assert_eq!(p.castle_t, None, "castle never reached -> honest None");
        assert_eq!(p.imperial_t, None, "imperial never reached -> honest None");

        // Phase C enrichment (task-enrichC): 2 raw commands (the research + the train above)
        // over the sample's 1_800_000ms (30-minute) duration -> apm = 2 / 30 events/minute.
        let apm = p.apm.expect("apm must always be Some for a replay-derived player");
        assert!(
            (apm - (2.0_f32 / 30.0)).abs() < 0.001,
            "apm={apm} must equal 2 events / 30 minutes"
        );

        // Phase B enrichment (task-enrichB): the same replay's lone `train` event (unit 448,
        // amount 1) must flow into `ReplayBatch.player_units` as one row.
        assert_eq!(batch.player_units.len(), 1);
        assert_eq!(batch.player_units[0].match_id, MatchId(6006));
        assert_eq!(batch.player_units[0].profile_id, ProfileId(5001));
        assert_eq!(batch.player_units[0].unit_id, GameUnitId(448));
        assert_eq!(batch.player_units[0].trained, 1);
    }

    #[test]
    fn to_batch_flows_derived_units_into_player_units() {
        // Phase B enrichment (task-enrichB): two players, each training a distinct unit across
        // multiple batches — proves `PlayerSummary.units` flows into `ReplayBatch.player_units`
        // as one `NewMatchPlayerUnit` per (player, distinct unit_id), matched by profile_id
        // (never mixed between players), with `trained` = the SUMMED amount, not the row count.
        //
        // Deliberately NOT `sample_player` (it hardcodes `player_number: 1` for every player,
        // fine for the single-player-number fixtures above but wrong here — `replay::derive`
        // attributes events to a player by `player_number`, so two players sharing player_number
        // 1 would silently collapse onto the same derived `units`). Distinct player_numbers (1,
        // 2) here, matching each event's own `player_number` below.
        let players = vec![
            ReplayPlayer {
                player_number: 1,
                profile_id: ProfileId(5001),
                civ_id: GameCivId(1),
                name: "Player".to_owned(),
                team: 1,
                color: 1,
                won: Some(true),
                elo: Some(1650),
            },
            ReplayPlayer {
                player_number: 2,
                profile_id: ProfileId(5002),
                civ_id: GameCivId(2),
                name: "Player".to_owned(),
                team: 2,
                color: 2,
                won: Some(false),
                elo: Some(1590),
            },
        ];
        let mut parsed = sample_parsed(7007, Some(9), players);
        parsed.events = vec![
            ReplayEvent {
                profile_id: Some(ProfileId(5001)),
                player_number: 1,
                t_ms: 100_000,
                kind: "train".to_owned(),
                target_id: Some(83),
                amount: Some(5),
                detail: None,
            },
            ReplayEvent {
                profile_id: Some(ProfileId(5001)),
                player_number: 1,
                t_ms: 150_000,
                kind: "train".to_owned(),
                target_id: Some(83),
                amount: Some(3),
                detail: None,
            },
            ReplayEvent {
                profile_id: Some(ProfileId(5002)),
                player_number: 2,
                t_ms: 100_000,
                kind: "train".to_owned(),
                target_id: Some(448),
                amount: Some(2),
                detail: None,
            },
        ];
        let seed = sample_seed(7007, RelicMatchType::SoloRmRanked, None);

        let batch = to_batch(parsed, seed).expect("valid replay+seed must map");

        assert_eq!(
            batch.player_units.len(),
            2,
            "one row per (player, distinct unit_id)"
        );
        let p5001 = batch
            .player_units
            .iter()
            .find(|u| u.profile_id == ProfileId(5001))
            .expect("player 5001 must have a unit row");
        assert_eq!(p5001.match_id, MatchId(7007));
        assert_eq!(p5001.unit_id, GameUnitId(83));
        assert_eq!(p5001.trained, 8, "5 + 3 = 8, summed not row-counted");

        let p5002 = batch
            .player_units
            .iter()
            .find(|u| u.profile_id == ProfileId(5002))
            .expect("player 5002 must have a unit row");
        assert_eq!(p5002.unit_id, GameUnitId(448));
        assert_eq!(
            p5002.trained, 2,
            "player 5002's train must not leak into player 5001's total"
        );

        // Phase C enrichment (task-enrichC): apm flows into `NewMatchPlayer`, matched by
        // profile_id, and is COUNT-based (raw commands), not amount-summed — player 5001's two
        // `train` commands (batch amounts 5 and 3) count as 2 raw events, NOT 8 (the `trained`
        // total above); player 5002's one command counts as 1, not 2. Both over the sample's
        // 1_800_000ms (30-minute) duration.
        let player5001 = batch
            .players
            .iter()
            .find(|p| p.profile_id == ProfileId(5001))
            .expect("player 5001 must be present");
        let player5002 = batch
            .players
            .iter()
            .find(|p| p.profile_id == ProfileId(5002))
            .expect("player 5002 must be present");
        let apm5001 = player5001.apm.expect("apm must always be Some");
        let apm5002 = player5002.apm.expect("apm must always be Some");
        assert!(
            (apm5001 - (2.0_f32 / 30.0)).abs() < 0.001,
            "apm5001={apm5001} must be 2 raw commands / 30 minutes, not amount-summed (8/30)"
        );
        assert!(
            (apm5002 - (1.0_f32 / 30.0)).abs() < 0.001,
            "apm5002={apm5002} must be 1 raw command / 30 minutes, not amount-summed (2/30), \
             and must not leak player 5001's commands"
        );
    }

    #[test]
    fn to_batch_flows_derived_techs_into_player_techs() {
        // Phase D enrichment (task-enrichD): two players, each researching a distinct watched
        // tech (plus one non-watched research and one duplicate) — proves `PlayerSummary.techs`
        // flows into `ReplayBatch.player_techs` as one `NewMatchPlayerTech` per (player, watched
        // tech ACTUALLY researched), matched by profile_id (never mixed between players), with
        // `t_ms` = the CLICK (min) time, not the later duplicate.
        //
        // Deliberately NOT `sample_player` (hardcodes `player_number: 1` for every player) —
        // distinct player_numbers (1, 2) here, matching each event's own `player_number` below;
        // see `to_batch_flows_derived_units_into_player_units`'s comment for why.
        let players = vec![
            ReplayPlayer {
                player_number: 1,
                profile_id: ProfileId(5001),
                civ_id: GameCivId(1),
                name: "Player".to_owned(),
                team: 1,
                color: 1,
                won: Some(true),
                elo: Some(1650),
            },
            ReplayPlayer {
                player_number: 2,
                profile_id: ProfileId(5002),
                civ_id: GameCivId(2),
                name: "Player".to_owned(),
                team: 2,
                color: 2,
                won: Some(false),
                elo: Some(1590),
            },
        ];
        let mut parsed = sample_parsed(8008, Some(9), players);
        parsed.events = vec![
            ReplayEvent {
                profile_id: Some(ProfileId(5001)),
                player_number: 1,
                t_ms: 10_000,
                kind: "research".to_owned(),
                target_id: Some(22), // Loom, first click
                amount: None,
                detail: None,
            },
            ReplayEvent {
                profile_id: Some(ProfileId(5001)),
                player_number: 1,
                t_ms: 20_000,
                kind: "research".to_owned(),
                target_id: Some(22), // Loom again — later duplicate, must NOT win
                amount: None,
                detail: None,
            },
            ReplayEvent {
                profile_id: Some(ProfileId(5001)),
                player_number: 1,
                t_ms: 5_000,
                kind: "research".to_owned(),
                target_id: Some(999), // not watched -> ignored
                amount: None,
                detail: None,
            },
            ReplayEvent {
                profile_id: Some(ProfileId(5002)),
                player_number: 2,
                t_ms: 15_000,
                kind: "research".to_owned(),
                target_id: Some(213), // Wheelbarrow
                amount: None,
                detail: None,
            },
        ];
        let seed = sample_seed(8008, RelicMatchType::SoloRmRanked, None);

        let batch = to_batch(parsed, seed).expect("valid replay+seed must map");

        assert_eq!(
            batch.player_techs.len(),
            2,
            "one row per (player, watched tech actually researched); the non-watched tech (999) \
             and the duplicate Loom research must not add extra rows"
        );
        let p5001 = batch
            .player_techs
            .iter()
            .find(|t| t.profile_id == ProfileId(5001))
            .expect("player 5001 must have a tech row");
        assert_eq!(p5001.match_id, MatchId(8008));
        assert_eq!(p5001.tech_id, TechId(22));
        assert_eq!(
            p5001.t_ms, 10_000,
            "CLICK time is the MIN over duplicates (10_000), not the later 20_000"
        );

        let p5002 = batch
            .player_techs
            .iter()
            .find(|t| t.profile_id == ProfileId(5002))
            .expect("player 5002 must have a tech row");
        assert_eq!(p5002.tech_id, TechId(213));
        assert_eq!(
            p5002.t_ms, 15_000,
            "player 5002's tech must not leak into player 5001's row"
        );
    }
}
