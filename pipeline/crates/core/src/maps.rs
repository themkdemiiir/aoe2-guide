//! Map id -> `{ name, slug, family }`, parsed from the committed
//! `scripts/data-pipeline/replay-rs/data/maps.tsv` (SINGLE SOURCE OF TRUTH — read here, never
//! copied or modified).
//!
//! Mirrors `replay-rs/src/analyze/maps.rs::parse`'s structural parse exactly: `splitn(4, '\t')`
//! (id, name, family, notes — notes ignored), skipping `#`-comment and blank lines. `family`
//! (col 3) is the same closed enum as `replay-rs::analyze::model::Family`.

use std::collections::HashMap;

use crate::slug::slug;

/// Map family, as classified in `maps.tsv` col 3. Only `Open`/`Closed` drive flank/pocket
/// coaching upstream; the rest are informational.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Open,
    Closed,
    Hybrid,
    Water,
    Nomad,
    Special,
    Other,
}

impl Family {
    fn parse(s: &str) -> Family {
        match s.trim() {
            "open" => Family::Open,
            "closed" => Family::Closed,
            "hybrid" => Family::Hybrid,
            "water" => Family::Water,
            "nomad" => Family::Nomad,
            "special" => Family::Special,
            _ => Family::Other,
        }
    }
}

/// One `maps.tsv` row, id-keyed inside `MapTable`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapInfo {
    pub name: String,
    /// `slug::slug(name)` — the `games.map` key.
    pub slug: String,
    pub family: Family,
}

#[derive(Debug, Clone, Default)]
pub struct MapTable {
    by_id: HashMap<u32, MapInfo>,
}

impl MapTable {
    /// Looks up a map by its `map_id`. `None` for an id `maps.tsv` doesn't cover.
    pub fn get(&self, id: u32) -> Option<&MapInfo> {
        self.by_id.get(&id)
    }
}

/// Parses `maps.tsv`-shaped text. Exposed standalone (rather than only via [`load`]) so tests can
/// exercise the parser against small inline fixtures as well as the real committed file.
pub fn parse(tsv: &str) -> MapTable {
    let mut by_id = HashMap::new();
    for line in tsv.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut it = line.splitn(4, '\t');
        let (Some(id), Some(name), Some(family)) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        if let Ok(id) = id.trim().parse::<u32>() {
            let name = name.trim().to_string();
            let map_slug = slug(&name);
            by_id.insert(
                id,
                MapInfo {
                    slug: map_slug,
                    name,
                    family: Family::parse(family),
                },
            );
        }
    }
    MapTable { by_id }
}

/// Loads the real, committed `maps.tsv`, baked into the binary at compile time.
pub fn load() -> MapTable {
    parse(include_str!(
        "../../../../scripts/data-pipeline/replay-rs/data/maps.tsv"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_maps_from_a_fixture() {
        let t = parse(
            "9\tArabia\topen\twiki note\n29\tArena\tclosed\twiki note\n149\tAfrican Clearing\tnomad\twiki note\n# comment\n",
        );
        let arabia = t.get(9).expect("id 9");
        assert_eq!(arabia.name, "Arabia");
        assert_eq!(arabia.slug, "arabia");
        assert_eq!(arabia.family, Family::Open);

        let arena = t.get(29).expect("id 29");
        assert_eq!(arena.name, "Arena");
        assert_eq!(arena.family, Family::Closed);

        assert_eq!(t.get(99999), None);
    }

    #[test]
    fn real_file_has_arabia_open_and_arena_closed() {
        let t = load();
        let arabia = t.get(9).expect("id 9 present in real maps.tsv");
        assert_eq!(arabia.name, "Arabia");
        assert_eq!(arabia.slug, "arabia");
        assert_eq!(arabia.family, Family::Open);

        let arena = t.get(29).expect("id 29 present in real maps.tsv");
        assert_eq!(arena.name, "Arena");
        assert_eq!(arena.family, Family::Closed);
    }
}
