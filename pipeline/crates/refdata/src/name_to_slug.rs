//! aoe2techtree display name -> content slug, for `game-facts.json` (the 48 units the site's
//! unit pages carry facts for). Order is NOT load-bearing: `game-facts.json`'s `units` map is a
//! [`std::collections::BTreeMap`] (alphabetical), and its sole consumer
//! (`src/pages/[lang]/units/[unit].astro`) does a keyed lookup, never iterates in order.
//!
//! Uses the authoritative aoe2techtree game names (the `<b>...</b>` help-string names), including
//! the three that differ from the old aalises list (`Arbalester`/`Camel Rider`/`Heavy Camel Rider`,
//! not aalises's `Arbalest`/`Camel`/`Heavy Camel`).
//!
//! Deliberately EXCLUDES three units aoe2techtree HAS but the aalises-based committed file (and the
//! site) don't track (`Eagle Scout`, `Fire Galley`, `Demolition Raft`) — including them would make
//! `game-facts.json` a 51-unit file, silently expanding its scope past the site's 48. Add them here
//! (they resolve + derive cleanly) if/when the site grows pages for them.
pub const NAME_TO_SLUG: &[(&str, &str)] = &[
    ("Militia", "militia"),
    ("Man-at-Arms", "man-at-arms"),
    ("Long Swordsman", "long-swordsman"),
    ("Two-Handed Swordsman", "two-handed-swordsman"),
    ("Champion", "champion"),
    ("Spearman", "spearman"),
    ("Pikeman", "pikeman"),
    ("Halberdier", "halberdier"),
    ("Eagle Warrior", "eagle-warrior"),
    ("Elite Eagle Warrior", "elite-eagle-warrior"),
    ("Archer", "archer"),
    ("Crossbowman", "crossbowman"),
    ("Arbalester", "arbalester"),
    ("Skirmisher", "skirmisher"),
    ("Elite Skirmisher", "elite-skirmisher"),
    ("Cavalry Archer", "cavalry-archer"),
    ("Heavy Cavalry Archer", "heavy-cavalry-archer"),
    ("Hand Cannoneer", "hand-cannoneer"),
    ("Scout Cavalry", "scout-cavalry"),
    ("Light Cavalry", "light-cavalry"),
    ("Hussar", "hussar"),
    ("Knight", "knight"),
    ("Cavalier", "cavalier"),
    ("Paladin", "paladin"),
    ("Camel Rider", "camel-rider"),
    ("Heavy Camel Rider", "heavy-camel-rider"),
    ("Monk", "monk"),
    ("Battering Ram", "battering-ram"),
    ("Capped Ram", "capped-ram"),
    ("Siege Ram", "siege-ram"),
    ("Mangonel", "mangonel"),
    ("Onager", "onager"),
    ("Siege Onager", "siege-onager"),
    ("Scorpion", "scorpion"),
    ("Heavy Scorpion", "heavy-scorpion"),
    ("Bombard Cannon", "bombard-cannon"),
    ("Trebuchet", "trebuchet"),
    ("Galley", "galley"),
    ("War Galley", "war-galley"),
    ("Galleon", "galleon"),
    ("Fire Ship", "fire-ship"),
    ("Fast Fire Ship", "fast-fire-ship"),
    ("Demolition Ship", "demolition-ship"),
    ("Heavy Demolition Ship", "heavy-demolition-ship"),
    ("Cannon Galleon", "cannon-galleon"),
    ("Elite Cannon Galleon", "elite-cannon-galleon"),
    ("Fishing Ship", "fishing-ship"),
    ("Transport Ship", "transport-ship"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_forty_eight_entries() {
        assert_eq!(NAME_TO_SLUG.len(), 48);
    }

    #[test]
    fn uses_the_authoritative_game_names() {
        let names: Vec<&str> = NAME_TO_SLUG.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"Arbalester"));
        assert!(names.contains(&"Camel Rider"));
        assert!(names.contains(&"Heavy Camel Rider"));
        assert!(!names.contains(&"Eagle Scout"), "site doesn't track this yet");
    }

    #[test]
    fn every_slug_is_unique() {
        let mut slugs: Vec<&str> = NAME_TO_SLUG.iter().map(|(_, s)| *s).collect();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), NAME_TO_SLUG.len());
    }
}
