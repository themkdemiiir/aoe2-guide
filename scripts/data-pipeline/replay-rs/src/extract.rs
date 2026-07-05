//! Parse a replay with aoe2rec (in-process) and extract structured data.
//!
//! Faithful port of `scripts/data-pipeline/replays/extract.py`. Where the Python
//! version shells out to the `aoe2js` CLI and reads JSON, this calls
//! `aoe2rec::Savegame::from_bytes` directly — no subprocess, no JSON roundtrip.
//!
//! Produces four intact, queryable shapes per replay:
//!   - `meta`    : one row (match_id, build, version, map_id, duration_ms, n_players)
//!   - `players` : one row per real player (profile_id, civ_id, name, team, color, won, elo)
//!   - `events`  : EVERY player action — the full timeline, nothing dropped
//!   - `ages`    : per player per age reached (uptime + cumulative composition),
//!     the same shape as the historical aoestats `match_ages` table.
//!
//! Raw ids are kept as-is; id->name mapping happens later via the icon-map dim,
//! so the parser has no name dependencies and never loses information.

use anyhow::{anyhow, Result};
use serde::Serialize;
use std::collections::HashMap;

use aoe2rec::actions::{ActionData, Game};
use aoe2rec::{Operation, Savegame};

use replay_rs::config;
use replay_rs::postgame::{collect_leaderboard_elo, EloTable};

// --- output row shapes (serialised to NDJSON; column types match store.py) ---

#[derive(Serialize)]
pub struct MetaRow {
    pub match_id: i64,
    pub build: Option<u32>,
    pub version_major: Option<u16>,
    pub map_id: Option<u32>,
    pub duration_ms: u32,
    pub n_players: usize,
}

#[derive(Serialize)]
pub struct PlayerRow {
    pub match_id: i64,
    pub player_number: i32,
    pub profile_id: i32,
    pub civ_id: u32,
    pub name: String,
    pub team: u8,
    pub color: i32,
    pub won: Option<bool>,
    /// Ranked ELO read straight from the replay's post-game leaderboard block
    /// (ladder 3 = 1v1 RM, 4 = team RM — a match is one mode, so whichever is
    /// present). `None` when the replay carries no post-game block (game end
    /// unrecorded) — an honest absence, never a fabricated default.
    pub elo: Option<i32>,
}

#[derive(Serialize)]
pub struct EventRow {
    pub match_id: i64,
    pub profile_id: Option<i32>,
    pub player_number: i32,
    pub t_ms: u32,
    pub kind: String,
    pub target_id: Option<i64>,
    pub amount: Option<i64>,
    pub detail: Option<String>,
}

#[derive(Serialize)]
pub struct AgeRow {
    pub match_id: i64,
    pub profile_id: i32,
    pub civ_id: u32,
    pub won: Option<bool>,
    pub age: String,
    pub uptime_ms: u32,
    pub villagers: i64,
    pub military: i64,
    pub n_buildings: i64,
    pub n_research: i64,
}

/// The four tables produced from one replay.
pub struct Extracted {
    pub meta: MetaRow,
    pub players: Vec<PlayerRow>,
    pub events: Vec<EventRow>,
    pub ages: Vec<AgeRow>,
}

// --- internal helpers --------------------------------------------------------

/// Real-player record keyed by `player_number` (skip gaia/empty: civ_id == 0).
struct PlayerInfo {
    profile_id: i32,
    civ_id: u32,
    name: String,
    team: u8,
    color: i32,
}

/// One decoded action, mirroring extract.py's `_classify` tuple.
struct Decoded {
    player_number: i32,
    kind: String,
    target_id: Option<i64>,
    amount: Option<i64>,
    detail: Option<String>,
}

/// Build action keeps a raw byte blob; the building id is int32 LE at [12:16].
fn decode_build_id(data: &[u8]) -> Option<i64> {
    if data.len() < 16 {
        return None;
    }
    let bytes: [u8; 4] = data[12..16].try_into().ok()?;
    Some(i32::from_le_bytes(bytes) as i64)
}

/// Name of the `Game` sub-command (its serde variant tag), e.g. "Diplomacy".
fn game_command_name(g: &Game) -> Option<String> {
    // The Game enum is small and Serialize-tagged; reading the single top-level
    // key gives the variant name without a hand-written match over every arm.
    match serde_json::to_value(g).ok()? {
        serde_json::Value::Object(map) => map.into_iter().next().map(|(k, _)| k),
        serde_json::Value::String(s) => Some(s), // unit-like variants (e.g. Spy)
        _ => None,
    }
}

/// Lowercased variant name + player_id for *any* ActionData, read generically
/// from its Serialize tag. This is how extract.py classifies the long tail of
/// actions (Move, Interact, Gatherpoint, ...) without a 40-arm match.
fn generic_kind_and_player(ad: &ActionData) -> (String, Option<i64>) {
    match serde_json::to_value(ad) {
        Ok(serde_json::Value::Object(map)) => {
            // Externally-tagged enum: exactly one top-level key = variant name.
            if let Some((variant, body)) = map.into_iter().next() {
                let pid = body
                    .get("player_id")
                    .and_then(|v| v.as_i64());
                return (variant.to_lowercase(), pid);
            }
            (String::new(), None)
        }
        _ => (String::new(), None),
    }
}

