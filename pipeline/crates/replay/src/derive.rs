//! Pure per-player derivation from a [`ParsedReplay`]'s own events: opening classification +
//! age-up-COMPLETION timings (Phase A, `.superpowers/sdd/task-enrichA-brief.md`), per-unit
//! trained-composition totals (Phase B, `.superpowers/sdd/task-enrichB-brief.md`),
//! commands-per-minute (Phase C, `.superpowers/sdd/task-enrichC-brief.md`), and watched-eco-tech
//! first-research CLICK timings (Phase D, `.superpowers/sdd/task-enrichD-brief.md`) — fills
//! `match_players.opening`/`feudal_t`/`castle_t`/`imperial_t`/`apm`, `match_player_units`, and
//! `match_player_techs`, which the replay ingest path previously left `NULL`/empty.
//!
//! **Ported (not imported)** from `analyzer/crates/analyzer/src/analyze/{metrics.rs,compare.rs}`
//! — that crate is a separate workspace root with its own vendored `aoe2rec` and a different
//! event model (`Ev`/`EvKind`, pre-expanded `Train` amounts), so importing it isn't an option.
//! Each ported function below carries a `// source:` comment pointing at the original. Adapted to
//! this crate's [`ReplayEvent`] (`kind: String`, `target_id`/`amount: Option<i64>`, NOT
//! pre-expanded — irrelevant for the Phase A functions since neither counts amounts, only
//! first-seen timestamps; Phase B's [`player_units`] is the first function here that does).
//!
//! Returned ALONGSIDE `ParsedReplay` (via [`derive`], called on `&ParsedReplay`) rather than
//! attached as a new field on the struct itself — keeps `parse`/`types` untouched (smaller,
//! lower-risk diff) and keeps this module directly unit-testable against hand-built
//! `ParsedReplay` values without touching the parser at all.
//!
//! ## APM basis — REPLAY-SOURCE ONLY, one scalar, per-command not per-batch (Phase C)
//! [`PlayerSummary::apm`] counts a player's raw `events` (one per command the replay recorded —
//! train/research/build/game/order/resign + the generic fallthrough already produce exactly one
//! event per command) over `parsed.duration_ms`. A shift-queued train of 5 is ONE event here
//! (ONE action for APM), never 5 — that batch total is Phase B's separate `trained` metric,
//! summed over `amount`. aoestats has no per-action data at all, so its `match_players` rows keep
//! `apm = NULL` (honest absence); nothing else ever fills this column, so — unlike the age
//! timings' completion-vs-click cross-source pooling rule — there's no reconciliation concern:
//! this crate's own `duration_ms` basis only needs to be internally consistent with itself, which
//! it is by construction. Do NOT reconcile it against the analyzer's `game.get_duration()`; that
//! is a separate system with its own duration basis. A player with zero events gets `Some(0.0)`
//! (they genuinely acted zero times) — never `None`, which is reserved for the aoestats path.
//! See [`apm`].
//!
//! ## CRITICAL correctness rule — completion, not click
//! `match_players.{feudal_t,castle_t,imperial_t}` are **COMPLETION seconds**: the aoestats
//! ingest path fills these same columns from `*_age_uptime` (already completion), and
//! `benchmark_ageup.sql` pools rows from BOTH sources into one median. [`derive`] therefore
//! stores `click_ms / 1000.0 + age_research_s(civ)`, never the raw click — storing raw click
//! would sit ~130-190s short of the aoestats value for the same real event and corrupt that
//! median. `None` when the player never reached that age — never fabricated. See
//! [`completion_s`].
//!
//! ## The honest metric — Phase B's `units` (queued, not surviving)
//! Each `PlayerSummary.units` entry's second element is `trained` = Σ `amount` over that player's
//! `train` events for one `unit_id` — EXACTLY the same `amount.unwrap_or(1).max(1)` rule
//! `parse.rs`'s own age-summary walk already uses (`parse.rs:361-372`). This is units QUEUED,
//! never surviving army: the replay format carries no deaths/losses, so survivorship is honestly
//! unknowable from this data alone. See [`player_units`].
//!
//! ## Tech-timings basis — CLICK, not completion, replay-only (Phase D)
//! `PlayerSummary.techs` (and, downstream, `match_player_techs.t_ms`) is the research-**START**
//! (CLICK) time for each of [`config::WATCHED_TECHS`] — the min `t_ms` of that player's `research`
//! events whose `target_id` is the watched tech, exactly what the replay records and what the
//! analyzer's `eco_tech_times`/`first_research` return. This is the OPPOSITE basis from
//! `feudal_t`/`castle_t`/`imperial_t` above, which store COMPLETION (click + civ-aware research
//! seconds) so they can pool against aoestats' `*_age_uptime` columns. Tech-research timings have
//! no aoestats equivalent at all (that archive carries no per-tech data), so there is no
//! cross-source pooling to match here — click is the honest, directly-recorded value, and
//! converting it to a fabricated "completion" would invent a number the replay never recorded.
//! **Never conflate the two bases**: `match_player_techs.t_ms` (click) and
//! `match_players.feudal_t`/`castle_t`/`imperial_t` (completion) measure different instants even
//! for the same age-up-marking research event. See [`player_techs`].

