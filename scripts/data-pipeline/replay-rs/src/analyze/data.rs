//! Load the committed `costs.json`, `benchmark.json`, and `civs.tsv` (baked in via
//! `include_str!`), and bucket an ELO. No DB/network at runtime.

use std::collections::HashMap;

use serde::Deserialize;

// --- costs (float estimator) -------------------------------------------------
#[derive(Debug, Clone, Copy, Deserialize, Default)]
pub struct Cost {
    pub food: f64,
    pub wood: f64,
    pub gold: f64,
    pub stone: f64,
}

#[derive(Debug, Deserialize)]
pub struct Costs {
    pub unit: HashMap<u16, Cost>,
    pub building: HashMap<i64, Cost>,
    pub tech: HashMap<u16, Cost>,
}

pub fn load_costs() -> Costs {
    serde_json::from_str(include_str!("../../data/costs.json")).expect("costs.json")
}

// --- benchmark (age timing medians) ------------------------------------------
#[derive(Debug, Clone, Deserialize)]
pub struct Slice {
    pub feudal_s: Option<f64>,
    pub castle_s: Option<f64>,
    pub imperial_s: Option<f64>,
    /// Median villagers trained by Castle Age among WINNERS of this slice.
    /// Basis verified comparable to the analyzer's vils@Castle: match_ages
    /// `villagers` are per-AGE-WINDOW counts (dark med 23 / feudal 11 /
    /// castle 42 — windows, not snapshots), so dark+feudal summed = trained by
    /// castle-up, same as our cumulative DeQueue count (both exclude starting
    /// villagers). Built by build-benchmark-vils.{sql,mjs}; absent cells stay None.
    #[serde(default)]
    pub vils_castle: Option<f64>,
}

// civ -> map(slug) -> elo_bucket -> mode -> Slice. Map/civ rollups live under the "all" key.
type Civs = HashMap<String, HashMap<String, HashMap<String, HashMap<String, Slice>>>>;

#[derive(Debug, Deserialize)]
struct BenchmarkFile {
    civs: Civs,
}

/// How precisely a benchmark slice matched the request — lets the coaching note be worded honestly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// exact `(civ, map, bucket, mode)` cell
    Exact,
    /// same-mode map rollup `(civ, map, "all", mode)` — right mode, all elo
    MapMode,
    /// map all-mode rollup `(civ, map, "all", "all")` — mode-agnostic (team-heavy for a 1v1 request)
    MapAll,
}

#[derive(Debug)]
pub struct Benchmark(Civs);

impl Benchmark {
    /// Resolve `(civ, map, bucket, mode)` → `(slice, kind)`. Falls back WITHIN the map:
    /// exact → same-mode rollup `(civ, map, "all", mode)` → all-mode rollup
    /// `(civ, map, "all", "all")` → `None` if the map has no data. So we never compare a player
    /// against an unrelated map, and a 1v1 request prefers 1v1 data before degrading to the
    /// team-heavy all-mode median. `kind` lets the coaching note say which baseline was used.
    pub fn slice(&self, civ: &str, map: &str, bucket: &str, mode: &str) -> Option<(&Slice, MatchKind)> {
        let m = self.0.get(civ)?.get(map)?;
        if let Some(s) = m.get(bucket).and_then(|b| b.get(mode)) {
            return Some((s, MatchKind::Exact));
        }
        let all = m.get("all")?;
        if let Some(s) = all.get(mode) {
            return Some((s, MatchKind::MapMode));
        }
        all.get("all").map(|s| (s, MatchKind::MapAll))
    }
}

pub fn load_benchmark() -> Benchmark {
    let f: BenchmarkFile =
        serde_json::from_str(include_str!("../../data/benchmark.json")).expect("benchmark.json");
    Benchmark(f.civs)
}

