//! Parse a replay with aoe2rec (in-process) into a [`ParsedReplay`].
//!
//! Faithful port of the OLD extractor's `scripts/data-pipeline/replay-rs/src/extract.rs`
//! (itself a faithful port of `scripts/data-pipeline/replays/extract.py`). The only behavioral
//! difference from the old `extract::extract`: every `usize`/`u32`/`u16`/`u8`/`i64` value the
//! replay hands back is narrowed to the DB-native `i32`/`i16` with a checked `try_from` at the
//! point it lands in a `Replay*` output field, instead of a silent `as` cast — a corrupt record
//! now returns [`Error::Overflow`] instead of silently truncating (real games never hit this).
//!
//! Raw ids are kept as-is; id -> name mapping happens later via the icon-map dim, so this parser
//! has no name dependencies and never loses information.
//!
//! [`parse`] never panics, even if the vendored `aoe2rec` decoder does: [`parse_savegame`] wraps
//! its one call to `Savegame::from_bytes` in `catch_unwind` and converts a caught panic into
//! `Error::Parse` — see that function's doc.

use std::collections::{HashMap, HashSet};
use std::panic::{self, AssertUnwindSafe};

use aoe2rec::actions::{ActionData, Game};
use aoe2rec::{Operation, Savegame};
use bytes::Bytes;
use pipeline_core::{Age, GameCivId, MatchId, ProfileId};

use crate::config;
use crate::error::{overflow, Error, Result};
use crate::postgame::{collect_leaderboard_elo, EloTable};
use crate::types::{ParsedReplay, ReplayAge, ReplayEvent, ReplayPlayer};

/// Real-player record keyed by `player_number` (skip gaia/empty: civ_id == 0). Field types match
/// `aoe2rec::header::Player`'s natively, widened where that's already infallible (`u8` ids) —
/// narrowed to the output types only once, at the bottom of [`parse`].
struct PlayerInfo {
    profile_id: i32,
    civ_id: u32,
    name: String,
    team: u8,
    color: i32,
}

/// One decoded action, mirroring the old extractor's `_classify` tuple. Kept in aoe2rec's own
/// (or widened-but-still-native) types; narrowed to the `Replay*` output types only at the
/// output boundary in [`parse`] — "parse, don't validate, at every port boundary."
struct Decoded {
    player_number: i32,
    kind: String,
    target_id: Option<i64>,
    amount: Option<i64>,
    detail: Option<String>,
}

/// Build action keeps a raw byte blob; the building id is int32 LE at `[12:16]`. This exact offset
/// was verified byte-for-byte against the old Python/Rust extractors' own `[12:16]` slice (never
/// independently re-derived here) and parity-checked against one real fixture replay in task 4b's
/// report (`.superpowers/sdd/task-4b-replay-report.md`, local/gitignored) — re-verify this offset
/// (and `pipeline/vendor/aoe2rec/VENDORED.md`'s pinned version) against a fresh fixture if a much
/// newer game build starts producing wrong `target_id`s for `Build` actions.
fn decode_build_id(data: &[u8]) -> Option<i64> {
    if data.len() < 16 {
        return None;
    }
    let raw: [u8; 4] = data[12..16].try_into().ok()?;
    Some(i64::from(i32::from_le_bytes(raw)))
}

/// Name of the `Game` sub-command (its serde variant tag), e.g. "Diplomacy".
fn game_command_name(g: &Game) -> Option<String> {
    // The Game enum is small and Serialize-tagged; reading the single top-level key gives the
    // variant name without a hand-written match over every arm.
    match serde_json::to_value(g).ok()? {
        serde_json::Value::Object(map) => map.into_iter().next().map(|(k, _)| k),
        serde_json::Value::String(s) => Some(s), // unit-like variants (e.g. Spy)
        _ => None,
    }
}