use std::collections::BTreeMap;

use pipeline_core::{GameUnitId, OpeningKind, ProfileId, TechId};

use crate::config;
use crate::error::overflow;
use crate::types::{ParsedReplay, ReplayEvent};
use crate::Result;

/// One player's derived build-order summary + unit composition + APM + watched-tech timings.
/// Phase A fills `opening` + the three age-up-COMPLETION timings; Phase B adds `units`; Phase C
/// adds `apm`; Phase D (this task) adds `techs`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSummary {
    pub profile_id: ProfileId,
    /// Rule-based opening tag ("Scouts into Archers", "Fast Castle", "Drush + M@A", ...). `None`
    /// when there's nothing honest to say (no Feudal reached, or a passive Feudal with no
    /// Castle and no feudal military) — never a guessed label.
    pub opening: Option<String>,
    /// The closed [`OpeningKind`] this player's rich `opening` tag reconciles onto for
    /// cross-source pooling (final-review finding #1 — see `pipeline_core::opening`'s module doc).
    /// ALWAYS `Some` exactly when `opening` is `Some`, and `None` exactly when `opening` is
    /// `None` — the two are derived from the SAME classification pass in [`classify_opening`], so
    /// they can never desync. `"Drush + X"` -> [`OpeningKind::Drush`] (drush is primary — aoestats
    /// has its own standalone `drush` label with no memory of what followed it); otherwise the
    /// PRIMARY (first-trained) opener line, or [`OpeningKind::FastCastle`] for a passive-Feudal
    /// Castle rush.
    pub opening_kind: Option<OpeningKind>,
    /// Feudal-Age-up COMPLETION time in seconds. `None` if this player never reached Feudal.
    pub feudal_t: Option<f32>,
    /// Castle-Age-up COMPLETION time in seconds. `None` if this player never reached Castle.
    pub castle_t: Option<f32>,
    /// Imperial-Age-up COMPLETION time in seconds. `None` if this player never reached Imperial.
    pub imperial_t: Option<f32>,
    /// Per-unit `trained` totals (Phase B enrichment) — one entry per DISTINCT unit_id this
    /// player trained, sorted by unit_id for determinism, covering EVERY trained unit_id (eco +
    /// military alike — see the module doc's "honest metric" note). Empty (never fabricated as
    /// an all-zero row) when the player trained nothing at all.
    pub units: Vec<(GameUnitId, i32)>,
    /// Commands-per-minute (Phase C enrichment) — always `Some`, never `None`, for a replay
    /// player: see the module doc's "APM basis" note for why replay-derived APM is never absent
    /// (a player with zero commands honestly gets `Some(0.0)`). `Option` only because this same
    /// value flows into `match_players.apm`, which IS `NULL` for the aoestats path — see
    /// [`apm`].
    pub apm: Option<f32>,
    /// Per-watched-tech first-research CLICK time in ms (Phase D enrichment) — one entry per
    /// `config::WATCHED_TECHS` id this player ACTUALLY researched, sorted by tech_id for
    /// determinism. **CLICK, not completion** — see the module doc's "Tech-timings basis" note
    /// for why this differs from `feudal_t`/`castle_t`/`imperial_t` above. A tech never
    /// researched by this player is simply absent — never a fabricated sentinel row.
    pub techs: Vec<(TechId, i32)>,
}

/// Derives every real player's [`PlayerSummary`] from a parsed replay's own `events` — no second
/// `aoe2rec` walk, no IO. Call this BEFORE consuming `parsed.players`/`parsed.events` (e.g. via
/// `.into_iter()`); match results back to a player by [`PlayerSummary::profile_id`].
///
/// # Errors
/// [`crate::Error::Overflow`] if a `train` event's `target_id` (unit_id) or a player's
/// summed `amount` for one unit_id doesn't fit `i32` ([`player_units`]) — unreachable with real
/// replay data (unit ids and army sizes never approach `i32::MAX`), but never silently narrowed.
pub fn derive(parsed: &ParsedReplay) -> Result<Vec<PlayerSummary>> {
    // `load_game_civs` parses a compile-time `include_str!`-embedded JSON (see
    // `pipeline_core::civs`'s module doc) — no runtime file IO, so calling it here doesn't
    // violate this crate's "pure, zero IO" invariant. Its `Result` is still handled without
    // `unwrap`/`expect`/`panic` (rust-playbook.md rule 8): a load failure (or, per player, a
    // civ_id absent from the committed map — e.g. a future DLC civ this repo hasn't indexed
    // yet) folds into the SAME `""` fallback `age_research_s` already treats as baseline. That
    // is not a new fabrication introduced by this port — it's the identical defaulting the
    // ported analyzer's own `compare::build_metrics` already performs
    // (`civs.get(&info.civ_id).map(String::as_str).unwrap_or("")`).
    let civs = pipeline_core::civs::load_game_civs().ok();

    parsed
        .players
        .iter()
        .map(|p| {
            let (feudal_click, castle_click, imperial_click) =
                age_clicks(&parsed.events, p.player_number);
            let civ_slug = civs
                .as_ref()
                .and_then(|m| m.slug(p.civ_id).ok())
                .unwrap_or("");
            let (feudal_res_s, castle_res_s, imperial_res_s) = age_research_s(civ_slug);
            let opening = classify_opening(&parsed.events, p.player_number, feudal_click, castle_click);
            Ok(PlayerSummary {
                profile_id: p.profile_id,
                opening: opening.as_ref().map(|(tag, _)| tag.clone()),
                opening_kind: opening.map(|(_, kind)| kind),
                feudal_t: completion_s(feudal_click, feudal_res_s),
                castle_t: completion_s(castle_click, castle_res_s),
                imperial_t: completion_s(imperial_click, imperial_res_s),
                units: player_units(&parsed.events, p.player_number)?,
                apm: Some(apm(&parsed.events, p.player_number, parsed.duration_ms)),
                techs: player_techs(&parsed.events, p.player_number),
            })
        })
        .collect()
}

