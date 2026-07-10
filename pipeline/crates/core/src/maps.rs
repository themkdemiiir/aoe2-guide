//! Map id -> `{ name, slug, family }`, parsed from the committed
//! `reference-data/maps.tsv` (SINGLE SOURCE OF TRUTH — read here, never
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

    /// The exact lowercase label this variant casts to in Postgres's `map_family` enum
    /// (`m20260705_000001_create_enums.rs`). Deliberately exhaustive (no wildcard arm): adding a
    /// new [`Family`] variant without also giving it a DB label here is a `cargo build` failure,
    /// not a silent `'other'` fallback — the dims loader's "fail loud on an unmapped family" rule.
    pub const fn as_db_str(self) -> &'static str {
        match self {
            Family::Open => "open",
            Family::Closed => "closed",
            Family::Hybrid => "hybrid",
            Family::Water => "water",
            Family::Nomad => "nomad",
            Family::Special => "special",
            Family::Other => "other",
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

    /// Iterates every `(map_id, MapInfo)` pair — e.g. for the dims loader to load the whole
    /// reference table rather than look up one id at a time.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &MapInfo)> {
        self.by_id.iter().map(|(&id, info)| (id, info))
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
        "../../../../reference-data/maps.tsv"
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

    #[test]
    fn as_db_str_matches_the_map_family_enum_labels() {
        // Mirrors the exact literal list `CREATE TYPE map_family AS ENUM (...)` declares
        // (`m20260705_000001_create_enums.rs`) — a dims-loader spot check that these two never
        // drift apart.
        for (family, label) in [
            (Family::Open, "open"),
            (Family::Closed, "closed"),
            (Family::Hybrid, "hybrid"),
            (Family::Water, "water"),
            (Family::Nomad, "nomad"),
            (Family::Special, "special"),
            (Family::Other, "other"),
        ] {
            assert_eq!(family.as_db_str(), label);
            assert_eq!(
                Family::parse(label),
                family,
                "as_db_str must round-trip through parse"
            );
        }
    }

    #[test]
    fn iter_covers_every_row_including_a_known_map() {
        let t = parse("9\tArabia\topen\twiki note\n29\tArena\tclosed\twiki note\n");
        let by_id: HashMap<u32, &MapInfo> = t.iter().collect();
        assert_eq!(by_id.len(), 2);
        assert_eq!(by_id[&9].slug, "arabia");
        assert_eq!(by_id[&29].family, Family::Closed);
    }
}