// --- civ_id -> slug (benchmark key) ------------------------------------------
pub fn load_civs() -> HashMap<u32, String> {
    include_str!("../../data/civs.tsv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let mut it = l.splitn(2, '\t');
            Some((it.next()?.trim().parse().ok()?, it.next()?.trim().to_string()))
        })
        .collect()
}

// --- elo bucketing -----------------------------------------------------------
/// source: existing `games.elo_bucket` boundaries.
pub fn elo_bucket(elo: i32) -> &'static str {
    match elo {
        i32::MIN..=999 => "<1000",
        1000..=1199 => "1000-1199",
        1200..=1399 => "1200-1399",
        1400..=1649 => "1400-1649",
        1650..=1799 => "1650-1799",
        1800..=1999 => "1800-1999",
        2000..=2199 => "2000-2199",
        2200..=2499 => "2200-2499",
        _ => "2500+",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_known_costs() {
        let c = load_costs();
        assert_eq!(c.unit[&83].food, 50.0); // villager
        assert_eq!(c.building[&70].wood, 25.0); // house
        assert_eq!(c.unit[&38].gold, 75.0); // knight
    }

    #[test]
    fn elo_bucket_boundaries() {
        assert_eq!(elo_bucket(999), "<1000");
        assert_eq!(elo_bucket(1300), "1200-1399");
        assert_eq!(elo_bucket(2600), "2500+");
    }

    #[test]
    fn benchmark_slice_is_map_specific_with_fallback() {
        let b = load_benchmark();
        // exact team cell on arabia
        let (s, kind) = b.slice("franks", "arabia", "1400-1649", "team").expect("franks arabia");
        assert_eq!(kind, MatchKind::Exact);
        assert!(s.castle_s.unwrap() > 1000.0); // arabia Castle ~21 min
        // arena is much faster to Castle (Fast Castle) — proves map-specificity
        let (a, _) = b.slice("franks", "arena", "1400-1649", "team").expect("franks arena");
        assert!(a.castle_s.unwrap() < s.castle_s.unwrap());
        // the 2026-07 regen keys mode by games.ladder, so REAL 1v1 cells exist now
        let (v, kind2) = b.slice("franks", "arabia", "1400-1649", "1v1").expect("1v1 exact");
        assert_eq!(kind2, MatchKind::Exact);
        // and they carry the winner vils@Castle medians (build-benchmark-vils)
        assert!(v.vils_castle.expect("vils merged") > 20.0);
        // unknown map -> None (never compare against an unrelated map)
        assert!(b.slice("franks", "no_such_map", "1400-1649", "team").is_none());
    }

    #[test]
    fn slice_prefers_same_mode_rollup_over_all_mode() {
        use std::collections::HashMap;
        let mk = |feudal: f64| Slice { feudal_s: Some(feudal), castle_s: None, imperial_s: None, vils_castle: None };
        let mut modes = HashMap::new();
        modes.insert("1v1".to_string(), mk(100.0));
        modes.insert("all".to_string(), mk(200.0)); // all-mode (team-heavy) is slower
        let mut buckets = HashMap::new();
        buckets.insert("all".to_string(), modes);
        let mut maps = HashMap::new();
        maps.insert("arabia".to_string(), buckets);
        let mut civs = HashMap::new();
        civs.insert("franks".to_string(), maps);
        let b = Benchmark(civs);
        // a 1v1 request with no exact cell must prefer the (map,'all','1v1') rollup
        // over the team-heavy (map,'all','all') rollup.
        let (s, kind) = b.slice("franks", "arabia", "1400-1649", "1v1").expect("rollup");
        assert_eq!(kind, MatchKind::MapMode);
        assert_eq!(s.feudal_s, Some(100.0));
    }

    #[test]
    fn civs_maps_id_to_slug() {
        let c = load_civs();
        assert_eq!(c.get(&2).map(String::as_str), Some("franks"));
        assert_eq!(c.get(&1).map(String::as_str), Some("britons"));
    }
}