/// Converts a raw age-up-research CLICK time into a COMPLETION time in seconds — see the module
/// doc's "CRITICAL correctness rule". `None` (never fabricated) when the click itself is `None`
/// (the player never reached this age).
fn completion_s(click_ms: Option<i32>, research_s: f64) -> Option<f32> {
    let click_ms = click_ms?;
    Some((f64::from(click_ms) / 1000.0 + research_s) as f32)
}

/// The min `t_ms` of `player_number`'s `research` events whose `target_id` equals `tech_id` — the
/// shared min-over-research primitive [`first_age_click`] (age-up techs 101/102/103) and
/// [`player_techs`] (Phase D's watched eco techs) both need, factored out so neither duplicates
/// the filter. Compares `target_id` (`i64`) by WIDENING `tech_id` up to `i64` rather than
/// narrowing the event's value down to `u16`: `tech_id` is always representable in `i64`
/// losslessly, so this is provably equivalent to a decode-then-compare for every input, without
/// ever needing a fallible conversion (unlike [`player_units`], which must decode an unbounded
/// `target_id` INTO its result and so genuinely needs the checked/fail-loud guard).
/// source: analyzer/crates/analyzer/src/analyze/metrics.rs::first_research
fn first_research_ms(evs: &[ReplayEvent], player_number: i16, tech_id: u16) -> Option<i32> {
    let target = i64::from(tech_id);
    evs.iter()
        .filter(|e| {
            e.player_number == player_number && e.kind == "research" && e.target_id == Some(target)
        })
        .map(|e| e.t_ms)
        .min()
}

/// The first `research` event whose `target_id` is the age-up tech for `age`
/// (`config::age_tech_id`: feudal/castle/imperial -> 101/102/103), for one player. `.min()` (via
/// [`first_research_ms`]) over every matching event (order-independent) rather than "first in the
/// list" — the replay's events are already in stream order in practice, but this ported function
/// makes no assumption either way, matching the analyzer original exactly. Resolves the tech id
/// via `config::age_tech_id` rather than re-hardcoding 101/102/103 here, so there's one canonical
/// copy of the three tech ids (in `config.rs`, alongside its inverse `age_name`).
/// source: analyzer/crates/analyzer/src/analyze/metrics.rs::first_research (called from
/// age_clicks with tech ids 101/102/103 directly).
fn first_age_click(evs: &[ReplayEvent], player_number: i16, age: &str) -> Option<i32> {
    let tech_id = config::age_tech_id(age)?;
    first_research_ms(evs, player_number, tech_id)
}

/// (feudal, castle, imperial) age-up CLICK ms for one player.
/// source: analyzer/crates/analyzer/src/analyze/metrics.rs::age_clicks
fn age_clicks(evs: &[ReplayEvent], player_number: i16) -> (Option<i32>, Option<i32>, Option<i32>) {
    (
        first_age_click(evs, player_number, "feudal"),
        first_age_click(evs, player_number, "castle"),
        first_age_click(evs, player_number, "imperial"),
    )
}

/// `true` when `e` is a `train` command for one of `unit_ids`.
fn trains(e: &ReplayEvent, unit_ids: &[u16]) -> bool {
    if e.kind != "train" {
        return false;
    }
    e.target_id
        .and_then(|tid| u16::try_from(tid).ok())
        .is_some_and(|u| unit_ids.contains(&u))
}

