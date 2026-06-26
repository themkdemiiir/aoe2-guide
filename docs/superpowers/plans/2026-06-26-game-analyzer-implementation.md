# Game Analyzer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an `analyze` subcommand to the `replay-rs` crate that turns one ranked replay into a per-player macro-coaching scorecard + "top fixes".

**Architecture:** Reuse `aoe2rec::Savegame` to parse, walk the op stream once into a normalized `Vec<Ev>` + per-game meta + per-player ELO, compute pure per-player metrics, layer a light color/slot flank-pocket tag (open/closed maps only) + two coordinate metrics, compare 4 ways against committed `benchmark.json`, and render with `comfy-table` + `owo-colors`. Parser-coupled code (`walk.rs`) is isolated from pure, unit-tested metric logic (`metrics.rs`).

**Tech Stack:** Rust 2021, `aoe2rec` (vendored), `serde`/`serde_json`, `owo-colors`, `comfy-table`, existing `anyhow`/`reqwest`/`rayon`. Node (for the one-off `costs.json` generator), DuckDB (for the one-off `benchmark.json` generator).

## Global Constraints

- Crate root: `scripts/data-pipeline/replay-rs/`. All `cargo` commands run from there.
- New deps EXACTLY: `owo-colors = "4.3"`, `comfy-table = { version = "7.2", default-features = false }`. No clap, no other table/color crate.
- Keep the existing hand-rolled CLI style in `main.rs` (a `match` on subcommand + a `while` arg loop). Do NOT add a CLI framework.
- Committed data files live in `scripts/data-pipeline/replay-rs/data/` and are baked in with `include_str!` (no runtime file lookup). `maps.tsv` already exists.
- `vendor/` and `target/` are gitignored; never commit them. `data/*.json` and `data/maps.tsv` ARE committed.
- Source-derived only: every AoE2 constant in code carries a `// source:` comment. No fabricated numbers.
- All money/timing values are honest estimates where the spec says so; print the float-estimator caveat verbatim.
- AoE2 constants (verified): age techs Feudal=101 Castle=102 Imperial=103; villager unit=83; eco units=[83,13,128,354,17]; age research durations Feudal=130s Castle=160s Imperial=190s; villager train=25s; Build blob `x:f32@[4..8]`, `y:f32@[8..12]`, `building_id:i32@[12..16]`; PostGame leaderboard `id 3=1v1 RM, 4=team RM`, game `player_number = leaderboard player_number + 1`; effective gather ~0.30 res/s. ELO buckets: `<1000,1000-1199,1200-1399,1400-1649,1650-1799,1800-1999,2000-2199,2200-2499,2500+`.

---

## Review corrections (concrete — fold each into its task)

A correctness pass (against the vendored `aoe2rec`, `config.rs`, `extract.rs`, `data.json`, `maps.tsv`) found these. They override the task bodies where they conflict.

**C1 — civ_id → slug for benchmark lookup (Task 7 + 10).** `benchmark.json` is keyed by civ *slug* (`games.civ`), but the replay only gives `civ_id: u32`. Commit `data/civs.tsv` (already generated) and load it in `data.rs`:

```rust
pub fn load_civs() -> std::collections::HashMap<u32, String> {
    include_str!("../../data/civs.tsv").lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| { let mut it = l.splitn(2, '\t'); Some((it.next()?.trim().parse().ok()?, it.next()?.trim().to_string())) })
        .collect()
}
```
In `findings`, `civ_slug = civs.get(&pm.info.civ_id)`; if absent, skip that player's YourElo/Pro findings (never panic).

**C2 — benchmark carries a `1v1`|`team` mode (Task 7).** Don't restrict to 1v1 (the headline use case is team games). SQL: drop `WHERE g.team_size = 1`; add `CASE WHEN g.team_size = 1 THEN '1v1' ELSE 'team' END AS mode`, `GROUP BY civ, bucket, mode`. Shape: `{ civ: { bucket: { "1v1": Slice, "team": Slice } } }`; `Benchmark::slice(civ, bucket, mode)`. In Task 10, `mode = if it's a team game { "team" } else { "1v1" }`, and take the bucket from the matching ladder ELO (id 4 team, id 3 1v1).

**C3 — ms→seconds + click→completion offset at the compare boundary (Task 10).** Analyzer ages are ms CLICK times; `Slice.*_s` are SECONDS COMPLETION times. Convert before comparing:

```rust
const FEUDAL_RES_S: f64 = 130.0; const CASTLE_RES_S: f64 = 160.0; const IMP_RES_S: f64 = 190.0; // source: spec
let feudal_completion_s = pm.feudal_ms.map(|ms| ms as f64 / 1000.0 + FEUDAL_RES_S); // compare to slice.feudal_s
```

**C4 — idle windows: Option-safe, no underflow (Task 10).** `feudal_ms`/`castle_ms` are `Option<u32>`; `idle_tc_ms` takes `u32`:

```rust
let idle_dark = pm.feudal_ms.map(|f| idle_tc_ms(evs, pn, f)).unwrap_or(0);
let idle_feudal = match (pm.feudal_ms, pm.castle_ms) {
    (Some(f), Some(c)) => idle_tc_ms(evs, pn, c).saturating_sub(idle_tc_ms(evs, pn, f)),
    _ => 0,
};
```

**C5 — `Severity` ordered for the "top fixes" sort (Task 10/11).** `#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)] pub enum Severity { Low, Med, High }`; sort `by_key(|f| std::cmp::Reverse(f.severity))`. Derive `Debug, Clone` on `Finding`, `PlayerMetrics`, `Basis`, `Role`.

**C6 — Task 10 & 11 tests must be CONCRETE (no `unimplemented!`/empty-vec/`/* meta */`).** `#[derive(Debug, Clone)]` on `PlayerMetrics`, plus a builder:

```rust
fn pm(pn: i32, idle_dark_ms: u32, feudal_ms: Option<u32>) -> PlayerMetrics {
    PlayerMetrics { info: PlayerInfo { player_number: pn, profile_id: 0, civ_id: 2, name: "MKD".into(), color_id: pn, team_id: 2 },
        feudal_ms, castle_ms: None, imperial_ms: None, vils_castle: 0, idle_dark_ms, idle_feudal_ms: 0,
        first_military_ms: None, eco_techs: vec![], apm: 0.0, base: None, elo_1v1: None, elo_team: None, role: None }
}
fn empty_bench() -> Benchmark { Benchmark(std::collections::HashMap::new()) }
#[test] fn flags_high_dark_idle_tc() {
    let f = findings(&[pm(1, 80_000, Some(720_000))], &empty_bench(), &std::collections::HashMap::new(), Family::Open);
    assert!(f.iter().any(|x| x.metric.contains("idle TC") && matches!(x.severity, Severity::High)));
}
```
Task 11: build `pm(1, 80_000, Some(720_000))`, a concrete `Vec<Finding>` (or empty), a concrete `GameMeta`, then assert the report `contains("MKD")` and `contains("Top Fixes")`.

**C7 — minor smells:** `walk::decode_build` shares the `[12..16]` building-id offset with `extract::decode_build_id` — comment the shared contract. `base_centroid` uses `< window_ms` while `vils_at`/`idle_tc_ms` use `<= until` — standardize on `<=`. `build-benchmark.sql` reads `match_ages.{reached,uptime}` (present in the aoestats table); the future `replay_ages` rebuild has `uptime_ms`/no `reached` — adjust then. `assign_roles` tags all team sizes; the report should phrase 2v2/3v3 roles as "loose" (spec decision 3). `GameMeta.{team_positions,map_size}` are reserved (unused in v1).