/// Map an ActionData -> (player_number, kind, target_id, amount, detail).
/// Faithful port of extract.py `_classify`.
fn classify(ad: &ActionData) -> Decoded {
    match ad {
        ActionData::DeQueue {
            player_id,
            unit_id,
            amount,
            ..
        } => Decoded {
            player_number: *player_id as i32,
            kind: "train".into(),
            target_id: Some(*unit_id as i64),
            // `amount or 1`: a 0 amount in the record still means one queued unit.
            amount: Some(if *amount == 0 { 1 } else { *amount as i64 }),
            detail: None,
        },
        ActionData::Research {
            player_id,
            technology_type,
            ..
        } => Decoded {
            player_number: *player_id as i32,
            kind: "research".into(),
            target_id: Some(*technology_type as i64),
            amount: None,
            detail: None,
        },
        ActionData::Build {
            player_id, data, ..
        } => Decoded {
            player_number: *player_id as i32,
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
            player_number: *player_id as i32,
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
            player_number: *player_id as i32,
            kind: "order".into(),
            target_id: None,
            amount: None,
            // order_type is a small repr enum; its serde tag is the name.
            detail: serde_json::to_value(order_type)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string)),
        },
        ActionData::Resign { player_id, .. } => Decoded {
            player_number: *player_id as i32,
            kind: "resign".into(),
            target_id: None,
            amount: None,
            detail: None,
        },
        // Everything else: keep the timeline intact with the lowercased variant
        // name + time + player, exactly as extract.py's fallthrough does.
        other => {
            let (kind, pid) = generic_kind_and_player(other);
            Decoded {
                player_number: pid.unwrap_or(0) as i32,
                kind,
                target_id: None,
                amount: None,
                detail: None,
            }
        }
    }
}

/// real players only: player_number -> PlayerInfo, skipping civ_id == 0 slots.
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

/// Parse one replay's raw bytes and extract the four tables. Errors on a parse
/// failure (the caller maps that to the `parse_failed` status).
pub fn extract(match_id: i64, replay_bytes: bytes::Bytes) -> Result<Extracted> {
    let game = Savegame::from_bytes(replay_bytes).map_err(|e| anyhow!(e.to_string()))?;
    let players = players_map(&game);

    // --- walk the operation stream: every Action carries its own world_time ---
    // event = (profile_id, player_number, t_ms, kind, target_id, amount, detail)
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
    let mut resigned: std::collections::HashSet<i32> = Default::default();
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
                let d = classify(action_data);
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
    // age_time: player_number -> {age_name -> first uptime_ms}
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

    // winner: a single resigner => the others won; otherwise unknown (filled
    // later from the games table). Matches extract.py's `winner_known`.
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
    let mut ages: Vec<AgeRow> = Vec::new();
    for (&pnum, pinfo) in &players {
        let times = match age_time.get(&pnum) {
            Some(t) => t,
            None => continue,
        };
        for age in config::AGES {
            let t_age = match times.get(age) {
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
                        let unit = u16::try_from(tid).ok();
                        if let Some(u) = unit {
                            if config::is_eco_unit(u) {
                                if u == config::VILLAGER_ID {
                                    vils += amt;
                                }
                            } else {
                                mil += amt;
                            }
                        } else {
                            // non-eco (unknown id) counts as military, as in Python
                            mil += amt;
                        }
                    }
                    "build" => nb += 1,
                    "research" => nr += 1,
                    _ => {}
                }
            }
            ages.push(AgeRow {
                match_id,
                profile_id: pinfo.profile_id,
                civ_id: pinfo.civ_id,
                won: won(pnum),
                age: age.to_string(),
                uptime_ms: t_age,
                villagers: vils,
                military: mil,
                n_buildings: nb,
                n_research: nr,
            });
        }
    }

    // --- meta + players + events rows -----------------------------------------
    let meta = MetaRow {
        match_id,
        build: Some(game.zheader.build),
        version_major: Some(game.zheader.version_major),
        map_id: Some(game.zheader.game_settings.resolved_map_id),
        duration_ms: last_t,
        n_players,
    };

    let player_rows = players
        .iter()
        .map(|(&pn, pi)| PlayerRow {
            match_id,
            player_number: pn,
            profile_id: pi.profile_id,
            civ_id: pi.civ_id,
            name: pi.name.clone(),
            team: pi.team,
            color: pi.color,
            won: won(pn),
            // 3 = 1v1 RM, 4 = team RM; a given match is one mode, so exactly one
            // of these is ever populated. None when the replay has no post-game
            // block at all (game end unrecorded).
            elo: elo.elo(pn, 3).or_else(|| elo.elo(pn, 4)),
        })
        .collect();

    let event_rows = events
        .into_iter()
        .map(|e| EventRow {
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

    Ok(Extracted {
        meta,
        players: player_rows,
        events: event_rows,
        ages,
    })
}
