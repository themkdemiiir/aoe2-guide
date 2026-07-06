//! Semantic constants [`crate::parse`] needs — the subset of the OLD extractor's
//! `scripts/data-pipeline/replay-rs/src/config.rs` that the PARSER actually reads (age-marker
//! tech ids, eco-unit classification). The API base/rate-limit/manifest-status constants in the
//! old file belong to the not-yet-landed `fetch` shell, not to this pure parser — they stay
//! behind in the old crate.

/// Research tech ids that mark an age-up, mapped to the canonical age-name string that
/// [`pipeline_core::Age::try_from`] expects (see `parse::parse`'s age-boundary loop).
pub(crate) fn age_name(technology_type: u16) -> Option<&'static str> {
    match technology_type {
        101 => Some("feudal"),
        102 => Some("castle"),
        103 => Some("imperial"),
        _ => None,
    }
}

/// The reverse of [`age_name`] — the age-up tech id for a canonical age name. Lets
/// `derive::first_age_click` resolve the CLICK tech id to hand to `derive::first_research_ms`
/// without re-hardcoding 101/102/103 a second time outside this module.
pub(crate) fn age_tech_id(age: &str) -> Option<u16> {
    match age {
        "feudal" => Some(101),
        "castle" => Some(102),
        "imperial" => Some(103),
        _ => None,
    }
}

/// The three ages, in order, a replay's summary ever reaches — a replay never records a player
/// still in the Dark Age at summary time (see `pipeline_core::age::Age`'s module doc for why the
/// core enum still has a fourth, `Dark`, variant that this parser never emits).
pub(crate) const AGES: [&str; 3] = ["feudal", "castle", "imperial"];

pub(crate) const VILLAGER_ID: u16 = 83;

/// Units that are economy/utility, not combat: villager(83), fishing ship(13), trade cart(128),
/// trade cog(354), and 17.
const ECO_UNIT_IDS: [u16; 5] = [83, 13, 128, 354, 17];

pub(crate) fn is_eco_unit(unit_id: u16) -> bool {
    ECO_UNIT_IDS.contains(&unit_id)
}

// --- opening classification (ported for `crate::derive::classify_opening`) --------------------
// source: analyzer/crates/analyzer/src/analyze/metrics.rs (MILITIA_LINE/OPENER_LINES)

use pipeline_core::OpeningKind;

/// Dark-Age militia-line training, counted toward a "Drush" prefix. Feudal-opening unit lines.
/// source: .cache/aoe2-data data.json, ids verified by COST (internal names are legacy-shifted):
/// 74 Militia 50F/20G, 75 Man-at-Arms, 93 Spearman 35F/25W, 4 Archer, 7 Skirmisher 25F/35W,
/// 448 Scout Cavalry, 751 Eagle Scout.
pub(crate) const MILITIA_LINE: [u16; 2] = [74, 75];

/// `(unit ids, rich display tag, its OpeningKind)` — the [`OpeningKind`] rides ALONGSIDE the
/// display tag in the SAME array entry (not a second parallel lookup) so the two can never drift:
/// see `pipeline_core::opening`'s module doc for why `M@A` reconciles onto
/// [`OpeningKind::ManAtArms`] (aoestats' own spelling) while `Skirms`/`Eagles`/`Spears` stay their
/// own variants (aoestats has no equivalent to fold them into).
pub(crate) const OPENER_LINES: &[(&[u16], &str, OpeningKind)] = &[
    (&[448], "Scouts", OpeningKind::Scouts),
    (&[4], "Archers", OpeningKind::Archers),
    (&[7], "Skirms", OpeningKind::Skirms),
    (&[751], "Eagles", OpeningKind::Eagles),
    (&[74, 75], "M@A", OpeningKind::ManAtArms),
    (&[93], "Spears", OpeningKind::Spears),
];

// --- age-up research (click -> completion), ported for `crate::derive::age_research_s` -------
// source: analyzer/crates/analyzer/src/analyze/compare.rs

/// Baseline no-bonus age-up research time (seconds). source: aoe2techtree data.json Tech
/// 101/102/103 ResearchTime — Feudal 130s, Castle 160s, Imperial 190s.
pub(crate) const FEUDAL_RES_S: f64 = 130.0;
pub(crate) const CASTLE_RES_S: f64 = 160.0;
pub(crate) const IMP_RES_S: f64 = 190.0;

/// Malay's Feudal-Age bonus ("Advancing to the next Age is 66% faster" — src/data/
/// civilizations.json malay bonus; AoE2 wiki/Liquipedia Feudal Age). "66% faster" = research-rate
/// ×1.66 ⇒ time = base / 1.66, confirmed by the wiki's Malay Feudal 78s (=130/1.66) and Castle
/// 96s (=160/1.66). The only civ (today) whose age-up research deviates from baseline.
pub(crate) const MALAY_AGE_FACTOR: f64 = 1.0 / 1.66;

// --- watched eco-tech research timings, ported for `crate::derive::player_techs` (Phase D,
// `task-enrichD`) --------------------------------------------------------------------------------
// source: analyzer/crates/analyzer/src/analyze/metrics.rs:12-23 (WATCHED_TECHS), ids verified vs
// aoe2techtree data.json: 22 Loom, 213 Wheel Barrow, 249 Hand Cart, 202 Double Bit Axe, 203 Bow
// Saw, 14 Horse Collar, 13 Heavy Plow, 55 Gold Mining, 182 Gold Shaft Mining, 278 Stone Mining.

/// Watched eco upgrades + display name. Names are for a later exporter/provenance only (Phase E)
/// — `derive::player_techs` uses just the ids; the pipeline stores tech_id alone, no name column
/// (see `m20260706_000014_create_match_player_techs.rs`'s doc for why).
pub(crate) const WATCHED_TECHS: &[(u16, &str)] = &[
    (22, "Loom"),
    (213, "Wheelbarrow"),
    (249, "Hand Cart"),
    (202, "Double-Bit Axe"),
    (203, "Bow Saw"),
    (14, "Horse Collar"),
    (13, "Heavy Plow"),
    (55, "Gold Mining"),
    (182, "Gold Shaft Mining"),
    (278, "Stone Mining"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_name_maps_the_three_age_up_tech_ids() {
        assert_eq!(age_name(101), Some("feudal"));
        assert_eq!(age_name(102), Some("castle"));
        assert_eq!(age_name(103), Some("imperial"));
        assert_eq!(age_name(100), None);
        assert_eq!(age_name(0), None);
    }

    #[test]
    fn is_eco_unit_matches_the_five_known_ids() {
        for id in [83, 13, 128, 354, 17] {
            assert!(is_eco_unit(id));
        }
        assert!(!is_eco_unit(75)); // a military unit id, not eco
    }

    #[test]
    fn age_tech_id_is_the_exact_reverse_of_age_name() {
        for id in [101, 102, 103] {
            let name = age_name(id).expect("101/102/103 must map to a name");
            assert_eq!(age_tech_id(name), Some(id), "age_tech_id must invert age_name for {id}");
        }
        assert_eq!(age_tech_id("dark"), None);
        assert_eq!(age_tech_id("nonsense"), None);
    }

    #[test]
    fn watched_techs_covers_the_ten_ported_ids_verbatim() {
        // source: analyzer/crates/analyzer/src/analyze/metrics.rs:12-23 — same order, same ids.
        let ids: Vec<u16> = WATCHED_TECHS.iter().map(|&(id, _)| id).collect();
        assert_eq!(ids, vec![22, 213, 249, 202, 203, 14, 13, 55, 182, 278]);
    }
}