/// Lowercased variant name + player_id for *any* `ActionData`, read generically from its
/// Serialize tag. This is how the old extractor classified the long tail of actions (Move,
/// Interact, Gatherpoint, ...) without a 40-arm match. Split out from [`generic_kind_and_player`]
/// so it's unit-testable directly against hand-built `serde_json::Value`s — every ActionData
/// variant the CURRENT vendored `aoe2rec` defines carries a `player_id`, so the `None`/bare-string
/// arms below are unreachable with real data today; they exist for forward compat with a future
/// aoe2rec bump that adds a unit-like or player-less variant (see `classify`'s fallback arm, which
/// now fails loud rather than fabricating `player_number = 0` for that case).
fn kind_and_player_from_value(value: serde_json::Value) -> (String, Option<i64>) {
    match value {
        // Externally-tagged enum: exactly one top-level key = variant name.
        serde_json::Value::Object(map) => match map.into_iter().next() {
            Some((variant, body)) => {
                let pid = body.get("player_id").and_then(|v| v.as_i64());
                (variant.to_lowercase(), pid)
            }
            None => (String::new(), None),
        },
        // Unit-like variant (e.g. the nested `Game::Spy`) serializes as a bare string, same as
        // `game_command_name` already handles — no body, so never a player_id either.
        serde_json::Value::String(s) => (s.to_lowercase(), None),
        _ => (String::new(), None),
    }
}

/// Lowercased variant name + player_id for *any* `ActionData`, read generically from its
/// Serialize tag.
fn generic_kind_and_player(ad: &ActionData) -> (String, Option<i64>) {
    match serde_json::to_value(ad) {
        Ok(value) => kind_and_player_from_value(value),
        Err(_) => (String::new(), None),
    }
}

/// Resolves a decoded `(kind, pid)` pair's `pid` to the narrowed `i32` `Decoded::player_number` —
/// or fails loud (`Error::Parse`) when `pid` is genuinely absent, rather than fabricating `0` (a
/// real player_number the game could otherwise legitimately assign, unlike a true "unknown").
/// Unreachable with real data today (see [`kind_and_player_from_value`]'s doc); kept as its own
/// pure, directly-testable function rather than inlined into `classify`.
fn require_player_number(kind: &str, pid: Option<i64>) -> Result<i32> {
    match pid {
        Some(pid) => i32::try_from(pid).map_err(overflow("event.player_id")),
        None => Err(Error::Parse(format!(
            "action {kind:?} carries no resolvable player_id"
        ))),
    }
}

/// Map an `ActionData` -> `Decoded`. Faithful port of the old extractor's `classify`; the
/// generic fallback arm's `i64 -> i32` player-id narrowing is now checked (`try_from`) instead
/// of an `as` cast (every named arm's `player_id` is already a native `u8`, so those widen
/// infallibly via `i32::from`).
fn classify(ad: &ActionData) -> Result<Decoded> {
    Ok(match ad {
        ActionData::DeQueue {
            player_id,
            unit_id,
            amount,
            ..
        } => Decoded {
            player_number: i32::from(*player_id),
            kind: "train".into(),
            target_id: Some(i64::from(*unit_id)),
            // `amount or 1`: a 0 amount in the record still means one queued unit.
            amount: Some(if *amount == 0 { 1 } else { i64::from(*amount) }),
            detail: None,
        },
        ActionData::Research {
            player_id,
            technology_type,
            ..
        } => Decoded {
            player_number: i32::from(*player_id),
            kind: "research".into(),
            target_id: Some(i64::from(*technology_type)),
            amount: None,
            detail: None,
        },
        ActionData::Build {
            player_id, data, ..
        } => Decoded {
            player_number: i32::from(*player_id),
            kind: "build".into(),
            target_id: decode_build_id(data),
            amount: None,
            detail: None,
        },
        ActionData::Game {
            player_id,
            game_command,
            ..
        } => Decoded {
            player_number: i32::from(*player_id),
            kind: "game".into(),
            target_id: None,
            amount: None,
            detail: game_command_name(game_command),
        },
        ActionData::Order {
            player_id,
            order_type,
            ..
        } => Decoded {
            player_number: i32::from(*player_id),
            kind: "order".into(),
            target_id: None,
            amount: None,
            // order_type is a small repr enum; its serde tag is the name.
            detail: serde_json::to_value(order_type)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string)),
        },
        ActionData::Resign { player_id, .. } => Decoded {
            player_number: i32::from(*player_id),
            kind: "resign".into(),
            target_id: None,
            amount: None,
            detail: None,
        },
        // Everything else: keep the timeline intact with the lowercased variant name + time +
        // player, exactly as the old extractor's fallthrough does.
        other => {
            let (kind, pid) = generic_kind_and_player(other);
            let player_number = require_player_number(&kind, pid)?;
            Decoded {
                player_number,
                kind,
                target_id: None,
                amount: None,
                detail: None,
            }
        }
    })
}

