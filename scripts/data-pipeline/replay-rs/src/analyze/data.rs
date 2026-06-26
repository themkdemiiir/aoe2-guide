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
    // villager medians intentionally omitted: historical match_ages villagers are per-age,
    // not cumulative like the analyzer's count — bases differ, so we don't compare them in v1.
}

// civ -> map(slug) -> elo_bucket -> mode -> Slice. Map/civ rollups live under the "all" key.
type Civs = HashMap<String, HashMap<String, HashMap<String, HashMap<String, Slice>>>>;

#[derive(Debug, Deserialize)]
struct BenchmarkFile {
    civs: Civs,
}

#[derive(Debug)]
pub struct Benchmark(Civs);

impl Benchmark {
    /// Resolve `(civ, map, bucket, mode)` → `(slice, exact?)`. Falls back WITHIN the map to
    /// the `(civ, map, "all", "all")` rollup, and returns `None` if the map has no data at
    /// all — so we never compare a player against an unrelated map's median. `exact` is
    /// false when a rollup was used, so the coaching note can be worded honestly.
    pub fn slice(&self, civ: &str, map: &str, bucket: &str, mode: &str) -> Option<(&Slice, bool)> {
        let c = self.0.get(civ)?;
        if let Some(s) = c.get(map).and_then(|m| m.get(bucket)).and_then(|b| b.get(mode)) {
            return Some((s, true));
        }
        c.get(map)
            .and_then(|m| m.get("all"))
            .and_then(|b| b.get("all"))
            .map(|s| (s, false))
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
        let (s, exact) = b.slice("franks", "arabia", "1400-1649", "team").expect("franks arabia");
        assert!(exact);
        assert!(s.castle_s.unwrap() > 1000.0); // arabia Castle ~21 min
        // arena is much faster to Castle (Fast Castle) — proves map-specificity
        let (a, _) = b.slice("franks", "arena", "1400-1649", "team").expect("franks arena");
        assert!(a.castle_s.unwrap() < s.castle_s.unwrap());
        // a sparse 1v1 cell falls back to the map rollup (exact = false)
        let (_, ex) = b.slice("franks", "arabia", "1400-1649", "1v1").expect("fallback");
        assert!(!ex);
        // unknown map -> None (never compare against an unrelated map)
        assert!(b.slice("franks", "no_such_map", "1400-1649", "team").is_none());
    }

    #[test]
    fn civs_maps_id_to_slug() {
        let c = load_civs();
        assert_eq!(c.get(&2).map(String::as_str), Some("franks"));
        assert_eq!(c.get(&1).map(String::as_str), Some("britons"));
    }
}