/// Rule-based opening tag (light port of dj0wns/AoE_Rec_Opening_Analysis): dark-age militia ≥3 =
/// "Drush" prefix; then the first two DISTINCT unit lines opened in the Feudal window, in train
/// order ("Scouts into Archers"); no feudal military but Castle reached = "Fast Castle". `None`
/// when there's nothing to say (no Feudal, or a passive Feudal with no Castle) — never guess.
/// source: analyzer/crates/analyzer/src/analyze/metrics.rs::classify_opening
///
/// Returns the rich display tag PAIRED with its [`OpeningKind`] (never one without the other —
/// see [`PlayerSummary::opening_kind`]'s doc for why they can't desync) rather than re-deriving
/// the kind from the formatted string afterward: `config::OPENER_LINES` already carries each
/// line's `OpeningKind` alongside its display tag, so this reads it straight from the SAME match
/// that builds the rich string, never a second, lossier parse of it.
///
/// **Kind resolution** (see `pipeline_core::opening`'s module doc for the cross-source rationale):
/// a `"Drush + "` prefix always wins as [`OpeningKind::Drush`], regardless of the body (aoestats'
/// own standalone `drush` label carries no memory of what followed it); otherwise a passive-Feudal
/// Castle rush is [`OpeningKind::FastCastle`]; otherwise the PRIMARY is the first-trained opener
/// line's own kind (`"Scouts into Archers"` -> [`OpeningKind::Scouts`], the earlier-trained line —
/// never the later `Archers`).
fn classify_opening(
    evs: &[ReplayEvent],
    player_number: i16,
    feudal_ms: Option<i32>,
    castle_ms: Option<i32>,
) -> Option<(String, OpeningKind)> {
    let feudal = feudal_ms?;
    let castle_or = castle_ms.unwrap_or(feudal + 12 * 60_000);
    let dark_militia = evs
        .iter()
        .filter(|e| e.player_number == player_number && e.t_ms < feudal)
        .filter(|e| trains(e, &config::MILITIA_LINE))
        .count();

    let mut opened: Vec<(i32, &str, OpeningKind)> = config::OPENER_LINES
        .iter()
        .filter_map(|&(ids, tag, kind)| {
            evs.iter()
                .filter(|e| {
                    e.player_number == player_number && e.t_ms >= feudal && e.t_ms < castle_or
                })
                .filter(|e| trains(e, ids))
                .map(|e| e.t_ms)
                .min()
                .map(|t| (t, tag, kind))
        })
        .collect();
    opened.sort_by_key(|&(t, _, _)| t);

    let feudal_tags: Vec<(&str, OpeningKind)> =
        opened.iter().take(2).map(|&(_, tag, kind)| (tag, kind)).collect();
    let (body, body_kind) = match feudal_tags.as_slice() {
        [] if castle_ms.is_some() => ("Fast Castle".to_string(), OpeningKind::FastCastle),
        [] => return None,
        [(tag, kind)] => ((*tag).to_string(), *kind),
        [(a, ak), (b, _)] => (format!("{a} into {b}"), *ak),
        _ => unreachable!(),
    };
    Some(if dark_militia >= 3 {
        (format!("Drush + {body}"), OpeningKind::Drush)
    } else {
        (body, body_kind)
    })
}

/// Sums one player's `train`-event `amount`s per DISTINCT `unit_id` — Phase B enrichment (see
/// the module doc's "honest metric" note): `trained` = Σ `amount.unwrap_or(1).max(1)`, EXACTLY
/// the same rule `parse.rs`'s own age-summary walk already uses (`parse.rs:361-372`). Folds into
/// a `BTreeMap<i32, i64>` (unit_id -> running total) first — its own iteration order is already
/// sorted by key, so the final `Vec` comes out unit_id-ordered for free, no separate sort step —
/// then narrows both the key and the total to `i32` (fail-loud, never silently wrapped: see the
/// function's `# Errors`). An empty `evs`/no matching `train` events for `player_number` yields
/// an empty `Vec`, never a fabricated all-zero row.
///
/// # Errors
/// [`crate::Error::Overflow`] if a `train` event's `target_id` doesn't fit `i32` (folding into
/// the map), or if one unit_id's summed `amount` total doesn't fit `i32` (converting to the
/// final `Vec`) — unreachable with real replay data (AOE2 unit ids and army sizes never approach
/// `i32::MAX`), kept fail-loud rather than silently narrowed.
fn player_units(evs: &[ReplayEvent], player_number: i16) -> Result<Vec<(GameUnitId, i32)>> {
    let mut totals: BTreeMap<i32, i64> = BTreeMap::new();
    for e in evs {
        if e.player_number != player_number || e.kind != "train" {
            continue;
        }
        let Some(target_id) = e.target_id else {
            continue;
        };
        let unit_id = i32::try_from(target_id).map_err(overflow("train.target_id"))?;
        // `amount or 1`: a 0/absent amount in the record still means one queued unit — the SAME
        // rule `parse.rs:363` uses for the age-summary walk.
        let amount = e.amount.unwrap_or(1).max(1);
        *totals.entry(unit_id).or_insert(0) += amount;
    }
    totals
        .into_iter()
        .map(|(unit_id, trained)| {
            let trained = i32::try_from(trained).map_err(overflow("train.trained"))?;
            Ok((GameUnitId(unit_id), trained))
        })
        .collect()
}