---

## File Structure

```
scripts/data-pipeline/replay-rs/
  Cargo.toml                       MODIFY: add owo-colors, comfy-table
  data/maps.tsv                    EXISTS (id<TAB>name<TAB>family<TAB>notes; analyzer reads col1+col3)
  data/civs.tsv                    EXISTS (id<TAB>slug; civ_id -> benchmark civ key)
  data/costs.json                  NEW (generated; committed)
  data/benchmark.json              NEW (generated; committed)
  src/main.rs                      MODIFY: add "analyze" subcommand dispatch
  src/analyze/mod.rs               NEW: AnalyzeArgs, Input, run(), CLI parse
  src/analyze/model.rs             NEW: Ev, EvKind, GameMeta, PlayerMetrics, GameMetrics, Family, Role, Basis, Severity, Finding
  src/analyze/maps.rs              NEW: parse maps.tsv -> id -> (name, Family)
  src/analyze/walk.rs              NEW: &Savegame -> (GameMeta, Vec<Ev>, players, EloTable)
  src/analyze/metrics.rs           NEW: pure fns over &[Ev] -> PlayerMetrics fields
  src/analyze/data.rs              NEW: load costs.json + benchmark.json; elo_bucket()
  src/analyze/position.rs          NEW: assign_roles() + coord_metrics()
  src/analyze/float.rs             NEW: float estimator
  src/analyze/compare.rs           NEW: 4-way -> Vec<Finding>
  src/analyze/report.rs            NEW: render scorecard + top fixes
scripts/data-pipeline/
  build-costs.mjs                  NEW: aoe2techtree data.json -> data/costs.json
  build-benchmark.sql              NEW: DuckDB -> data/benchmark.json
```

---

## Task 1: Wire the `analyze` subcommand (skeleton)

**Files:**
- Modify: `Cargo.toml` (deps), `src/main.rs` (dispatch + `cmd_analyze`)
- Create: `src/analyze/mod.rs`, `src/analyze/model.rs`

**Interfaces:**
- Produces: `analyze::run(args: analyze::AnalyzeArgs) -> anyhow::Result<()>`; `analyze::AnalyzeArgs { input: Input, you: Option<String> }`; `enum Input { File(PathBuf), MatchId(i64) }`.

- [ ] **Step 1:** Add deps to `Cargo.toml` under `[dependencies]`:

```toml
owo-colors = "4.3"
comfy-table = { version = "7.2", default-features = false }
```

- [ ] **Step 2:** Create `src/analyze/model.rs` with the shared types (start minimal; later tasks extend):

```rust
//! Shared data types for the analyze subcommand.
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family { Open, Closed, Hybrid, Water, Nomad, Special, Other }

#[derive(Debug, Clone)]
pub enum Input { File(PathBuf), MatchId(i64) }

#[derive(Debug, Clone)]
pub struct AnalyzeArgs { pub input: Input, pub you: Option<String> }
```

- [ ] **Step 3:** Create `src/analyze/mod.rs`:

```rust
//! Replay -> per-player macro-coaching report.
pub mod model;
pub use model::{AnalyzeArgs, Input};

use anyhow::Result;

pub fn run(args: AnalyzeArgs) -> Result<()> {
    println!("analyze: input={:?} you={:?} (not yet implemented)", args.input, args.you);
    Ok(())
}
```

- [ ] **Step 4:** In `src/main.rs`, add `mod analyze;`, a `"analyze" => cmd_analyze(rest),` arm in the `match subcmd`, update `print_usage`, and add:

```rust
fn cmd_analyze(args: &[String]) -> Result<()> {
    use analyze::{AnalyzeArgs, Input};
    let mut input: Option<Input> = None;
    let mut you: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--match-id" => input = Some(Input::MatchId(take_value(args, &mut i, "--match-id")?.parse()?)),
            "--you" => you = Some(take_value(args, &mut i, "--you")?),
            v if !v.starts_with("--") => input = Some(Input::File(std::path::PathBuf::from(v))),
            other => bail!("analyze: unknown flag {other}"),
        }
        i += 1;
    }
    let input = input.ok_or_else(|| anyhow::anyhow!("analyze: need <file.aoe2record> or --match-id N"))?;
    analyze::run(AnalyzeArgs { input, you })
}
```

- [ ] **Step 5:** Run: `cargo build --release` — Expected: compiles. Then `./target/release/replay-rs analyze /path/to/any.aoe2record` prints the "not yet implemented" line.

- [ ] **Step 6:** Commit: `git add -A && git commit -m "feat(analyze): scaffold analyze subcommand + deps"`

---

## Task 2: Parse `maps.tsv` (map id → family)

**Files:**
- Create: `src/analyze/maps.rs`, `tests` inline.

**Interfaces:**
- Produces: `maps::MapTable` with `pub fn lookup(&self, id: u32) -> (&str, Family)` returning `("Unknown", Family::Other)` for unknown ids; `maps::load() -> MapTable` (from `include_str!("../../data/maps.tsv")`).

- [ ] **Step 1: Write the failing test** in `src/analyze/maps.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::model::Family;
    #[test]
    fn parses_known_maps() {
        let t = parse("9\tArabia\topen\twiki note\n29\tArena\tclosed\twiki note\n149\tAfrican Clearing\tnomad\twiki note\n# comment\n");
        assert_eq!(t.lookup(9), ("Arabia", Family::Open));
        assert_eq!(t.lookup(29), ("Arena", Family::Closed));
        assert_eq!(t.lookup(149).1, Family::Nomad);
        assert_eq!(t.lookup(99999), ("Unknown", Family::Other));
    }
}
```

- [ ] **Step 2:** Run `cargo test -p replay-rs maps::tests::parses_known_maps` — Expected: FAIL (no `parse`).

- [ ] **Step 3: Implement**:

```rust
//! Map id -> (name, family), parsed from the committed data/maps.tsv.
use std::collections::HashMap;
use crate::analyze::model::Family;

pub struct MapTable { by_id: HashMap<u32, (String, Family)> }

fn family_from(s: &str) -> Family {
    match s.trim() {
        "open" => Family::Open, "closed" => Family::Closed, "hybrid" => Family::Hybrid,
        "water" => Family::Water, "nomad" => Family::Nomad, "special" => Family::Special,
        _ => Family::Other,
    }
}

pub fn parse(tsv: &str) -> MapTable {
    let mut by_id = HashMap::new();
    for line in tsv.lines() {
        if line.starts_with('#') || line.trim().is_empty() { continue; }
        // columns: id<TAB>name<TAB>family<TAB>notes(ignored). splitn(4) keeps notes out of `fam`.
        let mut it = line.splitn(4, '\t');
        let (Some(id), Some(name), Some(fam)) = (it.next(), it.next(), it.next()) else { continue };
        if let Ok(id) = id.trim().parse::<u32>() {
            by_id.insert(id, (name.trim().to_string(), family_from(fam)));
        }
    }
    MapTable { by_id }
}

pub fn load() -> MapTable { parse(include_str!("../../data/maps.tsv")) }

impl MapTable {
    pub fn lookup(&self, id: u32) -> (&str, Family) {
        self.by_id.get(&id).map(|(n, f)| (n.as_str(), *f)).unwrap_or(("Unknown", Family::Other))
    }
}
```

- [ ] **Step 4:** Add `pub mod maps;` to `mod.rs`. Run the test — Expected: PASS.
- [ ] **Step 5:** Commit: `git commit -am "feat(analyze): map id->family table from maps.tsv"`