/// Real players only: `player_number -> PlayerInfo`, skipping civ_id == 0 slots.
fn players_map(game: &Savegame) -> HashMap<i32, PlayerInfo> {
    let mut out = HashMap::new();
    for p in &game.zheader.game_settings.players {
        if p.civ_id == 0 {
            continue; // gaia / empty slot
        }
        out.insert(
            p.player_number,
            PlayerInfo {
                profile_id: p.profile_id,
                civ_id: p.civ_id,
                name: String::from(&p.name),
                team: p.resolved_team_id,
                color: p.color_id,
            },
        );
    }
    out
}

/// Decodes raw `.aoe2record` bytes into an `aoe2rec::Savegame`, converting BOTH an ordinary parse
/// error and a caught PANIC into `Error::Parse`. The vendored `aoe2rec` header decoder `.unwrap()`s
/// internally (`vendor/aoe2rec/src/header/mod.rs:14,16`; `lib.rs:51`) — a newer game build the
/// vendored copy doesn't understand yet panics instead of returning `Err`, which would otherwise
/// crash the 24/7 crawler (see `pipeline/vendor/aoe2rec/VENDORED.md`). `AssertUnwindSafe`: `bytes`
/// is moved in and never read again after this call, panic or not, so there is no shared mutable
/// state an unwind could leave inconsistent.
fn parse_savegame(bytes: Bytes) -> Result<Savegame> {
    match panic::catch_unwind(AssertUnwindSafe(|| Savegame::from_bytes(bytes))) {
        Ok(Ok(game)) => Ok(game),
        Ok(Err(e)) => Err(Error::Parse(e.to_string())),
        Err(payload) => Err(Error::Parse(panic_message(payload))),
    }
}

/// Stringifies a `catch_unwind` panic payload — panics almost always carry a `&str` (a `panic!(literal)`)
/// or `String` (a `panic!("{}", ...)`/`.unwrap()` message); anything else falls back to a fixed
/// message rather than guessing at its shape.
fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "replay parser panicked with a non-string payload".to_string()
    }
}