/// Each of `config::WATCHED_TECHS`' first-research CLICK time for one player (Phase D
/// enrichment) — see the module doc's "Tech-timings basis" note for why this is click, not
/// completion. Folds into a `BTreeMap<i32, i32>` (tech_id -> min t_ms) exactly like
/// [`player_units`]'s `BTreeMap<i32, i64>` fold: its iteration order is already sorted by key, so
/// the final `Vec` comes out tech_id-ordered for free — this MATTERS here because
/// `config::WATCHED_TECHS` is declared in provenance order (matching the analyzer source), not
/// numeric order. A tech this player never researched contributes no entry (no fabricated
/// sentinel row); a LATER duplicate research of an already-seen tech never overrides the
/// earlier (min) `t_ms`, since [`first_research_ms`] itself already takes the min across every
/// matching event for that one tech. Infallible (`Vec`, not `Result`): every id here is a
/// compile-time `u16` constant, so widening it to `i32` (`TechId`) can never overflow — unlike
/// `player_units`, which must decode an unbounded LIVE `target_id` and so needs the checked/
/// fail-loud guard (see [`first_research_ms`]'s doc for the equivalence argument).
/// source: analyzer/crates/analyzer/src/analyze/metrics.rs::eco_tech_times
fn player_techs(evs: &[ReplayEvent], player_number: i16) -> Vec<(TechId, i32)> {
    let mut first_click: BTreeMap<i32, i32> = BTreeMap::new();
    for &(tech_id, _name) in config::WATCHED_TECHS {
        if let Some(t_ms) = first_research_ms(evs, player_number, tech_id) {
            first_click.insert(i32::from(tech_id), t_ms);
        }
    }
    first_click
        .into_iter()
        .map(|(tech_id, t_ms)| (TechId(tech_id), t_ms))
        .collect()
}

/// Commands-per-minute (Phase C enrichment) — see the module doc's "APM basis" note. The
/// numerator is the COUNT of `player_number`'s events, one per RAW command: `parsed.events` is
/// already one-event-per-raw-command (train/research/build/game/order/resign + the generic
/// fallthrough), so a shift-queued train of 5 is still ONE event here, never 5 — that batch total
/// is [`player_units`]'s separate `trained` metric. `duration_ms` floors at one second
/// (`.max(1.0 / 60.0)` minutes) before dividing, the analyzer's own divide-by-zero guard.
/// source: analyzer/crates/analyzer/src/analyze/compare.rs:88-89
fn apm(evs: &[ReplayEvent], player_number: i16, duration_ms: i32) -> f32 {
    let count = evs.iter().filter(|e| e.player_number == player_number).count();
    let minutes = (f64::from(duration_ms) / 60_000.0).max(1.0 / 60.0);
    (count as f64 / minutes) as f32
}

