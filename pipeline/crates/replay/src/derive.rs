//! Pure per-player derivation from a [`ParsedReplay`]'s own events: opening classification +
//! age-up-COMPLETION timings (Phase A, `.superpowers/sdd/task-enrichA-brief.md`) and per-unit
//! trained-composition totals (Phase B, `.superpowers/sdd/task-enrichB-brief.md`) — fills
//! `match_players.opening`/`feudal_t`/`castle_t`/`imperial_t` and `match_player_units`, which the
//! replay ingest path previously left `NULL`/empty.
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

use std::collections::BTreeMap;

use pipeline_core::{GameUnitId, ProfileId};

use crate::config;
use crate::error::overflow;
use crate::types::{ParsedReplay, ReplayEvent};
use crate::Result;

/// One player's derived build-order summary + unit composition. Phase A fills `opening` + the
/// three age-up-COMPLETION timings; Phase B (this task) adds `units`. Phase C/D (APM/tech-timings)
/// grow this struct later, per the task brief's scope note.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSummary {
    pub profile_id: ProfileId,
    /// Rule-based opening tag ("Scouts into Archers", "Fast Castle", "Drush + M@A", ...). `None`
    /// when there's nothing honest to say (no Feudal reached, or a passive Feudal with no
    /// Castle and no feudal military) — never a guessed label.
    pub opening: Option<String>,
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
            Ok(PlayerSummary {
                profile_id: p.profile_id,
                opening: classify_opening(
                    &parsed.events,
                    p.player_number,
                    feudal_click,
                    castle_click,
                ),
                feudal_t: completion_s(feudal_click, feudal_res_s),
                castle_t: completion_s(castle_click, castle_res_s),
                imperial_t: completion_s(imperial_click, imperial_res_s),
                units: player_units(&parsed.events, p.player_number)?,
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

/// The first `research` event whose `target_id` decodes to the age-up tech for `age`
/// (`config::age_name`: 101/102/103 -> feudal/castle/imperial), for one player. `.min()` over
/// every matching event (order-independent) rather than "first in the list" — the replay's
/// events are already in stream order in practice, but this ported function makes no assumption
/// either way, matching the analyzer original exactly.
/// source: analyzer/crates/analyzer/src/analyze/metrics.rs::first_research (called from
/// age_clicks with tech ids 101/102/103 directly; this port reads them via `config::age_name`
/// instead of re-hardcoding them, so there's one canonical copy of the three tech ids).
fn first_age_click(evs: &[ReplayEvent], player_number: i16, age: &str) -> Option<i32> {
    evs.iter()
        .filter(|e| e.player_number == player_number && e.kind == "research")
        .filter_map(|e| {
            let tid = e.target_id?;
            let tech = u16::try_from(tid).ok()?;
            (config::age_name(tech) == Some(age)).then_some(e.t_ms)
        })
        .min()
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
fn classify_opening(
    evs: &[ReplayEvent],
    player_number: i16,
    feudal_ms: Option<i32>,
    castle_ms: Option<i32>,
) -> Option<String> {
    let feudal = feudal_ms?;
    let castle_or = castle_ms.unwrap_or(feudal + 12 * 60_000);
    let dark_militia = evs
        .iter()
        .filter(|e| e.player_number == player_number && e.t_ms < feudal)
        .filter(|e| trains(e, &config::MILITIA_LINE))
        .count();

    let mut opened: Vec<(i32, &str)> = config::OPENER_LINES
        .iter()
        .filter_map(|(ids, tag)| {
            evs.iter()
                .filter(|e| {
                    e.player_number == player_number && e.t_ms >= feudal && e.t_ms < castle_or
                })
                .filter(|e| trains(e, ids))
                .map(|e| e.t_ms)
                .min()
                .map(|t| (t, *tag))
        })
        .collect();
    opened.sort_by_key(|&(t, _)| t);

    let feudal_tags: Vec<&str> = opened.iter().take(2).map(|&(_, tag)| tag).collect();
    let body = match feudal_tags.as_slice() {
        [] if castle_ms.is_some() => Some("Fast Castle".to_string()),
        [] => None,
        [one] => Some((*one).to_string()),
        [a, b] => Some(format!("{a} into {b}")),
        _ => unreachable!(),
    }?;
    Some(if dark_militia >= 3 {
        format!("Drush + {body}")
    } else {
        body
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
    use pipeline_core::GameCivId;

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
        assert!(s.feudal_t.is_some(), "feudal itself WAS reached");
        assert_eq!(s.castle_t, None);
    }

    #[test]
    fn opening_is_none_when_feudal_was_never_reached() {
        let p = parsed(vec![player(5001, 1, FRANKS)], vec![]);
        let s = &derive(&p).expect("well-formed test replay must derive cleanly")[0];
        assert_eq!(s.opening, None);
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
}
