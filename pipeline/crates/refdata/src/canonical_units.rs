//! The canonical list of trainable units emitted into `unit-stats.json`.
//!
//! [`CANONICAL_UNITS`] maps a unit's aoe2techtree display `name` (the `<b>...</b>` span of its
//! help string — see [`crate::techtree`]) → our content `slug`. **Order matters** — the `units`
//! array in `unit-stats.json` is emitted in this exact declaration order. A game unit whose
//! display name has no entry here is excluded from the output (this is how the source's 245 units
//! narrow to the 90 trainable units the site tracks).
//!
//! **Three names differ from the old aalises-sourced list**, because aalises used abbreviated
//! display names the actual game doesn't: aalises `"Arbalest"`/`"Camel"`/`"Heavy Camel"` are the
//! game's `"Arbalester"`/`"Camel Rider"`/`"Heavy Camel Rider"` (which already matched our slugs
//! `arbalester`/`camel-rider`/`heavy-camel-rider` better than aalises did). Now that the source IS
//! the game data, these use the authoritative game names — a display name here must be the exact
//! `<b>...</b>` text or [`crate::techtree::TechTree::resolve`] fails loud
//! ([`crate::error::RefdataError::UnitNotFound`]).

/// `(aoe2techtree_display_name, content_slug)`, in the exact order `unit-stats.json`'s `units`
/// array is emitted. See the module doc for why order is load-bearing here.
pub const CANONICAL_UNITS: &[(&str, &str)] = &[
    // Militia line
    ("Militia", "militia"),
    ("Man-at-Arms", "man-at-arms"),
    ("Long Swordsman", "long-swordsman"),
    ("Two-Handed Swordsman", "two-handed-swordsman"),
    ("Champion", "champion"),
    // Spear line
    ("Spearman", "spearman"),
    ("Pikeman", "pikeman"),
    ("Halberdier", "halberdier"),
    // Eagle line (Meso civs)
    ("Eagle Warrior", "eagle-warrior"),
    ("Elite Eagle Warrior", "elite-eagle-warrior"),
    // Archer line
    ("Archer", "archer"),
    ("Crossbowman", "crossbowman"),
    // aalises called this "Arbalest"; the game name is "Arbalester" (matches the slug).
    ("Arbalester", "arbalester"),
    // Skirmisher line
    ("Skirmisher", "skirmisher"),
    ("Elite Skirmisher", "elite-skirmisher"),
    // Cavalry Archer line
    ("Cavalry Archer", "cavalry-archer"),
    ("Heavy Cavalry Archer", "heavy-cavalry-archer"),
    // Hand Cannoneer
    ("Hand Cannoneer", "hand-cannoneer"),
    // Scout line
    ("Scout Cavalry", "scout-cavalry"),
    ("Light Cavalry", "light-cavalry"),
    ("Hussar", "hussar"),
    // Knight line
    ("Knight", "knight"),
    ("Cavalier", "cavalier"),
    ("Paladin", "paladin"),
    // Camel line — aalises "Camel"/"Heavy Camel"; the game names are "Camel Rider"/"Heavy Camel
    // Rider" (match the slugs).
    ("Camel Rider", "camel-rider"),
    ("Heavy Camel Rider", "heavy-camel-rider"),
    // Monk
    ("Monk", "monk"),
    // Siege
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
    // Navy
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
    // Unique units
    ("Berserk", "berserk"),
    ("Elite Berserk", "elite-berserk"),
    ("Cataphract", "cataphract"),
    ("Elite Cataphract", "elite-cataphract"),
    ("Chu Ko Nu", "chu-ko-nu"),
    ("Elite Chu Ko Nu", "elite-chu-ko-nu"),
    ("Conquistador", "conquistador"),
    ("Elite Conquistador", "elite-conquistador"),
    ("Huskarl", "huskarl"),
    ("Elite Huskarl", "elite-huskarl"),
    ("Jaguar Warrior", "jaguar-warrior"),
    ("Elite Jaguar Warrior", "elite-jaguar-warrior"),
    ("Janissary", "janissary"),
    ("Elite Janissary", "elite-janissary"),
    ("Longbowman", "longbowman"),
    ("Elite Longbowman", "elite-longbowman"),
    ("Mameluke", "mameluke"),
    ("Elite Mameluke", "elite-mameluke"),
    ("Mangudai", "mangudai"),
    ("Elite Mangudai", "elite-mangudai"),
    ("Plumed Archer", "plumed-archer"),
    ("Elite Plumed Archer", "elite-plumed-archer"),
    ("Samurai", "samurai"),
    ("Elite Samurai", "elite-samurai"),
    ("Tarkan", "tarkan"),
    ("Elite Tarkan", "elite-tarkan"),
    ("Teutonic Knight", "teutonic-knight"),
    ("Elite Teutonic Knight", "elite-teutonic-knight"),
    ("Throwing Axeman", "throwing-axeman"),
    ("Elite Throwing Axeman", "elite-throwing-axeman"),
    ("War Elephant", "war-elephant"),
    ("Elite War Elephant", "elite-war-elephant"),
    ("War Wagon", "war-wagon"),
    ("Elite War Wagon", "elite-war-wagon"),
    ("Woad Raider", "woad-raider"),
    ("Elite Woad Raider", "elite-woad-raider"),
    ("Karambit Warrior", "karambit-warrior"),
    ("Elite Karambit Warrior", "elite-karambit-warrior"),
    ("Longboat", "longboat"),
    ("Elite Longboat", "elite-longboat"),
    ("Turtle Ship", "turtle-ship"),
    ("Elite Turtle Ship", "elite-turtle-ship"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_exactly_ninety_entries() {
        assert_eq!(CANONICAL_UNITS.len(), 90);
    }

    #[test]
    fn every_slug_is_unique() {
        let mut slugs: Vec<&str> = CANONICAL_UNITS.iter().map(|(_, s)| *s).collect();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), CANONICAL_UNITS.len());
    }

    #[test]
    fn every_display_name_is_unique() {
        let mut names: Vec<&str> = CANONICAL_UNITS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CANONICAL_UNITS.len());
    }

    #[test]
    fn the_three_game_renamed_units_use_game_names() {
        let by_slug = |slug: &str| {
            CANONICAL_UNITS
                .iter()
                .find(|(_, s)| *s == slug)
                .map(|(n, _)| *n)
        };
        assert_eq!(by_slug("arbalester"), Some("Arbalester"));
        assert_eq!(by_slug("camel-rider"), Some("Camel Rider"));
        assert_eq!(by_slug("heavy-camel-rider"), Some("Heavy Camel Rider"));
    }
}
