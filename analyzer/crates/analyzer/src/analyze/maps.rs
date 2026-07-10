//! Map id -> (name, family), parsed from the committed `data/maps.tsv`.
//!
//! `family` (col 3) drives whether a team game gets a flank/pocket role tag:
//! only `open`/`closed` do. The notes column (col 4) is human reference, ignored here.
//!
//! `include_str!` target is `reference-data/maps.tsv` (single
//! source of truth — see `analyze/data.rs` for why this isn't duplicated here).

use std::collections::HashMap;

use crate::analyze::model::Family;

pub struct MapTable {
    by_id: HashMap<u32, (String, Family)>,
}

fn family_from(s: &str) -> Family {
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

pub fn parse(tsv: &str) -> MapTable {
    let mut by_id = HashMap::new();
    for line in tsv.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        // columns: id<TAB>name<TAB>family<TAB>notes(ignored). splitn(4) keeps notes out of `fam`.
        let mut it = line.splitn(4, '\t');
        let (Some(id), Some(name), Some(fam)) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        if let Ok(id) = id.trim().parse::<u32>() {
            by_id.insert(id, (name.trim().to_string(), family_from(fam)));
        }
    }
    MapTable { by_id }
}

pub fn load() -> MapTable {
    parse(include_str!(
        "../../../../../reference-data/maps.tsv"
    ))
}

/// `games.map` slug = lowercase alphanumeric of the map name ("Black Forest" -> "blackforest",
/// "African Clearing" -> "africanclearing"). Used to key the benchmark by map.
pub fn slug(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

impl MapTable {
    pub fn lookup(&self, id: u32) -> (&str, Family) {
        self.by_id
            .get(&id)
            .map(|(n, f)| (n.as_str(), *f))
            .unwrap_or(("Unknown", Family::Other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_maps() {
        let t = parse(
            "9\tArabia\topen\twiki note\n29\tArena\tclosed\twiki note\n149\tAfrican Clearing\tnomad\twiki note\n# comment\n",
        );
        assert_eq!(t.lookup(9), ("Arabia", Family::Open));
        assert_eq!(t.lookup(29), ("Arena", Family::Closed));
        assert_eq!(t.lookup(149).1, Family::Nomad);
        assert_eq!(t.lookup(99999), ("Unknown", Family::Other));
    }

    #[test]
    fn real_file_has_arabia_open_and_nomad_other() {
        let t = load();
        assert_eq!(t.lookup(9), ("Arabia", Family::Open));
        assert_eq!(t.lookup(33).1, Family::Nomad); // Nomad
        assert_eq!(t.lookup(149).1, Family::Nomad); // African Clearing
    }
}