---

## Task 3: Normalized event model + metric pure functions — ages & villagers

**Files:**
- Modify: `src/analyze/model.rs` (add `Ev`, `EvKind`), create `src/analyze/metrics.rs`.

**Interfaces:**
- Produces: `model::Ev { player: i32, t_ms: u32, kind: EvKind }`; `enum EvKind { Train(u16), Research(u16), Build { id: i64, x: f32, y: f32 }, Move { x: f32, y: f32 }, Interact { x: f32, y: f32 }, Other }`.
- Produces: `metrics::age_clicks(evs, player) -> (Option<u32>, Option<u32>, Option<u32>)` (feudal,castle,imp ms); `metrics::vils_at(evs, player, until_ms) -> u32`.

- [ ] **Step 1:** Add to `model.rs`:

```rust
#[derive(Debug, Clone, Copy)]
pub enum EvKind {
    Train(u16),               // DeQueue unit_id (amount expanded to repeated counts by walk.rs)
    Research(u16),            // technology_type
    Build { id: i64, x: f32, y: f32 },
    Move { x: f32, y: f32 },
    Interact { x: f32, y: f32 },
    Other,
}
#[derive(Debug, Clone, Copy)]
pub struct Ev { pub player: i32, pub t_ms: u32, pub kind: EvKind }
```

- [ ] **Step 2: Write failing tests** in `src/analyze/metrics.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::model::{Ev, EvKind};
    fn ev(p: i32, t: u32, k: EvKind) -> Ev { Ev { player: p, t_ms: t, kind: k } }
    #[test]
    fn age_clicks_returns_earliest_of_each_age() {
        // age_clicks uses .min() over t_ms, so the earliest Research(101) wins.
        let evs = vec![
            ev(1, 600_000, EvKind::Research(101)),   // feudal
            ev(1, 1_000_000, EvKind::Research(102)), // castle
            ev(2, 700_000, EvKind::Research(101)),
        ];
        assert_eq!(age_clicks(&evs, 1), (Some(600_000), Some(1_000_000), None));
        assert_eq!(age_clicks(&evs, 2), (Some(700_000), None, None));
    }
    #[test]
    fn vils_at_counts_villager_trains_until_t() {
        let evs = vec![
            ev(1, 1000, EvKind::Train(83)), ev(1, 2000, EvKind::Train(83)),
            ev(1, 3000, EvKind::Train(38)),  // knight, not a villager
            ev(1, 9000, EvKind::Train(83)),
        ];
        assert_eq!(vils_at(&evs, 1, 5000), 2);
        assert_eq!(vils_at(&evs, 1, 100_000), 3);
    }
}
```

- [ ] **Step 3:** Run `cargo test -p replay-rs metrics::tests` — Expected: FAIL.
- [ ] **Step 4: Implement** in `src/analyze/metrics.rs`:

```rust
//! Pure per-player metric functions over a time-sorted &[Ev]. No parser coupling.
use crate::analyze::model::{Ev, EvKind};
use crate::config::{ECO_UNIT_IDS, VILLAGER_ID}; // reuse the validated parser constants — NO duplication

fn first_research(evs: &[Ev], player: i32, tech: u16) -> Option<u32> {
    evs.iter().filter(|e| e.player == player)
        .filter_map(|e| match e.kind { EvKind::Research(t) if t == tech => Some(e.t_ms), _ => None })
        .min()
}

/// (feudal, castle, imperial) age-up CLICK times in ms. source: techs 101/102/103.
pub fn age_clicks(evs: &[Ev], player: i32) -> (Option<u32>, Option<u32>, Option<u32>) {
    (first_research(evs, player, 101), first_research(evs, player, 102), first_research(evs, player, 103))
}

/// Cumulative villagers queued by `until_ms` (DeQueue of unit 83; amounts pre-expanded).
pub fn vils_at(evs: &[Ev], player: i32, until_ms: u32) -> u32 {
    evs.iter().filter(|e| e.player == player && e.t_ms <= until_ms)
        .filter(|e| matches!(e.kind, EvKind::Train(VILLAGER_ID))).count() as u32
}
```

- [ ] **Step 5:** Add `pub mod metrics;` to `mod.rs`. Run tests — Expected: PASS.
- [ ] **Step 6:** Commit: `git commit -am "feat(analyze): Ev model + age/villager metrics (TDD)"`

---

## Task 4: Metrics — idle TC, first military, eco techs, base centroid, APM

**Files:** Modify `src/analyze/metrics.rs`.

**Interfaces:**
- Produces: `idle_tc_ms(evs, player, until_ms) -> u32`; `first_military_ms(evs, player) -> Option<u32>`; `eco_tech_times(evs, player) -> Vec<(u16, u32)>`; `base_centroid(evs, player, window_ms) -> Option<(f32, f32)>`; `apm(evs, player, duration_ms) -> f64`.

- [ ] **Step 1: Write failing tests** (append to metrics tests):

```rust
#[test]
fn idle_tc_is_window_minus_busy() {
    // 2 villagers by 200s, window=200s => busy=2*25s=50s => idle=150s.
    let evs = vec![ev(1, 10_000, EvKind::Train(83)), ev(1, 20_000, EvKind::Train(83))];
    assert_eq!(idle_tc_ms(&evs, 1, 200_000), 150_000);
    // never negative
    let many: Vec<Ev> = (0..20).map(|k| ev(1, k * 1000, EvKind::Train(83))).collect();
    assert_eq!(idle_tc_ms(&many, 1, 100_000), 0);
}
#[test]
fn first_military_is_first_non_eco_train() {
    let evs = vec![ev(1, 1000, EvKind::Train(83)), ev(1, 5000, EvKind::Train(7)) /*skirm*/];
    assert_eq!(first_military_ms(&evs, 1), Some(5000));
}
#[test]
fn base_centroid_is_median_of_builds() {
    let evs = vec![
        ev(1, 1000, EvKind::Build { id: 70, x: 10.0, y: 70.0 }),
        ev(1, 2000, EvKind::Build { id: 70, x: 14.0, y: 74.0 }),
        ev(1, 3000, EvKind::Build { id: 562, x: 12.0, y: 72.0 }),
        ev(1, 999_000, EvKind::Build { id: 70, x: 200.0, y: 5.0 }), // outside window, ignored
    ];
    assert_eq!(base_centroid(&evs, 1, 360_000), Some((12.0, 72.0)));
}
```

- [ ] **Step 2:** Run `cargo test -p replay-rs metrics::tests` — Expected: FAIL.
- [ ] **Step 3: Implement** (append to `metrics.rs`):

