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
}