/// Civ-aware click -> COMPLETION age-up research seconds (feudal, castle, imperial). Baseline
/// 130/160/190s (`config::FEUDAL_RES_S`/`CASTLE_RES_S`/`IMP_RES_S`); only civs with a sourced
/// age-up speed bonus deviate (today: Malay, `config::MALAY_AGE_FACTOR`). `civ_slug == ""`
/// (unresolved civ — see [`derive`]'s doc) falls through to the baseline `_` arm, same as every
/// other non-Malay civ.
/// source: analyzer/crates/analyzer/src/analyze/compare.rs::age_research_s
fn age_research_s(civ_slug: &str) -> (f64, f64, f64) {
    let m = match civ_slug {
        "malay" => config::MALAY_AGE_FACTOR,
        _ => 1.0,
    };
    (
        config::FEUDAL_RES_S * m,
        config::CASTLE_RES_S * m,
        config::IMP_RES_S * m,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ReplayPlayer;
    use pipeline_core::{GameCivId, TechId};

    /// GAME civ_id 29 -> "malay" (`src/data/civ-id-map.json`; verified in
    /// `pipeline_core::civs::civs::tests`). Used to prove the completion-time conversion is
    /// civ-AWARE, not just always the baseline factor.
    const MALAY: i32 = 29;
    /// GAME civ_id 2 -> "franks" — a plain baseline-factor civ.
    const FRANKS: i32 = 2;

    fn player(profile_id: i64, player_number: i16, civ_id: i32) -> ReplayPlayer {
        ReplayPlayer {
            player_number,
            profile_id: ProfileId(profile_id),
            civ_id: GameCivId(civ_id),
            name: "P".to_owned(),
            team: 1,
            color: 1,
            won: None,
            elo: None,
        }
    }

    fn ev(player_number: i16, t_ms: i32, kind: &str, target_id: Option<i64>) -> ReplayEvent {
        ReplayEvent {
            profile_id: None,
            player_number,
            t_ms,
            kind: kind.to_owned(),
            target_id,
            amount: None,
            detail: None,
        }
    }

    /// Same as [`ev`] but with an explicit `amount` — for Phase B's [`player_units`] tests, which
    /// need to control batch sizes (`ev` always leaves `amount: None`).
    fn ev_amt(
        player_number: i16,
        t_ms: i32,
        kind: &str,
        target_id: Option<i64>,
        amount: Option<i64>,
    ) -> ReplayEvent {
        ReplayEvent {
            profile_id: None,
            player_number,
            t_ms,
            kind: kind.to_owned(),
            target_id,
            amount,
            detail: None,
        }
    }

    fn parsed(players: Vec<ReplayPlayer>, events: Vec<ReplayEvent>) -> ParsedReplay {
        ParsedReplay {
            match_id: pipeline_core::MatchId(1),
            build: Some(101_101),
            map_id: Some(9),
            duration_ms: 1_800_000,
            n_players: players.len() as i16,
            players,
            events,
            ages: vec![],
        }
    }

    #[test]
    fn fast_castle_timeline_classifies_opening_and_stores_completion_not_click() {
        // Feudal click 10:00 (600_000ms), Castle click 25:00 (1_500_000ms), NO feudal-window
        // military at all -> "Fast Castle". Franks: baseline research factor (1.0).
        let evs = vec![
            ev(1, 600_000, "research", Some(101)),
            ev(1, 1_500_000, "research", Some(102)),
        ];
        let p = parsed(vec![player(5001, 1, FRANKS)], evs);

        let summaries = derive(&p).expect("well-formed test replay must derive cleanly");
        assert_eq!(summaries.len(), 1);
        let s = &summaries[0];
        assert_eq!(s.profile_id, ProfileId(5001));
        assert_eq!(s.opening.as_deref(), Some("Fast Castle"));
        assert_eq!(
            s.opening_kind,
            Some(OpeningKind::FastCastle),
            "opening_kind must match the rich tag it was paired with"
        );

        // COMPLETION, not click: click_s + baseline research_s.
        let expected_feudal = 600.0 + 130.0;
        let expected_castle = 1_500.0 + 160.0;
        assert!(
            (s.feudal_t.unwrap() - expected_feudal).abs() < 0.01,
            "feudal_t={:?} must equal click+research, not raw click (600.0)",
            s.feudal_t
        );
        assert!(
            (s.castle_t.unwrap() - expected_castle).abs() < 0.01,
            "castle_t={:?} must equal click+research, not raw click (1500.0)",
            s.castle_t
        );
    }

    #[test]
    fn completion_time_is_civ_aware_malay_faster_than_baseline() {
        // Same feudal/castle clicks as the baseline test above, but Malay -> both COMPLETION
        // times must be LOWER than the baseline-civ result (research itself is faster), and
        // exactly click_s + baseline_s/1.66.
        let evs = vec![
            ev(1, 600_000, "research", Some(101)),
            ev(1, 1_500_000, "research", Some(102)),
        ];
        let p = parsed(vec![player(5001, 1, MALAY)], evs);

        let s = &derive(&p).expect("well-formed test replay must derive cleanly")[0];
        let expected_feudal = 600.0 + 130.0 / 1.66;
        let expected_castle = 1_500.0 + 160.0 / 1.66;
        assert!((s.feudal_t.unwrap() - expected_feudal as f32).abs() < 0.01);
        assert!((s.castle_t.unwrap() - expected_castle as f32).abs() < 0.01);
        assert!(
            s.feudal_t.unwrap() < 600.0 + 130.0,
            "malay's completion must be faster than a baseline civ's"
        );
    }

    #[test]
    fn never_reached_imperial_stays_none_never_fabricated() {
        // Reaches Feudal + Castle, never researches Imperial (technology_type 103 never fires).
        let evs = vec![
            ev(1, 600_000, "research", Some(101)),
            ev(1, 1_500_000, "research", Some(102)),
        ];
        let p = parsed(vec![player(5001, 1, FRANKS)], evs);

        let s = &derive(&p).expect("well-formed test replay must derive cleanly")[0];
        assert!(s.feudal_t.is_some());
        assert!(s.castle_t.is_some());
        assert_eq!(
            s.imperial_t, None,
            "a player who never reached Imperial must get None, never a guessed value"
        );
    }

    #[test]
    fn passive_feudal_with_no_castle_and_no_military_is_an_unclassifiable_opening() {
        // Reaches Feudal, never Castle, and never trains ANY opener-line unit in the Feudal
        // window -> nothing honest to say about the opening.
        let evs = vec![ev(1, 600_000, "research", Some(101))];
        let p = parsed(vec![player(5001, 1, FRANKS)], evs);

        let s = &derive(&p).expect("well-formed test replay must derive cleanly")[0];
        assert_eq!(
            s.opening, None,
            "an unclassifiable opening must be None, never a guessed label"
        );
        assert_eq!(
            s.opening_kind, None,
            "opening_kind must be None exactly when opening is None — never desynced"
        );
        assert!(s.feudal_t.is_some(), "feudal itself WAS reached");
        assert_eq!(s.castle_t, None);
    }

    #[test]
    fn opening_is_none_when_feudal_was_never_reached() {
        let p = parsed(vec![player(5001, 1, FRANKS)], vec![]);
        let s = &derive(&p).expect("well-formed test replay must derive cleanly")[0];
        assert_eq!(s.opening, None);
        assert_eq!(s.opening_kind, None);
        assert_eq!(s.feudal_t, None);
        assert_eq!(s.castle_t, None);
        assert_eq!(s.imperial_t, None);
    }

    #[test]
    fn drush_prefix_requires_at_least_three_dark_age_militia_line_trains() {
        // 3 Militia-line trains before Feudal, then Scouts opened in the Feudal window.
        let evs = vec![
            ev(1, 100_000, "train", Some(74)), // Militia
            ev(1, 150_000, "train", Some(74)),
            ev(1, 200_000, "train", Some(75)), // Man-at-Arms (same line)
            ev(1, 600_000, "research", Some(101)),
            ev(1, 650_000, "train", Some(448)), // Scouts
        ];
        let p = parsed(vec![player(5001, 1, FRANKS)], evs);
        let s = &derive(&p).expect("well-formed test replay must derive cleanly")[0];
        assert_eq!(s.opening.as_deref(), Some("Drush + Scouts"));
        assert_eq!(
            s.opening_kind,
            Some(OpeningKind::Drush),
            "'Drush + X' -> primary kind is Drush, not Scouts — aoestats has its own standalone \
             'drush' label with no memory of what followed it (see pipeline_core::opening's \
             module doc)"
        );
    }

    #[test]
    fn scouts_into_archers_orders_by_train_time_not_declaration_order() {
        let evs = vec![
            ev(1, 600_000, "research", Some(101)),
            ev(1, 700_000, "train", Some(4)), // Archers trained first...
            ev(1, 650_000, "train", Some(448)), // ...but Scouts trained earlier.
        ];
        let p = parsed(vec![player(5001, 1, FRANKS)], evs);
        let s = &derive(&p).expect("well-formed test replay must derive cleanly")[0];
        assert_eq!(s.opening.as_deref(), Some("Scouts into Archers"));
        assert_eq!(
            s.opening_kind,
            Some(OpeningKind::Scouts),
            "the PRIMARY kind is the earlier-trained line (Scouts), never the later Archers"
        );
    }

    #[test]
    fn derive_returns_one_summary_per_player_matched_by_profile_id() {
        let evs = vec![
            ev(1, 600_000, "research", Some(101)),
            ev(2, 900_000, "research", Some(101)),
        ];
        let p = parsed(vec![player(5001, 1, FRANKS), player(5002, 2, MALAY)], evs);
        let summaries = derive(&p).expect("well-formed test replay must derive cleanly");
        assert_eq!(summaries.len(), 2);
        assert!(summaries.iter().any(|s| s.profile_id == ProfileId(5001)));
        assert!(summaries.iter().any(|s| s.profile_id == ProfileId(5002)));
    }

    #[test]
    fn player_units_sums_batched_trains_per_distinct_unit_sorted_by_unit_id() {
        // Two unit types, each trained across multiple batches: unit 448 (Scouts) via two
        // separate single trains, unit 83 (an eco/other unit) via one batch of 5. The expected
        // per-unit totals are the SUMS, not one row per train command, and come out sorted by
        // unit_id ascending regardless of train order or declaration order.
        let evs = vec![
            ev_amt(1, 100_000, "train", Some(448), Some(1)),
            ev_amt(1, 100_500, "train", Some(83), Some(5)),
            ev_amt(1, 101_000, "train", Some(448), Some(1)),
        ];
        let p = parsed(vec![player(5001, 1, FRANKS)], evs);

        let s = &derive(&p).expect("well-formed test replay must derive cleanly")[0];
        assert_eq!(
            s.units,
            vec![(GameUnitId(83), 5), (GameUnitId(448), 2)],
            "83 < 448 -> sorted by unit_id; 448's two single-batch trains sum to 2"
        );
    }

    #[test]
    fn player_units_treats_none_or_zero_amount_as_one_queued_unit() {
        // `amount: None` and `amount: Some(0)` must both count as exactly 1 — the SAME
        // `amount.unwrap_or(1).max(1)` rule `parse.rs`'s age-summary walk uses.
        let evs = vec![
            ev_amt(1, 100_000, "train", Some(74), None),
            ev_amt(1, 100_500, "train", Some(74), Some(0)),
        ];
        let p = parsed(vec![player(5001, 1, FRANKS)], evs);

        let s = &derive(&p).expect("well-formed test replay must derive cleanly")[0];
        assert_eq!(
            s.units,
            vec![(GameUnitId(74), 2)],
            "None and Some(0) must each count as 1 queued unit, summing to 2"
        );
    }

    #[test]
    fn player_units_is_empty_when_nothing_was_trained_never_fabricated() {
        // Only non-`train` events (a research click) -> nothing to sum, empty Vec, not a
        // fabricated all-zero row.
        let evs = vec![ev(1, 600_000, "research", Some(101))];
        let p = parsed(vec![player(5001, 1, FRANKS)], evs);

        let s = &derive(&p).expect("well-formed test replay must derive cleanly")[0];
        assert_eq!(
            s.units,
            Vec::new(),
            "a player who trained nothing must get an empty Vec, never a fabricated row"
        );
    }

    #[test]
    fn apm_counts_events_not_batch_amounts() {
        // Phase C (task-enrichC): 3 raw commands (1 research + 2 train, one of which is a
        // shift-queued batch of amount=5) over a 1-minute (60_000ms) duration -> APM must be
        // exactly 3.0, NOT 7.0 (which would double-count the batch's `amount` as 5 actions).
        let evs = vec![
            ev(1, 0, "research", Some(101)),
            ev_amt(1, 1_000, "train", Some(83), Some(5)),
            ev_amt(1, 2_000, "train", Some(448), Some(1)),
        ];
        let mut p = parsed(vec![player(5001, 1, FRANKS)], evs);
        p.duration_ms = 60_000;

        let s = &derive(&p).expect("well-formed test replay must derive cleanly")[0];
        assert_eq!(
            s.apm,
            Some(3.0),
            "3 raw commands over 1 minute = 3.0 APM, not 7.0 (amount=5 must count as 1 command)"
        );
    }

    #[test]
    fn apm_max_guard_prevents_divide_by_zero_on_near_zero_duration() {
        // duration_ms = 0 would divide-by-zero without the `.max(1.0 / 60.0)` guard (floors the
        // denominator at 1 second = 1/60 minute). 6 events / (1/60 min) = 360.0 APM, finite.
        let evs: Vec<ReplayEvent> = (0..6).map(|i| ev(1, i, "research", Some(101))).collect();
        let mut p = parsed(vec![player(5001, 1, FRANKS)], evs);
        p.duration_ms = 0;

        let s = &derive(&p).expect("well-formed test replay must derive cleanly")[0];
        let apm = s.apm.expect("apm must always be Some for a replay-derived player");
        assert!(apm.is_finite(), "the .max(1/60) guard must prevent a divide-by-zero/NaN/inf");
        assert!((apm - 360.0).abs() < 0.01, "apm={apm:?} must equal 6 / (1/60) = 360.0");
    }

    #[test]
    fn apm_is_zero_not_none_when_player_has_zero_events() {
        // A player with zero commands genuinely acted zero times -> Some(0.0), never None
        // (None is reserved for the aoestats path, which has no APM data at all).
        let p = parsed(vec![player(5001, 1, FRANKS)], vec![]);
        let s = &derive(&p).expect("well-formed test replay must derive cleanly")[0];
        assert_eq!(
            s.apm,
            Some(0.0),
            "zero events must be an honest Some(0.0), never a fabricated-absence None"
        );
    }

    #[test]
    fn apm_only_counts_the_matching_players_own_events() {
        // Player 2's events must not leak into player 1's APM.
        let evs = vec![
            ev(1, 0, "research", Some(101)),
            ev(2, 0, "research", Some(101)),
            ev(2, 1_000, "train", Some(448)),
        ];
        let mut p = parsed(vec![player(5001, 1, FRANKS), player(5002, 2, MALAY)], evs);
        p.duration_ms = 60_000;

        let summaries = derive(&p).expect("well-formed test replay must derive cleanly");
        let s1 = summaries
            .iter()
            .find(|s| s.profile_id == ProfileId(5001))
            .unwrap();
        let s2 = summaries
            .iter()
            .find(|s| s.profile_id == ProfileId(5002))
            .unwrap();
        assert_eq!(s1.apm, Some(1.0), "player 1 only has its own 1 event");
        assert_eq!(s2.apm, Some(2.0), "player 2 has its own 2 events");
    }

    #[test]
    fn player_units_only_counts_the_matching_players_own_trains() {
        // Player 2's train must not leak into player 1's totals.
        let evs = vec![
            ev_amt(1, 100_000, "train", Some(74), Some(1)),
            ev_amt(2, 100_000, "train", Some(448), Some(3)),
        ];
        let p = parsed(vec![player(5001, 1, FRANKS), player(5002, 2, MALAY)], evs);

        let summaries = derive(&p).expect("well-formed test replay must derive cleanly");
        let s1 = summaries
            .iter()
            .find(|s| s.profile_id == ProfileId(5001))
            .unwrap();
        let s2 = summaries
            .iter()
            .find(|s| s.profile_id == ProfileId(5002))
            .unwrap();
        assert_eq!(s1.units, vec![(GameUnitId(74), 1)]);
        assert_eq!(s2.units, vec![(GameUnitId(448), 3)]);
    }

    #[test]
    fn player_techs_captures_only_watched_techs_at_min_t_ms_sorted_by_id_ignoring_later_duplicates()
    {
        // Hand Cart (249) is declared BEFORE Horse Collar (14) in `config::WATCHED_TECHS`, but
        // the result must come out sorted by tech_id ascending (14 before 249) — proving the
        // sort is by id, not WATCHED_TECHS declaration order. Hand Cart is also researched
        // TWICE: the later duplicate (200_000) must NOT override the earlier click (100_000).
        // Tech 999 is not watched and must be ignored entirely.
        let evs = vec![
            ev(1, 100_000, "research", Some(249)), // Hand Cart, first click
            ev(1, 200_000, "research", Some(249)), // Hand Cart again — must NOT override
            ev(1, 150_000, "research", Some(14)),  // Horse Collar
            ev(1, 50_000, "research", Some(999)),  // not watched -> ignored
        ];
        let p = parsed(vec![player(5001, 1, FRANKS)], evs);

        let s = &derive(&p).expect("well-formed test replay must derive cleanly")[0];
        assert_eq!(
            s.techs,
            vec![(TechId(14), 150_000), (TechId(249), 100_000)],
            "sorted by tech_id ascending (14 < 249, despite 249 being declared first in \
             WATCHED_TECHS); Hand Cart's min t_ms (100_000) wins over its later duplicate \
             (200_000); the non-watched tech 999 is absent entirely"
        );
    }

    #[test]
    fn player_techs_is_empty_when_no_watched_tech_was_researched_never_fabricated() {
        // Only a non-watched tech's research event -> nothing to capture, empty Vec, not a
        // fabricated row.
        let evs = vec![ev(1, 50_000, "research", Some(999))];
        let p = parsed(vec![player(5001, 1, FRANKS)], evs);

        let s = &derive(&p).expect("well-formed test replay must derive cleanly")[0];
        assert_eq!(
            s.techs,
            Vec::new(),
            "a player who researched no watched tech must get an empty Vec, never a fabricated row"
        );
    }
}