```rust
pub const VIL_TRAIN_MS: u32 = 25_000; // source: villager train time 25s

/// Crude idle-TC estimate over [0, until]: window minus (villagers * 25s busy time).
/// Honest v1: assumes ~1 TC in the early game; flags BIG idle, not exact gaps. source: spec.
pub fn idle_tc_ms(evs: &[Ev], player: i32, until_ms: u32) -> u32 {
    let busy = vils_at(evs, player, until_ms).saturating_mul(VIL_TRAIN_MS);
    until_ms.saturating_sub(busy)
}

/// First trained non-eco unit time = first military. source: eco unit set.
pub fn first_military_ms(evs: &[Ev], player: i32) -> Option<u32> {
    evs.iter().filter(|e| e.player == player)
        .filter_map(|e| match e.kind { EvKind::Train(u) if !ECO_UNIT_IDS.contains(&u) => Some(e.t_ms), _ => None })
        .min()
}

/// Watched eco upgrades and their first-research time. source: canonical AoE2 tech ids
/// (verify against data/costs.json). Loom 22, Wheelbarrow 213, Hand Cart 249,
/// Double-Bit Axe 202, Bow Saw 203, Horse Collar 14, Heavy Plow 13, Gold Mining 55,
/// Gold Shaft 182, Stone Mining 278.
pub const WATCHED_TECHS: &[(u16, &str)] = &[
    (22, "Loom"), (213, "Wheelbarrow"), (249, "Hand Cart"), (202, "Double-Bit Axe"),
    (203, "Bow Saw"), (14, "Horse Collar"), (13, "Heavy Plow"),
    (55, "Gold Mining"), (182, "Gold Shaft Mining"), (278, "Stone Mining"),
];
pub fn eco_tech_times(evs: &[Ev], player: i32) -> Vec<(u16, u32)> {
    WATCHED_TECHS.iter().filter_map(|&(id, _)| first_research(evs, player, id).map(|t| (id, t))).collect()
}

fn median(mut v: Vec<f32>) -> Option<f32> {
    if v.is_empty() { return None; }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Some(v[v.len() / 2])
}

/// Base = median (x,y) of the player's Build coords within `window_ms`. source: spec (build-centroid).
pub fn base_centroid(evs: &[Ev], player: i32, window_ms: u32) -> Option<(f32, f32)> {
    let xs: Vec<f32> = evs.iter().filter(|e| e.player == player && e.t_ms < window_ms)
        .filter_map(|e| match e.kind { EvKind::Build { x, .. } => Some(x), _ => None }).collect();
    let ys: Vec<f32> = evs.iter().filter(|e| e.player == player && e.t_ms < window_ms)
        .filter_map(|e| match e.kind { EvKind::Build { y, .. } => Some(y), _ => None }).collect();
    match (median(xs), median(ys)) { (Some(x), Some(y)) => Some((x, y)), _ => None }
}

/// Actions per minute (all events for the player). Effort proxy.
pub fn apm(evs: &[Ev], player: i32, duration_ms: u32) -> f64 {
    let n = evs.iter().filter(|e| e.player == player).count() as f64;
    let mins = (duration_ms as f64 / 60_000.0).max(1.0 / 60.0);
    n / mins
}
```

- [ ] **Step 4:** Run tests — Expected: PASS.
- [ ] **Step 5:** Commit: `git commit -am "feat(analyze): idle-TC, military, eco-tech, centroid, APM metrics (TDD)"`

---

## Task 5: Walk a `Savegame` into events + meta + ELO

**Files:** Create `src/analyze/walk.rs`; extend `model.rs` (`GameMeta`, `PlayerInfo`, `EloTable`).

**Interfaces:**
- Produces: `walk::walk(game: &Savegame) -> Walked` where `Walked { meta: GameMeta, players: Vec<PlayerInfo>, evs: Vec<Ev>, elo: EloTable }`.
- `GameMeta { map_id: u32, map_size: u32, team_positions: bool, duration_ms: u32, rec_player: i32 }`.
- `PlayerInfo { player_number: i32, profile_id: i32, civ_id: u32, name: String, color_id: i32, team_id: u8 }`.
- `EloTable` with `pub fn elo(&self, player_number: i32, ladder: u32) -> Option<i32>`.

- [ ] **Step 1: Write failing unit tests** (the pure helpers, no full Savegame needed) in `walk.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn build_blob_decodes_x_y_id() {
        // x=69.0 @ [4..8], y=32.0 @ [8..12], id=70 @ [12..16]
        let mut d = vec![0u8; 16];
        d[4..8].copy_from_slice(&69.0f32.to_le_bytes());
        d[8..12].copy_from_slice(&32.0f32.to_le_bytes());
        d[12..16].copy_from_slice(&70i32.to_le_bytes());
        assert_eq!(decode_build(&d), Some((70, 69.0, 32.0)));
        assert_eq!(decode_build(&[0u8; 4]), None);
    }
    #[test]
    fn leaderboard_index_maps_to_game_player_number() {
        // leaderboard pn is 0-indexed; game pn = lb + 1
        let mut t = EloTable::default();
        t.insert(3, 0, 1290); t.insert(3, 1, 1309);
        assert_eq!(t.elo(1, 3), Some(1290));
        assert_eq!(t.elo(2, 3), Some(1309));
        assert_eq!(t.elo(2, 4), None);
    }
}
```

- [ ] **Step 2:** Run `cargo test -p replay-rs walk::tests` — Expected: FAIL.
- [ ] **Step 3: Implement** `walk.rs` (`decode_build`, `EloTable`, then the full `walk`):

```rust
//! Bridge: aoe2rec::Savegame -> normalized events + meta + ELO. Parser-coupled.
use std::collections::HashMap;
use aoe2rec::actions::ActionData;
use aoe2rec::{Operation, PostGameBlock, Savegame};
use crate::analyze::model::{Ev, EvKind, GameMeta, PlayerInfo};

/// Build action body: x:f32@[4..8], y:f32@[8..12], building_id:i32@[12..16]. source: validated probe.
pub fn decode_build(d: &[u8]) -> Option<(i64, f32, f32)> {
    if d.len() < 16 { return None; }
    let x = f32::from_le_bytes(d[4..8].try_into().ok()?);
    let y = f32::from_le_bytes(d[8..12].try_into().ok()?);
    let id = i32::from_le_bytes(d[12..16].try_into().ok()?) as i64;
    Some((id, x, y))
}

#[derive(Default)]
pub struct EloTable { by: HashMap<(u32, i32), i32> } // (ladder_id, game_player_number) -> elo
impl EloTable {
    pub fn insert(&mut self, ladder: u32, lb_player_number: i32, elo: i32) {
        self.by.insert((ladder, lb_player_number + 1), elo); // 0-indexed lb -> 1-indexed game pn
    }
    pub fn elo(&self, player_number: i32, ladder: u32) -> Option<i32> { self.by.get(&(ladder, player_number)).copied() }
}

pub struct Walked { pub meta: GameMeta, pub players: Vec<PlayerInfo>, pub evs: Vec<Ev>, pub elo: EloTable }

pub fn walk(game: &Savegame) -> Walked {
    let gs = &game.zheader.game_settings;
    let players: Vec<PlayerInfo> = gs.players.iter().filter(|p| p.civ_id != 0).map(|p| PlayerInfo {
        player_number: p.player_number, profile_id: p.profile_id, civ_id: p.civ_id,
        name: String::from(&p.name), color_id: p.color_id, team_id: p.resolved_team_id,
    }).collect();

    let mut evs = Vec::new();
    let mut elo = EloTable::default();
    let mut last_t = 0u32;
    for op in &game.operations {
        match op {
            Operation::Action { action_data, world_time, .. } => {
                last_t = last_t.max(*world_time);
                let kind = match action_data {
                    ActionData::DeQueue { unit_id, amount, .. } => {
                        // expand amount (>=1) to repeated Train events for cumulative counting
                        let n = (*amount).max(1);
                        for _ in 1..n { evs.push(Ev { player: pid(action_data), t_ms: *world_time, kind: EvKind::Train(*unit_id) }); }
                        EvKind::Train(*unit_id)
                    }
                    ActionData::Research { technology_type, .. } => EvKind::Research(*technology_type),
                    ActionData::Build { data, .. } => match decode_build(data) {
                        Some((id, x, y)) => EvKind::Build { id, x, y }, None => EvKind::Other },
                    ActionData::Move { x, y, .. } => EvKind::Move { x: *x, y: *y },
                    ActionData::Interact { x, y, .. } => EvKind::Interact { x: *x, y: *y },
                    _ => EvKind::Other,
                };
                evs.push(Ev { player: pid(action_data), t_ms: *world_time, kind });
            }
            Operation::PostGame { blocks, .. } => for b in blocks {
                if let PostGameBlock::Leaderboards { leaderboards, .. } = b {
                    for lb in leaderboards { for lp in &lb.players { elo.insert(lb.id, lp.player_number, lp.elo); } }
                }
            },
            _ => {}
        }
    }
    let meta = GameMeta {
        map_id: gs.resolved_map_id, map_size: gs.map_size,
        team_positions: gs.team_positions.into(), duration_ms: last_t,
        rec_player: game.zheader.replay.rec_player as i32,
    };
    Walked { meta, players, evs, elo }
}

/// player_id is at a known position on each ActionData variant; reuse the same accessor extract.rs uses.
fn pid(ad: &ActionData) -> i32 {
    // Minimal: only the variants we map carry a player_id we need; default 0 otherwise.
    match ad {
        ActionData::DeQueue { player_id, .. } | ActionData::Research { player_id, .. }
        | ActionData::Build { player_id, .. } | ActionData::Move { player_id, .. }
        | ActionData::Interact { player_id, .. } => *player_id as i32,
        _ => 0,
    }
}
```

