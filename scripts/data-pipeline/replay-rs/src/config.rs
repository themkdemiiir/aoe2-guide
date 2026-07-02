//! Shared configuration + semantic constants for the replay pipeline.
//!
//! Direct port of `scripts/data-pipeline/replays/config.py`. Everything tunable
//! (API base, rate limits, AoE2 DE ids, manifest status vocabulary) lives here so
//! the rest of the crate stays free of magic values.

// --- World's Edge Link API ---------------------------------------------------
pub const API_BASE: &str = "https://aoe-api.worldsedgelink.com/community/leaderboard";
pub const TITLE: &str = "age2";
pub const UA: &str = "aoe2guide-replays/1.0 (self-collect)";

/// getReplayFiles is rate-limited (~120/min per the rlink docs); stay safely under.
pub const REPLAYFILES_PER_MIN: u32 = 100;
/// Max matchIDs per getReplayFiles call.
pub const REPLAYFILES_BATCH: usize = 10;

// --- semantic decoding (AoE2 DE ids) -----------------------------------------
/// Research tech ids that mark an age-up. Maps to the canonical age name.
pub fn age_name(technology_type: u16) -> Option<&'static str> {
    match technology_type {
        101 => Some("feudal"),
        102 => Some("castle"),
        103 => Some("imperial"),
        _ => None,
    }
}

/// The three ages, in order, used for the per-player `ages` rows.
pub const AGES: [&str; 3] = ["feudal", "castle", "imperial"];

pub const VILLAGER_ID: u16 = 83;

/// Units that are economy/utility, not combat. villager(83), fishing ship(13),
/// trade cart(128), trade cog(354), and 17. Matches `config.ECO_UNIT_IDS`.
pub const ECO_UNIT_IDS: [u16; 5] = [83, 13, 128, 354, 17];

/// Canonical m:ss formatter (single copy — report/compare/main all render times).
pub fn mmss(total_secs: u32) -> String {
    format!("{}:{:02}", total_secs / 60, total_secs % 60)
}

pub fn is_eco_unit(unit_id: u16) -> bool {
    ECO_UNIT_IDS.contains(&unit_id)
}

// --- manifest status vocabulary (one place so producers/consumers agree) -----
pub const PENDING: &str = "pending"; // not yet looked up
#[allow(dead_code)] // transient pre-parse state; kept for status-vocab parity with manifest.py
pub const DOWNLOADABLE: &str = "downloadable"; // usable URL (transient, pre-parse)
pub const PARSED: &str = "parsed"; // downloaded + parsed + stored
pub const NO_REPLAY: &str = "no_replay"; // files exist but all size -1
pub const EXPIRED: &str = "expired"; // getReplayFiles NOT_FOUND (aged out)
pub const PARSE_FAILED: &str = "parse_failed"; // downloaded but could not parse
pub const ERROR: &str = "error"; // transient failure; safe to retry