/// Parse one replay's raw bytes into a [`ParsedReplay`]. `match_id` is supplied by the caller —
/// a replay's own bytes never encode it; that id comes from whichever discovery seed downloaded
/// this file (Task 4c). Pure: no IO, and no panics on malformed input — every failure mode
/// returns `Err` (this runs in a 24/7 unattended loop later).
pub fn parse(match_id: MatchId, bytes: Bytes) -> Result<ParsedReplay> {
    let game = parse_savegame(bytes)?;
    let players = players_map(&game);

    // --- walk the operation stream: every Action carries its own world_time ---
    struct Ev {
        profile_id: Option<i32>,
        player_number: i32,
        t_ms: u32,
        kind: String,
        target_id: Option<i64>,
        amount: Option<i64>,
        detail: Option<String>,
    }
    let mut events: Vec<Ev> = Vec::new();
    let mut resigned: HashSet<i32> = HashSet::new();
    let mut last_t: u32 = 0;
    let mut elo = EloTable::default();

    for op in &game.operations {
        match op {
            Operation::Action {
                action_data,
                world_time,
                ..
            } => {
                let world_time = *world_time;
                last_t = last_t.max(world_time);
                let d = classify(action_data)?;
                let profile_id = players.get(&d.player_number).map(|p| p.profile_id);
                if d.kind == "resign" {
                    resigned.insert(d.player_number);
                }
                events.push(Ev {
                    profile_id,
                    player_number: d.player_number,
                    t_ms: world_time,
                    kind: d.kind,
                    target_id: d.target_id,
                    amount: d.amount,
                    detail: d.detail,
                });
            }
            Operation::PostGame { blocks, .. } => {
                collect_leaderboard_elo(blocks, &mut elo);
            }
            _ => {} // Sync / Viewlock / Chat / ... are not events
        }
    }

    // --- per-age boundary time per player (first Research of 101/102/103) ------
    let mut age_time: HashMap<i32, HashMap<&'static str, u32>> = HashMap::new();
    for e in &events {
        if e.kind == "research" {
            if let Some(tid) = e.target_id {
                if let Some(age) = u16::try_from(tid).ok().and_then(config::age_name) {
                    age_time
                        .entry(e.player_number)
                        .or_default()
                        .entry(age)
                        .or_insert(e.t_ms); // first age-up click wins
                }
            }
        }
    }

    // winner: a single resigner => the others won; otherwise unknown (filled later from the
    // matches table). Matches the old extractor's `winner_known`.
    let n_players = players.len();
    let winner_known = !resigned.is_empty() && resigned.len() < n_players.max(1);
    let won = |pnum: i32| -> Option<bool> {
        if !winner_known {
            None
        } else {
            Some(!resigned.contains(&pnum))
        }
    };

    // --- ages: cumulative composition up to each reached age boundary ---------
    let mut ages: Vec<ReplayAge> = Vec::new();
    for (&pnum, pinfo) in &players {
        let times = match age_time.get(&pnum) {
            Some(t) => t,
            None => continue,
        };
        for age_str in config::AGES {
            let t_age = match times.get(age_str) {
                Some(&t) => t,
                None => continue, // didn't reach this age
            };
            let (mut vils, mut mil, mut nb, mut nr) = (0i64, 0i64, 0i64, 0i64);
            for e in &events {
                if e.player_number != pnum || e.t_ms > t_age {
                    continue;
                }
                match e.kind.as_str() {
                    "train" => {
                        let tid = e.target_id.unwrap_or(-1);
                        let amt = e.amount.unwrap_or(1).max(1);
                        match u16::try_from(tid).ok() {
                            Some(u) if config::is_eco_unit(u) => {
                                if u == config::VILLAGER_ID {
                                    vils += amt;
                                }
                            }
                            // non-eco (or unknown id) counts as military, as in the old parser.
                            _ => mil += amt,
                        }
                    }
                    "build" => nb += 1,
                    "research" => nr += 1,
                    _ => {}
                }
            }

            ages.push(ReplayAge {
                profile_id: ProfileId(i64::from(pinfo.profile_id)),
                civ_id: GameCivId(i32::try_from(pinfo.civ_id).map_err(overflow("age.civ_id"))?),
                won: won(pnum),
                age: Age::try_from(age_str)?,
                uptime_ms: i32::try_from(t_age).map_err(overflow("age.uptime_ms"))?,
                villagers: i32::try_from(vils).map_err(overflow("age.villagers"))?,
                military: i32::try_from(mil).map_err(overflow("age.military"))?,
                n_buildings: i32::try_from(nb).map_err(overflow("age.n_buildings"))?,
                n_research: i32::try_from(nr).map_err(overflow("age.n_research"))?,
            });
        }
    }

    // --- meta + players + events rows -----------------------------------------
    let build = i32::try_from(game.zheader.build).map_err(overflow("meta.build"))?;
    let map_id = i32::try_from(game.zheader.game_settings.resolved_map_id)
        .map_err(overflow("meta.map_id"))?;
    let duration_ms = i32::try_from(last_t).map_err(overflow("meta.duration_ms"))?;
    let n_players = i16::try_from(n_players).map_err(overflow("meta.n_players"))?;

    let mut players_out = Vec::with_capacity(players.len());
    for (&pn, pi) in &players {
        players_out.push(ReplayPlayer {
            player_number: i16::try_from(pn).map_err(overflow("player.player_number"))?,
            profile_id: ProfileId(i64::from(pi.profile_id)),
            civ_id: GameCivId(i32::try_from(pi.civ_id).map_err(overflow("player.civ_id"))?),
            name: pi.name.clone(),
            team: i16::from(pi.team),
            color: pi.color,
            won: won(pn),
            // 3 = 1v1 RM, 4 = team RM; a given match is one mode, so exactly one of these is
            // ever populated. None when the replay has no post-game block at all.
            elo: elo.elo(pn, 3).or_else(|| elo.elo(pn, 4)),
        });
    }

    let mut events_out = Vec::with_capacity(events.len());
    for e in events {
        events_out.push(ReplayEvent {
            profile_id: e.profile_id.map(|pid| ProfileId(i64::from(pid))),
            player_number: i16::try_from(e.player_number)
                .map_err(overflow("event.player_number"))?,
            t_ms: i32::try_from(e.t_ms).map_err(overflow("event.t_ms"))?,
            kind: e.kind,
            target_id: e.target_id,
            amount: e.amount,
            detail: e.detail,
        });
    }

    Ok(ParsedReplay {
        match_id,
        build: Some(build),
        map_id: Some(map_id),
        duration_ms,
        n_players,
        players: players_out,
        events: events_out,
        ages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Crafted so the VENDORED `aoe2rec` header decompressor PANICS rather than returning `Err`:
    /// `Savegame`'s `length: u32` field (first 4 bytes, LE) drives `#[br(count = length - 8)]`'s
    /// header-blob size; `length = 12` asks for 4 more bytes, which are not valid raw-deflate, so
    /// `header::decompress`'s `yazi::decompress(...).unwrap()` (`header/mod.rs:14`) panics with
    /// `InvalidBitstream`. Verified empirically against the unguarded vendored call before this
    /// fix landed (see the task report) — this is a real, reproducible panic, not a guess.
    const PANIC_INDUCING_BYTES: [u8; 12] = [12, 0, 0, 0, 0, 0, 0, 0, 0xDE, 0xAD, 0xBE, 0xEF];

    #[test]
    fn parse_savegame_returns_err_not_panic_on_bytes_that_panic_the_vendored_decoder() {
        // `Savegame` (the `Ok` type) isn't `Debug` (vendored, not our type to change), so match
        // directly rather than `.expect_err`/`.unwrap_err`.
        match parse_savegame(Bytes::from_static(&PANIC_INDUCING_BYTES)) {
            Err(Error::Parse(_)) => {}
            Err(other) => panic!("expected Error::Parse, got {other:?}"),
            Ok(_) => panic!("crafted bytes must fail, never panic the caller"),
        }
    }

    #[test]
    fn public_parse_returns_err_not_panic_on_bytes_that_panic_the_vendored_decoder() {
        // Exercises the actual public entry point the crawler calls, not just the internal helper.
        let result = parse(MatchId(1), Bytes::from_static(&PANIC_INDUCING_BYTES));
        assert!(
            result.is_err(),
            "must return Err, never unwind past parse()"
        );
    }

    #[test]
    fn parse_savegame_returns_err_on_ordinary_garbage_too() {
        // Plain-old malformed input (not the panic-inducing shape above) must also stay an
        // ordinary `Err`, matching the pre-existing (non-panicking) failure path.
        let result = parse_savegame(Bytes::from_static(b"not a real aoe2record at all"));
        assert!(matches!(result, Err(Error::Parse(_))));
    }

    #[test]
    fn kind_and_player_from_value_reads_the_bare_string_arm() {
        // A future unit-like top-level ActionData variant would serialize this way (mirrors
        // `game_command_name`'s existing `Value::String` arm for the nested `Game` enum).
        let (kind, pid) = kind_and_player_from_value(serde_json::Value::String("Spy".to_string()));
        assert_eq!(kind, "spy");
        assert_eq!(pid, None, "a bare string variant carries no player_id");
    }

    #[test]
    fn kind_and_player_from_value_reads_the_object_arm() {
        let value = serde_json::json!({ "Move": { "player_id": 3 } });
        let (kind, pid) = kind_and_player_from_value(value);
        assert_eq!(kind, "move");
        assert_eq!(pid, Some(3));
    }

    #[test]
    fn kind_and_player_from_value_is_honest_when_the_body_has_no_player_id() {
        let value = serde_json::json!({ "Foo": {} });
        let (kind, pid) = kind_and_player_from_value(value);
        assert_eq!(kind, "foo");
        assert_eq!(pid, None);
    }

    #[test]
    fn require_player_number_fails_loud_instead_of_defaulting_to_zero() {
        let err = require_player_number("spy", None).expect_err("None must not become 0");
        assert!(matches!(err, Error::Parse(_)));
    }

    #[test]
    fn require_player_number_narrows_a_present_pid() {
        assert_eq!(require_player_number("move", Some(3)).unwrap(), 3);
    }

    #[test]
    fn require_player_number_overflow_still_reports_overflow_not_parse() {
        let err = require_player_number("move", Some(i64::MAX)).unwrap_err();
        assert!(matches!(
            err,
            Error::Overflow {
                field: "event.player_id",
                ..
            }
        ));
    }
}