Add to `model.rs`:

```rust
#[derive(Debug, Clone)]
pub struct GameMeta { pub map_id: u32, pub map_size: u32, pub team_positions: bool, pub duration_ms: u32, pub rec_player: i32 }
#[derive(Debug, Clone)]
pub struct PlayerInfo { pub player_number: i32, pub profile_id: i32, pub civ_id: u32, pub name: String, pub color_id: i32, pub team_id: u8 }
```

- [ ] **Step 4:** Add `pub mod walk;` to `mod.rs`. Run `cargo test -p replay-rs walk::tests` — Expected: PASS.
- [ ] **Step 5: Add an `#[ignore]` integration smoke test** (runs only on demand against a real save):

```rust
#[test] #[ignore]
fn walks_a_real_replay() {
    let path = std::env::var("TEST_REPLAY").expect("set TEST_REPLAY=/path/to.aoe2record");
    let game = Savegame::from_file(std::path::Path::new(&path)).unwrap();
    let w = walk(&game);
    assert!(!w.players.is_empty());
    assert!(w.meta.duration_ms > 0);
    eprintln!("map_id={} players={} evs={}", w.meta.map_id, w.players.len(), w.evs.len());
}
```

Run: `TEST_REPLAY="$HOME/İndirilenler/AgeIIDE_Replay_475242532.aoe2record" cargo test -p replay-rs walks_a_real_replay -- --ignored --nocapture` — Expected: prints map_id=9 players=2.
- [ ] **Step 6:** Commit: `git commit -am "feat(analyze): walk Savegame -> events+meta+ELO (TDD + ignored smoke)"`

---

## Task 6: Generate & load `costs.json`

**Files:** Create `scripts/data-pipeline/build-costs.mjs`, `data/costs.json` (generated), `src/analyze/data.rs` (Costs half).

**Interfaces:**
- Produces: `data::Costs { unit: HashMap<u16,Cost>, building: HashMap<i64,Cost>, tech: HashMap<u16,Cost> }`, `Cost { food, wood, gold, stone: f64 }`, `data::load_costs() -> Costs` (from `include_str!`).

- [ ] **Step 1:** Write `scripts/data-pipeline/build-costs.mjs`:

```js
// Extract unit/building/tech costs from aoe2techtree data.json -> data/costs.json.
// Run: node scripts/data-pipeline/build-costs.mjs  (reads .cache/aoe2-data/data.json)
import { readFileSync, writeFileSync } from "node:fs";
const src = JSON.parse(readFileSync(".cache/aoe2-data/data.json", "utf8")).data;
const cost = (c = {}) => ({ food: c.Food || 0, wood: c.Wood || 0, gold: c.Gold || 0, stone: c.Stone || 0 });
const pick = (obj) => Object.fromEntries(Object.entries(obj).map(([id, v]) => [id, cost(v.Cost)]));
const out = { _source: "aoe2techtree data/data.json", unit: pick(src.Unit), building: pick(src.Building), tech: pick(src.Tech) };
writeFileSync("scripts/data-pipeline/replay-rs/data/costs.json", JSON.stringify(out, null, 0));
console.log(`costs.json: ${Object.keys(out.unit).length} units, ${Object.keys(out.building).length} buildings, ${Object.keys(out.tech).length} techs`);
```

- [ ] **Step 2:** Run `node scripts/data-pipeline/build-costs.mjs` from repo root — Expected: writes `data/costs.json`; prints counts (~245 units / 40 buildings / 194 techs). Sanity-check: villager (`unit["83"]`) has `food:50`, house (`building["70"]`) has `wood:25`.

> **NOTE (age-tech ids — verified):** aoe2techtree mislabels the age tech *names* (id 101 = "Middle Age", 102 = "Feudal Age", 103 = "Imperial Age"), but the *costs* at 101/102/103 are exactly the replay's feudal/castle/imperial age-up costs (500f / 800f+200g / 1000f+800g) — so `costs.json` keyed by id IS correct for the float estimator. Age *semantics* come only from `config.rs` (101=feudal), never from the data.json names. Do not "fix" this.
- [ ] **Step 3: Write failing test** in `src/analyze/data.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loads_known_costs() {
        let c = load_costs();
        assert_eq!(c.unit[&83].food, 50.0);   // villager
        assert_eq!(c.building[&70].wood, 25.0); // house
    }
}
```

- [ ] **Step 4:** Run `cargo test -p replay-rs data::tests::loads_known_costs` — Expected: FAIL.
- [ ] **Step 5: Implement** the Costs half of `data.rs`:

```rust
//! Load committed benchmark.json + costs.json (baked in via include_str!).
use std::collections::HashMap;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize, Default)]
pub struct Cost { pub food: f64, pub wood: f64, pub gold: f64, pub stone: f64 }

#[derive(Debug, Deserialize)]
pub struct Costs {
    #[serde(deserialize_with = "str_keyed_u16")] pub unit: HashMap<u16, Cost>,
    #[serde(deserialize_with = "str_keyed_i64")] pub building: HashMap<i64, Cost>,
    #[serde(deserialize_with = "str_keyed_u16")] pub tech: HashMap<u16, Cost>,
}

fn str_keyed_u16<'de, D: serde::Deserializer<'de>>(d: D) -> Result<HashMap<u16, Cost>, D::Error> {
    let m: HashMap<String, Cost> = HashMap::deserialize(d)?;
    Ok(m.into_iter().filter_map(|(k, v)| k.parse().ok().map(|k| (k, v))).collect())
}
fn str_keyed_i64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<HashMap<i64, Cost>, D::Error> {
    let m: HashMap<String, Cost> = HashMap::deserialize(d)?;
    Ok(m.into_iter().filter_map(|(k, v)| k.parse().ok().map(|k| (k, v))).collect())
}

pub fn load_costs() -> Costs { serde_json::from_str(include_str!("../../data/costs.json")).expect("costs.json") }
```

- [ ] **Step 6:** Add `pub mod data;` to `mod.rs`. Run test — Expected: PASS.
- [ ] **Step 7:** Commit: `git add -A && git commit -m "feat(analyze): costs.json generator + loader (TDD)"`

---

## Task 7: Generate & load `benchmark.json` + elo_bucket

**Files:** Create `scripts/data-pipeline/build-benchmark.sql`, `data/benchmark.json`, extend `src/analyze/data.rs`.

**Interfaces:**
- Produces: `data::Benchmark` with `pub fn slice(&self, civ: &str, bucket: &str) -> Option<&Slice>`; `Slice { feudal_s, castle_s, imperial_s, vils_castle: Option<f64> }`; `data::elo_bucket(elo: i32) -> &'static str`; `data::load_benchmark() -> Benchmark`.

- [ ] **Step 1:** Write `scripts/data-pipeline/build-benchmark.sql` (run on the VM; emits a JSON the generator nests). Use `match_ages` JOIN `games` for elo_bucket; medians per civ×bucket×age, plus a `2500+` slice already covered by the bucket. The SQL writes a flat parquet/csv; a tiny mjs nests it. For v1, emit civ×bucket medians of age completion seconds + vils@castle:

```sql
-- Run on the VM (DB unlocked): ~/bin/duckdb -readonly aoe2.duckdb -c ".read build-benchmark.sql"
COPY (
  WITH j AS (
    SELECT g.civ, g.elo_bucket AS bucket, ma.age, ma.uptime AS age_s, ma.villagers
    FROM match_ages ma JOIN games g USING (match_id, profile_id)
    WHERE ma.reached AND g.team_size = 1
  )
  SELECT civ, bucket,
    median(CASE WHEN age='feudal'   THEN age_s END) AS feudal_s,
    median(CASE WHEN age='castle'   THEN age_s END) AS castle_s,
    median(CASE WHEN age='imperial' THEN age_s END) AS imperial_s,
    median(CASE WHEN age='castle'   THEN villagers END) AS vils_castle
  FROM j GROUP BY 1,2
) TO 'benchmark-flat.csv' (HEADER, DELIMITER ',');
```

- [ ] **Step 2:** Write `scripts/data-pipeline/build-benchmark.mjs` to nest `benchmark-flat.csv` → `data/benchmark.json` shaped `{ "<civ>": { "<bucket>": { feudal_s, castle_s, imperial_s, vils_castle } } }`. (Plain CSV parse + group; `_basis: "match_ages completion seconds, team_size=1"`.)
- [ ] **Step 3:** Run both on the VM, scp `benchmark.json` into `data/`. (If the corpus is locked, fall back to a tiny hand-written fixture for tests and note in commit that the real file is regenerated later.) Sanity: `benchmark["franks"]["1400-1649"].feudal_s` is a plausible ~700–800.
- [ ] **Step 4: Write failing tests**:

```rust
#[test]
fn elo_bucket_boundaries() {
    assert_eq!(elo_bucket(999), "<1000");
    assert_eq!(elo_bucket(1300), "1200-1399");
    assert_eq!(elo_bucket(2600), "2500+");
}
#[test]
fn benchmark_slice_lookup() {
    let b = load_benchmark();
    assert!(b.slice("franks", "1400-1649").is_some());
}
```

- [ ] **Step 5: Implement** (append to `data.rs`):

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct Slice { pub feudal_s: Option<f64>, pub castle_s: Option<f64>, pub imperial_s: Option<f64>, pub vils_castle: Option<f64> }
#[derive(Debug, Deserialize)]
pub struct Benchmark(pub HashMap<String, HashMap<String, Slice>>);
impl Benchmark { pub fn slice(&self, civ: &str, bucket: &str) -> Option<&Slice> { self.0.get(civ)?.get(bucket) } }
pub fn load_benchmark() -> Benchmark { serde_json::from_str(include_str!("../../data/benchmark.json")).expect("benchmark.json") }

/// source: existing games.elo_bucket boundaries.
pub fn elo_bucket(elo: i32) -> &'static str {
    match elo {
        i32::MIN..=999 => "<1000", 1000..=1199 => "1000-1199", 1200..=1399 => "1200-1399",
        1400..=1649 => "1400-1649", 1650..=1799 => "1650-1799", 1800..=1999 => "1800-1999",
        2000..=2199 => "2000-2199", 2200..=2499 => "2200-2499", _ => "2500+",
    }
}
```

- [ ] **Step 6:** Run tests — Expected: PASS. Commit: `git add -A && git commit -m "feat(analyze): benchmark.json generator + loader + elo_bucket (TDD)"`

---

## Task 8: Position — role tag (color/slot) + coordinate metrics

**Files:** Create `src/analyze/position.rs`; extend `model.rs` (`Role`).

**Interfaces:**
- Produces: `model::Role { Flank, Pocket }`; `position::assign_roles(players: &[PlayerInfo], family: Family) -> HashMap<i32, Role>` (empty unless ≥2 teams AND family Open|Closed); `position::coord_metrics(evs, players, bases) -> HashMap<i32, CoordMetric>`; `CoordMetric { nearest_enemy_dist: Option<f32>, find_enemy_ms: Option<u32> }`.

- [ ] **Step 1: Write failing tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::model::{Family, PlayerInfo, Role};
    fn p(pn: i32, team: u8) -> PlayerInfo { PlayerInfo { player_number: pn, profile_id: 0, civ_id: 1, name: String::new(), color_id: pn, team_id: team } }
    #[test]
    fn roles_4v4_min_max_slot_are_flanks() {
        let ps = vec![p(1,2),p(3,2),p(5,2),p(7,2), p(2,3),p(4,3),p(6,3),p(8,3)];
        let r = assign_roles(&ps, Family::Open);
        assert_eq!(r[&1], Role::Flank); assert_eq!(r[&7], Role::Flank);
        assert_eq!(r[&3], Role::Pocket); assert_eq!(r[&5], Role::Pocket);
    }
    #[test]
    fn roles_skip_when_family_other_or_one_team() {
        let ps = vec![p(1,2),p(3,2),p(2,3),p(4,3)];
        assert!(assign_roles(&ps, Family::Water).is_empty());     // out-of-scope map
        let one = vec![p(1,1),p(2,1),p(3,1),p(4,1)];              // all team 1 = FFA
        assert!(assign_roles(&one, Family::Open).is_empty());
    }
    #[test]
    fn roles_2v2_lower_slot_flank_higher_pocket() {
        let ps = vec![p(1,2),p(3,2),p(2,3),p(4,3)];
        let r = assign_roles(&ps, Family::Open);
        assert_eq!(r[&1], Role::Flank); assert_eq!(r[&3], Role::Pocket);
    }
}
```

- [ ] **Step 2:** Run `cargo test -p replay-rs position::tests` — Expected: FAIL.
- [ ] **Step 3: Implement** `position.rs`:

```rust
//! Light flank/pocket tag (color/slot rule) + coordinate metrics. source: spec decisions.
use std::collections::{HashMap, HashSet};
use crate::analyze::model::{Ev, EvKind, Family, PlayerInfo, Role};

/// Within each team, min & max slot = Flank, inner = Pocket (documented ranked convention).
/// 2v2 special-case: lower slot = Flank, higher = Pocket. Gated to ≥2 teams and Open|Closed maps.
pub fn assign_roles(players: &[PlayerInfo], family: Family) -> HashMap<i32, Role> {
    let mut out = HashMap::new();
    if !matches!(family, Family::Open | Family::Closed) { return out; }
    let mut teams: HashMap<u8, Vec<i32>> = HashMap::new();
    for p in players { if p.team_id != 1 { teams.entry(p.team_id).or_default().push(p.player_number); } }
    if teams.len() < 2 { return out; } // FFA / unresolved
    for (_t, mut slots) in teams {
        slots.sort();
        let n = slots.len();
        for (i, pn) in slots.iter().enumerate() {
            let role = match n {
                1 => continue,
                2 => if i == 0 { Role::Flank } else { Role::Pocket },
                _ => if i == 0 || i == n - 1 { Role::Flank } else { Role::Pocket },
            };
            out.insert(*pn, role);
        }
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct CoordMetric { pub nearest_enemy_dist: Option<f32>, pub find_enemy_ms: Option<u32> }

const FIND_RADIUS: f32 = 15.0; // tiles; "scout reached the enemy base region". source: spec estimate.

pub fn coord_metrics(evs: &[Ev], players: &[PlayerInfo], bases: &HashMap<i32, (f32, f32)>) -> HashMap<i32, CoordMetric> {
    let team: HashMap<i32, u8> = players.iter().map(|p| (p.player_number, p.team_id)).collect();
    let dist = |a: (f32, f32), b: (f32, f32)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    let mut out = HashMap::new();
    for p in players {
        let Some(&base) = bases.get(&p.player_number) else { out.insert(p.player_number, CoordMetric::default()); continue };
        let enemies: Vec<(f32, f32)> = bases.iter()
            .filter(|(pn, _)| team.get(pn) != Some(&p.team_id)).map(|(_, &b)| b).collect();
        let nearest = enemies.iter().map(|&e| dist(base, e)).min_by(|a, b| a.partial_cmp(b).unwrap());
        // first time any of this player's unit coords come within FIND_RADIUS of an enemy base
        let find = evs.iter().filter(|e| e.player == p.player_number).filter_map(|e| {
            let c = match e.kind { EvKind::Move { x, y } | EvKind::Interact { x, y } => Some((x, y)), _ => None }?;
            if enemies.iter().any(|&en| dist(c, en) < FIND_RADIUS) { Some(e.t_ms) } else { None }
        }).min();
        out.insert(p.player_number, CoordMetric { nearest_enemy_dist: nearest, find_enemy_ms: find });
    }
    out
}
```

Add to `model.rs`: `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Role { Flank, Pocket }`.

- [ ] **Step 4:** Add `pub mod position;` to `mod.rs`. Run tests — Expected: PASS.
- [ ] **Step 5:** Commit: `git commit -am "feat(analyze): flank/pocket slot rule + coord metrics (TDD)"`

---

## Task 9: Float estimator

**Files:** Create `src/analyze/float.rs`.

**Interfaces:**
- Produces: `float::estimate(evs, player, costs, duration_ms) -> Vec<FloatWindow>`; `FloatWindow { start_ms, end_ms: u32, rate_gap_per_min: f64, approx_banked: f64 }`; `float::CAVEAT: &str`.

- [ ] **Step 1: Write failing test**:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::model::{Ev, EvKind};
    use crate::analyze::data::{Cost, Costs};
    use std::collections::HashMap;
    fn costs() -> Costs { Costs { unit: HashMap::new(), building: HashMap::new(), tech: HashMap::new() } }
    #[test]
    fn pure_income_no_spend_is_one_big_float() {
        // 10 villagers from t=0, no spend, over 600s => income ~ 10*0.30*600 = 1800 res floated.
        let evs: Vec<Ev> = (0..10).map(|k| Ev { player: 1, t_ms: k, kind: EvKind::Train(83) }).collect();
        let w = estimate(&evs, 1, &costs(), 600_000);
        assert!(!w.is_empty());
        assert!(w.iter().map(|x| x.approx_banked).sum::<f64>() > 1000.0);
    }
}
```

- [ ] **Step 2:** Run `cargo test -p replay-rs float::tests` — Expected: FAIL.
- [ ] **Step 3: Implement** `float.rs` — sample the timeline at fixed steps, integrate income (`vils_at × 0.30 res/s`) minus cumulative spend (sum of costs of Train/Build/Research up to t via `costs.json`), and emit windows where the running surplus grows:

```rust
//! Best-effort floating-resources estimate. source: spec (no resource data in the stream).
use crate::analyze::data::Costs;
use crate::analyze::metrics::vils_at;
use crate::analyze::model::{Ev, EvKind};

pub const CAVEAT: &str = "Float is ESTIMATED (income≈villagers×0.30 res/s minus known spend); it ignores \
villager assignment, walk distance, civ bonuses, market trades, upgrades, and deaths — it flags big floats, not exact banks.";

const GATHER_PER_S: f64 = 0.30;        // source: spec effective blended rate
const STEP_MS: u32 = 30_000;           // 30s sampling
const MIN_GAP_PER_MIN: f64 = 150.0;    // only report windows where surplus grows faster than this

#[derive(Debug, Clone)]
pub struct FloatWindow { pub start_ms: u32, pub end_ms: u32, pub rate_gap_per_min: f64, pub approx_banked: f64 }

fn spend_to(evs: &[Ev], player: i32, t: u32, c: &Costs) -> f64 {
    evs.iter().filter(|e| e.player == player && e.t_ms <= t).map(|e| {
        let cost = match e.kind {
            EvKind::Train(u) => c.unit.get(&u).copied(),
            EvKind::Build { id, .. } => c.building.get(&id).copied(),
            EvKind::Research(t) => c.tech.get(&t).copied(),
            _ => None,
        };
        cost.map(|x| x.food + x.wood + x.gold + x.stone).unwrap_or(0.0)
    }).sum()
}

pub fn estimate(evs: &[Ev], player: i32, c: &Costs, duration_ms: u32) -> Vec<FloatWindow> {
    let mut out = Vec::new();
    let income_to = |t: u32| {
        // integrate villagers(t)*rate by stepping; cheap and good enough
        let mut acc = 0.0; let mut s = 0u32;
        while s < t { acc += vils_at(evs, player, s) as f64 * GATHER_PER_S * (STEP_MS as f64 / 1000.0); s += STEP_MS; }
        acc
    };
    let surplus_at = |t: u32| (income_to(t) - spend_to(evs, player, t, c)).max(0.0);
    // track the surplus AT WINDOW START so rate_gap_per_min is a true per-minute rate, not an absolute bank.
    let mut win: Option<(u32, f64)> = None; // (start_ms, surplus_at_start)
    let mut prev_surplus = 0.0;
    let mut t = STEP_MS;
    let close = |start: u32, start_surplus: f64, end: u32, end_surplus: f64, out: &mut Vec<FloatWindow>| {
        let mins = ((end - start) as f64 / 60_000.0).max(STEP_MS as f64 / 60_000.0);
        let banked = (end_surplus - start_surplus).max(0.0);
        out.push(FloatWindow { start_ms: start, end_ms: end, rate_gap_per_min: banked / mins, approx_banked: banked });
    };
    while t <= duration_ms {
        let surplus = surplus_at(t);
        let gap_per_min = (surplus - prev_surplus) / (STEP_MS as f64 / 60_000.0);
        if gap_per_min >= MIN_GAP_PER_MIN { win.get_or_insert((t - STEP_MS, prev_surplus)); }
        else if let Some((start, start_surplus)) = win.take() { close(start, start_surplus, t, surplus, &mut out); }
        prev_surplus = surplus; t += STEP_MS;
    }
    if let Some((start, start_surplus)) = win { close(start, start_surplus, duration_ms, prev_surplus, &mut out); }
    out
}
```

- [ ] **Step 4:** Add `pub mod float;` to `mod.rs`. Run test — Expected: PASS.
- [ ] **Step 5:** Commit: `git commit -am "feat(analyze): float estimator (TDD)"`

---

## Task 10: Assemble per-player metrics + 4-way compare → Findings

**Files:** Create `src/analyze/compare.rs`; extend `model.rs` (`PlayerMetrics`, `Basis`, `Severity`, `Finding`).

**Interfaces:**
- Produces: `model::PlayerMetrics { info, feudal_ms, castle_ms, imperial_ms, vils_castle, idle_dark_ms, idle_feudal_ms, first_military_ms, eco_techs: Vec<(u16,u32)>, apm, base, elo_1v1, elo_team, role: Option<Role> }`.
- Produces: `compare::build_metrics(walked, costs) -> Vec<PlayerMetrics>` and `compare::findings(metrics, bench, family) -> Vec<Finding>`.
- `Finding { player_number, metric: String, your: String, reference: String, basis: Basis, severity: Severity, note: String }`; `enum Basis { YourElo, Pro, Opponent, Absolute }`; `enum Severity { Low, Med, High }`.

- [ ] **Step 1:** Add the structs/enums to `model.rs` (see Interfaces).
- [ ] **Step 2: Write failing tests** in `compare.rs` for representative findings (idle TC, slow age vs benchmark):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::model::*;
    fn pm(pn: i32) -> PlayerMetrics { /* construct with feudal_ms=Some(720_000), idle_dark_ms=80_000, ... */ unimplemented!() }
    #[test]
    fn flags_high_dark_idle_tc() {
        let m = vec![/* a player with idle_dark_ms = 80_000 */];
        let f = findings(&m, &empty_bench(), Family::Open);
        assert!(f.iter().any(|x| x.metric == "idle TC (dark)" && matches!(x.severity, Severity::High)));
    }
}
```

(Fill `pm`/`empty_bench` with concrete literals when implementing — no `unimplemented!` in the final code.)

- [ ] **Step 3:** Run — Expected: FAIL.
- [ ] **Step 4: Implement** `build_metrics` (calls Task 3–9 functions; idle_dark = `idle_tc_ms(evs, p, feudal_ms)`, idle_feudal = `idle_tc_ms(evs, p, castle_ms) - idle_tc_ms(evs, p, feudal_ms)`, vils_castle = `vils_at(evs, p, castle_ms)`, etc.) and `findings` (the 4 comparisons + absolute thresholds from the spec's mistake table: dark idle >15s flag/ >40s high; Feudal click >benchmark+60s; missing Loom by Feudal on aggro; no military by +90s on aggro builds; float windows). Each `Finding` gets a one-line `note`.
- [ ] **Step 5:** Run — Expected: PASS. Commit: `git commit -am "feat(analyze): per-player metrics + 4-way findings (TDD)"`

---

## Task 11: Report renderer

**Files:** Create `src/analyze/report.rs`.

**Interfaces:**
- Produces: `report::render(meta, map_name, family, metrics, findings, you) -> String`.

- [ ] **Step 1: Write failing test** asserting key content (not exact layout):

```rust
#[test]
fn report_contains_player_and_top_fix() {
    let s = render(/* meta */, "Arabia", Family::Open, &metrics, &findings, 1);
    assert!(s.contains("Arabia"));
    assert!(s.contains("Top Fixes") || s.contains("TOP FIXES"));
    assert!(s.contains("MKD"));   // the "you" player name
}
```

- [ ] **Step 2:** Run — Expected: FAIL.
- [ ] **Step 3: Implement** with `comfy-table` (one scorecard row per player: color name, role, Feudal/Castle/Imp, vils@Castle, idle, APM with ✓/⚠/✗ vs reference) + an `owo-colors`-styled "Top 3 Fixes" block sorted by severity; highlight the `you` row; gate ANSI on `std::io::IsTerminal`. Print `float::CAVEAT` and a "macro coach — no fights/micro/exact resources" footer.
- [ ] **Step 4:** Run — Expected: PASS. Commit: `git commit -am "feat(analyze): terminal report renderer (TDD)"`

---

## Task 12: Wire `run()` end-to-end (+ `--match-id`, `--you`)

**Files:** Modify `src/analyze/mod.rs`.

- [ ] **Step 1:** Implement `run()`: resolve `Input` (File → `Savegame::from_file`; MatchId → reuse `api::build_client` + `get_replay_files` + `best_file` + `download_replay` → `Savegame::from_bytes`), then `walk` → `maps::load().lookup(meta.map_id)` → `assign_roles` → `coord_metrics` → `build_metrics` → `findings` → `report::render`, printing the report. Resolve "you": `--you NAME` matches a player name (case-insensitive), else `meta.rec_player`.
- [ ] **Step 2:** Build: `cargo build --release` — Expected: compiles.
- [ ] **Step 3: Manual smoke** on local saves (1v1 Arabia, a 4v4, an African Clearing) and the downloaded replay:

```bash
B=./target/release/replay-rs
$B analyze "$HOME/İndirilenler/AgeIIDE_Replay_475242532.aoe2record"
$B analyze "$HOME/.local/share/Steam/steamapps/compatdata/813780/pfx/drive_c/users/steamuser/Games/Age of Empires 2 DE/76561198093330332/savegame/MP Replay v101.103.47452.0 @2026.06.10 190657 (3).aoe2record"
```

Expected: a scorecard per player; Arabia 1v1 shows ELO 1290/1309 and no role tag (1v1); the 4v4 shows roles only if its map is open/closed (African Clearing → none, by design). Eyeball that age-up times and vil counts are sane (cross-check vs the numbers from the design-session probe).
- [ ] **Step 4:** `--match-id` smoke (network): `$B analyze --match-id 475242532` — Expected: downloads + same report (or a clear "expired/not uploaded" error).
- [ ] **Step 5:** Commit: `git commit -am "feat(analyze): end-to-end run (file + --match-id + --you)"`

---

## Task 13: Final verification + README

**Files:** Modify `scripts/data-pipeline/replay-rs/README.md` (document `analyze`, the data files + regen commands, maps.tsv editing).

- [ ] **Step 1:** `cargo test -p replay-rs` — Expected: all pass. `cargo build --release` clean.
- [ ] **Step 2:** Cross-check ≥2 numbers (a player's Feudal/Castle click ms, vils@Castle) against the existing parser/extract output on the same replay; note any basis differences (click vs completion).
- [ ] **Step 3:** Add a README "analyze" section: usage, that role tags only appear on open/closed maps (edit `data/maps.tsv`), how to regenerate `costs.json`/`benchmark.json`, and the honest non-goals/caveats.
- [ ] **Step 4:** Commit: `git commit -am "docs(analyze): README usage + data regen + caveats"`

---

## Self-Review notes (gaps to watch during execution)

- **benchmark.json basis** (Task 7) compares the analyzer's age-up *click* time against `match_ages` *completion* time. Either add the research-duration offset (Feudal+130/Castle+160/Imperial+190 s) in `findings`, or rebuild from `replay_ages` when loaded. Decide in Task 10; document in Task 13.
- **WATCHED_TECHS ids** (Task 4) are canonical but unverified against this DE build — if a tech finding never fires, confirm the id in `data/costs.json`/`data.json`. Low risk (a wrong id just suppresses one finding).
- **Multi-TC idle** (Task 4) — the `window − vils×25s` model overcounts idle once a 2nd TC exists; keep idle-TC findings to dark/early-feudal (1 TC) in Task 10.
- **Role tag** depends on `data/maps.tsv` families the user may still be editing — non-blocking; the code reads whatever is committed.
- **Coordinate metrics** (proximity, find-enemy) are computed for all maps but only *surfaced* meaningfully on open maps — Task 11 decides what to show where.
