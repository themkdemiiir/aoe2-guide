# Analyzer Phase C Implementation Plan — lib/bin split, Report (schema v1), --json, recent/--latest

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the analyzer's output a serializable `Report` produced by a pure library function, expose it via `--json`, and add `recent` / `analyze --latest` so the user can analyze their latest ranked games without hand-copying match ids.

**Architecture:** The existing bin-only crate gains a `[lib]` target holding `analyze` + `config` (pure: replay in → `Report` out, zero IO). Everything IO-ish stays bin-side: `api.rs` (network, grows `get_recent_matches`), `report.rs` (terminal render, moves out of the lib), `main.rs` (CLI). Spec: `docs/superpowers/specs/2026-07-02-analyzer-report-schema-design.md`.

**Tech Stack:** Rust 2021, existing deps only (`serde`/`serde_json` already present — NO new crates).

## Global Constraints

- Crate root: `scripts/data-pipeline/replay-rs/`. All `cargo` commands run from there.
- NO new dependencies. No clap — keep the hand-rolled `while` arg loop in `main.rs`.
- Every AoE2/API constant carries a `// source:` comment. No fabricated values.
- No default values that hide missing data: absent `--profile-id` (and env) is a loud error.
- NO silent fallbacks anywhere: a `--you NAME` / `--profile-id` that matches no player in the
  replay is an ERROR, never a quiet fallback to the recorder. NO regex anywhere.
- Enums serialize `snake_case` (`Basis::YourElo` → `"your_elo"`). `schema_version` = 1.
- `--json` output replaces the pretty report entirely (stdout = pure JSON). Multi-game `--latest` output is NDJSON (one compact JSON per line); single file/`--match-id` stays one pretty document.
- Keep it light: this is a hobby project. No feature flags, no workspace split (that's the WASM phase).

## Spec amendments (folded in, override the spec where they conflict)

- **`analyze` takes `&YouSel`, not `Option<&str>`:** `--latest` must resolve "you" by profile id (the recording player of a *downloaded* replay is whoever uploaded it, not necessarily you). `pub enum YouSel { Auto, Name(String), ProfileId(i64) }`.
- **`PlayerInfo` gains `pub profile_id: i64`:** required for `YouSel::ProfileId`, useful to any JSON consumer. The field exists on the parsed replay (the bulk pipeline's `players` table already extracts it).

## File Structure

```
scripts/data-pipeline/replay-rs/
  Cargo.toml                 MODIFY (Task 5): add [lib]
  src/lib.rs                 NEW (Task 5): pub mod analyze; pub mod config;
  src/main.rs                MODIFY (Tasks 4,5,7,8): --json/--latest/--profile-id, recent, load_game moves here
  src/report.rs              NEW LOCATION (Task 5): git mv from src/analyze/report.rs (bin-side render)
  src/api.rs                 MODIFY (Task 6): + get_recent_matches (stays bin-side)
  src/analyze/model.rs       MODIFY (Task 1): Serialize derives, Report/ReportMeta/YouSel, profile_id
  src/analyze/mod.rs         MODIFY (Task 2): pure analyze(); (Task 5): run/load_game removed
  src/analyze/compare.rs     MODIFY (Task 2): findings() takes mode: &str
  src/analyze/walk.rs        MODIFY (Task 1): populate profile_id
```

---

### Task 1: Serializable model — `Report`, `ReportMeta`, `YouSel`, snake_case enums, `profile_id`

**Files:**
- Modify: `src/analyze/model.rs`, `src/analyze/walk.rs` (PlayerInfo construction), `src/analyze/compare.rs` (test builder), `src/analyze/report.rs` (test builder)

**Interfaces:**
- Produces: `model::{Report, ReportMeta, YouSel, SCHEMA_VERSION, MACRO_CAVEAT}`; `Serialize` on `Family, Role, Basis, Severity, PlayerInfo, PlayerMetrics, Finding, Report, ReportMeta`; `PlayerInfo.profile_id: i64`.

- [ ] **Step 1: Write the failing test** — append to `src/analyze/model.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_serializes_versioned_snake_case() {
        let r = Report {
            schema_version: SCHEMA_VERSION,
            meta: ReportMeta {
                map_id: 9, map_name: "Arabia".into(), family: Family::Open,
                mode: "1v1".into(), duration_ms: 2_400_000, you: 1,
            },
            players: vec![],
            findings: vec![Finding {
                player_number: 1, metric: "Feudal up-time".into(), your: "12:10".into(),
                reference: "11:20".into(), basis: Basis::YourElo, severity: Severity::High,
                note: "n".into(),
            }],
            caveats: vec![MACRO_CAVEAT.to_string()],
        };
        let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(v["schema_version"], 1);
        assert_eq!(v["meta"]["family"], "open");
        assert_eq!(v["meta"]["mode"], "1v1");
        assert_eq!(v["findings"][0]["basis"], "your_elo");
        assert_eq!(v["findings"][0]["severity"], "high");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p replay-rs model::tests -- --nocapture`
Expected: FAIL to compile — `Report`/`ReportMeta`/`SCHEMA_VERSION`/`MACRO_CAVEAT` not found.

- [ ] **Step 3: Implement** in `src/analyze/model.rs`:

Add at the top: `use serde::Serialize;`

Add derives (keep every existing derive, append `Serialize` + the rename attribute):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Family { Open, Closed, Hybrid, Water, Nomad, Special, Other }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role { Flank, Pocket }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis { YourElo, Pro, Opponent, Absolute }

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity { Low, Med, High }
```

Add `Serialize` to `PlayerInfo`, `PlayerMetrics`, `Finding` (plain `#[derive(Debug, Clone, Serialize)]`, fields already JSON-safe). Add to `PlayerInfo` (after `player_number`):

```rust
    /// Relic profile id — stable player identity across matches. source: replay GameSettings
    /// players (same field the bulk pipeline's `players` table extracts).
    pub profile_id: i64,
```

Add the new types + consts:

```rust
// --- serialized report (the one-way-door JSON contract; see the Phase C spec) --
/// Bump on any breaking shape change so a consumer can detect instead of misparse.
pub const SCHEMA_VERSION: u32 = 1;

/// Honesty footer, shared verbatim by terminal + JSON. source: analyzer design spec.
pub const MACRO_CAVEAT: &str =
    "macro coach only — no fights, micro, map control, or exact resources.";

/// How to pick the "you" player. Name = --you (case-insensitive); ProfileId = --latest.
#[derive(Debug, Clone)]
pub enum YouSel { Auto, Name(String), ProfileId(i64) }

#[derive(Debug, Clone, Serialize)]
pub struct ReportMeta {
    pub map_id: u32,
    pub map_name: String,
    pub family: Family,
    pub mode: String, // "1v1" | "team"
    pub duration_ms: u32,
    pub you: i32, // player_number
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub meta: ReportMeta,
    pub players: Vec<PlayerMetrics>,
    pub findings: Vec<Finding>, // FULL list, severity-sorted; terminal's take(8) is a render choice
    pub caveats: Vec<String>,
}
```

- [ ] **Step 4: Fix the three `PlayerInfo` construction sites** (compile errors point at them):
  - `src/analyze/walk.rs` — in `walk()`'s players map, add `profile_id: p.profile_id as i64,` (the vendored field may be `i32`/`u32`; the cast compiles either way — check the compiler error if not and match the real type).
  - `src/analyze/compare.rs` tests — `pm()` builder: add `profile_id: 0,`.
  - `src/analyze/report.rs` tests — `pm()` builder: add `profile_id: 0,`.

- [ ] **Step 5: Run the full suite**

Run: `cargo test -p replay-rs`
Expected: ALL PASS (new model test + all existing tests).

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(analyze): serializable Report model (schema v1, snake_case) + PlayerInfo.profile_id"
```

---

### Task 2: Pure `analyze()` + mode promoted through `findings()`

**Files:**
- Modify: `src/analyze/mod.rs`, `src/analyze/compare.rs`

**Interfaces:**
- Consumes: Task 1's `Report`, `ReportMeta`, `YouSel`.
- Produces: `analyze::analyze(game: &Savegame, you: &YouSel) -> anyhow::Result<Report>` (pure — no IO; `Err` only when an explicit `--you`/`--profile-id` selector matches no player); `compare::findings(metrics, bench, civs, family, map_slug, mode: &str) -> Vec<Finding>`; private `resolve_you(sel: &YouSel, players: &[PlayerInfo], rec: i32) -> anyhow::Result<i32>`.

- [ ] **Step 1: Write the failing tests.** In `src/analyze/mod.rs` add:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::model::{PlayerInfo, YouSel};

    fn p(pn: i32, profile_id: i64, name: &str) -> PlayerInfo {
        PlayerInfo { player_number: pn, profile_id, civ_id: 2, name: name.into(),
                     color_id: pn, team_id: 2, won: None }
    }

    #[test]
    fn resolve_you_by_profile_name_auto_and_loud_error() {
        let players = vec![p(1, 111, "Alice"), p(2, 222, "Bob")];
        assert_eq!(resolve_you(&YouSel::ProfileId(222), &players, 1).unwrap(), 2);
        assert_eq!(resolve_you(&YouSel::Name("bob".into()), &players, 1).unwrap(), 2); // case-insensitive
        assert_eq!(resolve_you(&YouSel::Auto, &players, 1).unwrap(), 1);
        // no-fallback rule: an unmatched explicit selector is an ERROR, never the recorder
        assert!(resolve_you(&YouSel::ProfileId(999), &players, 1).is_err());
        assert!(resolve_you(&YouSel::Name("nobody".into()), &players, 1).is_err());
    }
}
```

In `src/analyze/compare.rs`, update the two existing `findings(...)` test calls to pass the new final argument `"1v1"` (they construct 1–2 players).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p replay-rs`
Expected: FAIL to compile — `resolve_you` signature mismatch, `findings` arity.

- [ ] **Step 3: Implement.** In `src/analyze/compare.rs`:

```rust
/// mode is decided once by the caller (analyze()) and recorded in ReportMeta so the
/// findings and the report can never disagree. "team" | "1v1".
pub fn findings(
    metrics: &[PlayerMetrics],
    bench: &Benchmark,
    civs: &HashMap<u32, String>,
    family: Family,
    map_slug: &str,
    mode: &str,
) -> Vec<Finding> {
    let team = mode == "team";
    // ... body unchanged: delete the old `let team = is_team_game(metrics);`
    //     and `let mode = if team { "team" } else { "1v1" };` lines.
```

In `src/analyze/mod.rs`, replace the body of `run()`'s analysis section with a pure function (keep `run`/`load_game` working — they move to the bin in Task 5):

```rust
pub use model::{Report, YouSel};

/// Pure analysis: parsed replay in -> Report out. No file/network IO (committed
/// data/* are include_str!-baked). This is the future WASM boundary. Errs ONLY
/// when an explicit you-selector matches no player (no-fallback rule).
pub fn analyze(game: &Savegame, you: &YouSel) -> anyhow::Result<Report> {
    let w = walk::walk(game);
    let map_table = maps::load();
    let (map_name, family) = map_table.lookup(w.meta.map_id);
    let map_name = map_name.to_string();
    let map_slug = maps::slug(&map_name);

    let roles = position::assign_roles(&w.players, family);
    let bases: HashMap<i32, (f32, f32)> = w.players.iter()
        .filter_map(|p| metrics::base_centroid(&w.evs, p.player_number, 360_000)
            .map(|b| (p.player_number, b)))
        .collect();
    let coords = position::coord_metrics(&w.evs, &w.players, &bases);

    let costs = data::load_costs();
    let bench = data::load_benchmark();
    let civs = data::load_civs();

    let players = compare::build_metrics(&w, &costs, &roles, &coords);
    let mode = if compare::is_team_game(&players) { "team" } else { "1v1" };
    let findings = compare::findings(&players, &bench, &civs, family, &map_slug, mode);
    let you = resolve_you(you, &w.players, w.meta.rec_player)?;

    Ok(Report {
        schema_version: model::SCHEMA_VERSION,
        meta: model::ReportMeta {
            map_id: w.meta.map_id, map_name, family, mode: mode.to_string(),
            duration_ms: w.meta.duration_ms, you,
        },
        players,
        findings,
        caveats: vec![float::CAVEAT.to_string(), model::MACRO_CAVEAT.to_string()],
    })
}

/// "you" = Auto (the recorder) or an explicit name/profile selector. An explicit
/// selector that matches no player is an ERROR (no-fallback rule) — silently
/// coaching the wrong player would be worse than failing.
fn resolve_you(sel: &YouSel, players: &[model::PlayerInfo], rec: i32) -> anyhow::Result<i32> {
    match sel {
        YouSel::Name(n) => players.iter()
            .find(|p| p.name.eq_ignore_ascii_case(n)).map(|p| p.player_number)
            .ok_or_else(|| anyhow::anyhow!(
                "--you '{n}' matches no player (players: {})",
                players.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "))),
        YouSel::ProfileId(id) => players.iter()
            .find(|p| p.profile_id == *id).map(|p| p.player_number)
            .ok_or_else(|| anyhow::anyhow!("profile {id} is not a player in this replay")),
        YouSel::Auto => Ok(rec),
    }
}
```

Rewire the existing `run()` to call it, temporarily bridging to the OLD `render(...)`
signature (Task 3 collapses it; this keeps Task 2 independently green). Delete the old
`resolve_you(&Option<String>, &Walked)` function.

```rust
pub fn run(args: AnalyzeArgs) -> Result<()> {
    let game = load_game(&args.input)?;
    let sel = match &args.you { Some(n) => YouSel::Name(n.clone()), None => YouSel::Auto };
    let report = analyze(&game, &sel)?;
    // temporary bridge to the pre-Task-3 render signature; render only reads duration_ms
    let meta = model::GameMeta {
        map_id: report.meta.map_id, duration_ms: report.meta.duration_ms, rec_player: report.meta.you,
    };
    print!("{}", report::render(&meta, &report.meta.map_name, report.meta.family,
                                &report.players, &report.findings, report.meta.you));
    Ok(())
}
```

- [ ] **Step 4: Run the full suite**

Run: `cargo test -p replay-rs`
Expected: ALL PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(analyze): pure analyze(&Savegame, &YouSel) -> Report; mode decided once"
```

---

### Task 3: `report::render(&Report)` — collapse the six-param signature

**Files:**
- Modify: `src/analyze/report.rs`, `src/analyze/mod.rs` (run() call site)

**Interfaces:**
- Produces: `report::render(report: &Report) -> String`.

- [ ] **Step 1: Update the test** in `src/analyze/report.rs` — replace the existing `report_contains_player_and_top_fix` body's render call with a `Report`:

```rust
    #[test]
    fn report_contains_player_and_top_fix() {
        let report = Report {
            schema_version: SCHEMA_VERSION,
            meta: ReportMeta { map_id: 9, map_name: "Arabia".into(), family: Family::Open,
                               mode: "1v1".into(), duration_ms: 2_400_000, you: 1 },
            players: vec![pm("MKD", 1)],
            findings: vec![Finding {
                player_number: 1, metric: "idle TC (dark)".into(), your: "30s".into(),
                reference: "~0-15s".into(), basis: Basis::Absolute, severity: Severity::High,
                note: "tighten production".into(),
            }],
            caveats: vec![MACRO_CAVEAT.to_string()],
        };
        let s = render(&report);
        assert!(s.contains("Arabia"));
        assert!(s.contains("Top Fixes"));
        assert!(s.contains("MKD"));
        assert!(s.contains("idle TC (dark)"));
        assert!(s.contains("macro coach only")); // caveats now come from the Report
    }
```

(Adjust the test's `use` line to import `Report, ReportMeta, SCHEMA_VERSION, MACRO_CAVEAT`.)

- [ ] **Step 2: Run to verify failure** — `cargo test -p replay-rs report::tests` — FAIL (arity).

- [ ] **Step 3: Implement.** Change the signature and header of `render`:

```rust
pub fn render(report: &Report) -> String {
    let meta = &report.meta;
    let metrics = &report.players;
    let findings = &report.findings;
    let you = meta.you;
    let family = meta.family;
```

Inside, replace `map_name` with `meta.map_name`, `meta.duration_ms` stays, and replace the two hardcoded trailing `note:` lines with:

```rust
    // --- honest footer (verbatim from the Report so JSON and terminal agree) ---
    s.push('\n');
    for c in &report.caveats {
        s.push_str(&format!("note: {c}\n"));
    }
```

Remove the now-unused `use crate::analyze::float;` and `GameMeta` import. Update `run()` in `mod.rs` to `print!("{}", report::render(&report));` and delete the temporary `GameMeta` reconstruction from Task 2.

- [ ] **Step 4: Run the full suite** — `cargo test -p replay-rs` — ALL PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "refactor(analyze): render(&Report) — one input, caveats from the report"
```

---

### Task 4: `--json` flag (replaces pretty output)

**Files:**
- Modify: `src/main.rs` (`cmd_analyze`), `src/analyze/model.rs` (`AnalyzeArgs`), `src/analyze/mod.rs` (`run`)

**Interfaces:**
- Produces: `AnalyzeArgs { input, you, json: bool }`; `analyze <input> --json` prints one pretty JSON document, nothing else.

- [ ] **Step 1:** Add `pub json: bool` to `AnalyzeArgs` in `model.rs`. In `main.rs` `cmd_analyze`, add to the parse loop:

```rust
            "--json" => json = true,
```

(declare `let mut json = false;` above; pass `json` into `AnalyzeArgs`).

- [ ] **Step 2:** In `analyze/mod.rs` `run()`:

```rust
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report::render(&report));
    }
```

- [ ] **Step 3: Verify by hand** (no unit test — serialization is covered by Task 1; this is 4 lines of plumbing):

```bash
cargo build --release
./target/release/replay-rs analyze "$HOME/İndirilenler/AgeIIDE_Replay_475242532.aoe2record" --json | head -30
./target/release/replay-rs analyze "$HOME/İndirilenler/AgeIIDE_Replay_475242532.aoe2record" --json | python3 -m json.tool > /dev/null && echo VALID
```

Expected: pure JSON on stdout, first keys `schema_version`, `meta`; `VALID`. (Any local `.aoe2record` works if that one is gone.)

- [ ] **Step 4:** Update the `analyze` line in `print_usage` and the doc comment at the top of `main.rs`:

```
replay-rs analyze <file.aoe2record>|--match-id N [--you NAME] [--json]
```

- [ ] **Step 5:** `cargo test -p replay-rs` — ALL PASS. Commit:

```bash
git add -A && git commit -m "feat(analyze): --json flag — stdout becomes one pretty JSON Report"
```

---

### Task 5: The lib/bin split

**Files:**
- Modify: `Cargo.toml`, `src/main.rs`, `src/api.rs` + any bin module using `crate::config`
- Create: `src/lib.rs`
- Move: `src/analyze/report.rs` → `src/report.rs`
- Modify: `src/analyze/mod.rs`, `src/analyze/model.rs` (CLI types leave the lib)

**Interfaces:**
- Produces: lib crate `replay_rs` exposing `replay_rs::analyze::{analyze, Report, YouSel, data, model}` and `replay_rs::config`; bin keeps `api`, `report`, and all pipeline modules. `AnalyzeArgs` is deleted; `Input` becomes a bin-local enum in `main.rs`.

- [ ] **Step 1:** Add to `Cargo.toml` above `[[bin]]`:

```toml
[lib]
name = "replay_rs"
path = "src/lib.rs"
```

- [ ] **Step 2:** Create `src/lib.rs`:

```rust
//! Library surface: pure replay analysis (`analyze`) + shared AoE2/API constants
//! (`config`). All IO — network, file reads, terminal rendering — lives in the
//! binary. This boundary is what the future WASM build compiles.
pub mod analyze;
pub mod config;
```

- [ ] **Step 3:** Purge IO from the lib:
  - Delete `run()` and `load_game()` from `src/analyze/mod.rs`; delete `use crate::api;` and the `anyhow`/`Result` imports it needed; delete `pub use model::{AnalyzeArgs, Input};` (keep `pub use model::{Report, YouSel};`). Delete `pub mod report;`.
  - Delete `AnalyzeArgs`, `Input`, and `use std::path::PathBuf;` from `src/analyze/model.rs`.
  - `git mv src/analyze/report.rs src/report.rs`. Fix its imports to `use replay_rs::analyze::model::{Basis, Family, Report, Role, Severity};` (plus whatever the compiler asks for). The `pm()` test builder imports `PlayerInfo, PlayerMetrics` the same way.

- [ ] **Step 4:** Rewire the bin. In `src/main.rs`:
  - Remove `mod analyze;` and `mod config;`. Add `mod report;`.
  - `grep -rn "crate::config" src/ --include='*.rs'` — in every file OUTSIDE `src/analyze/` (expect `api.rs`, `extract.rs`, `pipeline.rs`, possibly `store.rs`/`manifest.rs`/`seed.rs`), change `use crate::config;`/`crate::config::` to `use replay_rs::config;`/`replay_rs::config::`. Files INSIDE `src/analyze/` keep `crate::config` (they're in the lib).
  - Replace `cmd_analyze` + add the moved loader:

```rust
/// Where the replay bytes come from (CLI-side concern; the lib only sees &Savegame).
enum Input { File(std::path::PathBuf), MatchId(i64) }

fn cmd_analyze(args: &[String]) -> Result<()> {
    use replay_rs::analyze::{self, YouSel};
    let mut input: Option<Input> = None;
    let mut you = YouSel::Auto;
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--match-id" => input = Some(Input::MatchId(take_value(args, &mut i, "--match-id")?.parse()?)),
            "--you" => you = YouSel::Name(take_value(args, &mut i, "--you")?),
            "--json" => json = true,
            v if !v.starts_with("--") => input = Some(Input::File(std::path::PathBuf::from(v))),
            other => bail!("analyze: unknown flag {other}"),
        }
        i += 1;
    }
    let input = input.ok_or_else(|| anyhow::anyhow!("analyze: need <file.aoe2record> or --match-id N"))?;
    let game = load_game(&input)?;
    let report = analyze::analyze(&game, &you)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report::render(&report));
    }
    Ok(())
}

/// File read or getReplayFiles->download — the IO the lib deliberately doesn't do.
fn load_game(input: &Input) -> Result<aoe2rec::Savegame> {
    // body moved VERBATIM from the old src/analyze/mod.rs load_game
}
```

  (`serde_json` is already a direct dependency of the package, usable from the bin.)

- [ ] **Step 5:** `cargo test -p replay-rs` — ALL PASS (lib tests + bin tests). Then `cargo build --release` and re-run the Task 4 Step 3 smoke commands — same output as before the split.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "refactor(replay-rs): [lib] replay_rs (pure analyze+config) / bin keeps all IO; report.rs moves bin-side"
```

---

### Task 6: `api::get_recent_matches` (probe first, then fixture-locked serde)

**Files:**
- Modify: `src/api.rs`

**Interfaces:**
- Produces: `api::RecentMatch { match_id: i64, completed_unix: i64, map_raw: Option<String>, ladder: Option<u32>, team_size: usize, my_civ_id: Option<u32>, my_rating: Option<i32>, my_won: Option<bool>, my_alias: Option<String> }`; `api::get_recent_matches(client, profile_id: i64) -> Result<Vec<RecentMatch>>` (AUTOMATCH-only, completed-only, newest first).

- [ ] **Step 1: PROBE the live endpoint before writing any struct** (source-derived rule — no guessed schemas):

```bash
# 1. grab a live profile_id from the 1v1 RM ladder (leaderboard_id=3; source: stream-relic.mjs)
curl -s "https://aoe-api.worldsedgelink.com/community/leaderboard/getLeaderBoard2?title=age2&leaderboard_id=3&start=1&count=3&sortBy=1" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['statGroups'][0]['members'][0]['profile_id'])"
# 2. fetch that player's recent history (substitute PID)
curl -s "https://aoe-api.worldsedgelink.com/community/leaderboard/getRecentMatchHistory?title=age2&profile_ids=%5BPID%5D" > target/recent-probe.json
python3 -c "import json; d=json.load(open('target/recent-probe.json')); m=[x for x in d['matchHistoryStats'] if x['description']=='AUTOMATCH'][0]; print(json.dumps({k:m[k] for k in ('id','completiontime','description','mapname','matchtype_id')}, indent=1)); print(json.dumps(m['matchhistorymember'][0], indent=1)); print(json.dumps(d['profiles'][0], indent=1))"
```

Confirm the field names used below (`id`, `completiontime`, `description`, `mapname`, `matchtype_id`, `matchhistorymember[].{profile_id, civilization_id, oldrating, newrating, outcome}`, `profiles[].{profile_id, alias}`). If any differ from the structs in Step 4, the STRUCTS change to match reality — never the other way. Trim ~2 AUTOMATCH matches + 1 non-AUTOMATCH + the matching `profiles` entries into a compact JSON for the test fixture in Step 2.

- [ ] **Step 2: Write the failing test** in `src/api.rs` — paste the REAL trimmed probe JSON as the fixture (the shape below shows the required scenario; replace values with probed ones):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed REAL getRecentMatchHistory response (probed <date>): 2 AUTOMATCH matches
    // (out of order) + 1 custom lobby that must be filtered out.
    const FIXTURE: &str = r#"{
      "matchHistoryStats": [
        {"id": 111, "completiontime": 1000, "description": "AUTOMATCH", "mapname": "arabia", "matchtype_id": 6,
         "matchhistorymember": [
           {"profile_id": 42, "civilization_id": 2, "oldrating": 1290, "newrating": 1301, "outcome": 1},
           {"profile_id": 7,  "civilization_id": 5, "oldrating": 1310, "newrating": 1299, "outcome": 0}]},
        {"id": 333, "completiontime": 3000, "description": "CUSTOM", "mapname": "arabia", "matchtype_id": 0,
         "matchhistorymember": [{"profile_id": 42, "civilization_id": 2, "outcome": 1}]},
        {"id": 222, "completiontime": 2000, "description": "AUTOMATCH", "mapname": "arena", "matchtype_id": 6,
         "matchhistorymember": [
           {"profile_id": 42, "civilization_id": 8, "oldrating": 1301, "newrating": 1288, "outcome": 0},
           {"profile_id": 9,  "civilization_id": 1, "oldrating": 1280, "newrating": 1293, "outcome": 1}]}
      ],
      "profiles": [{"profile_id": 42, "alias": "MKD"}, {"profile_id": 7, "alias": "Other"}]
    }"#;

    #[test]
    fn recent_matches_filters_sorts_and_joins_me() {
        let doc: RecentHistoryResponse = serde_json::from_str(FIXTURE).unwrap();
        let ms = normalize_recent(doc, 42);
        assert_eq!(ms.len(), 2); // CUSTOM dropped
        assert_eq!(ms[0].match_id, 222); // newest first (completiontime desc)
        assert_eq!(ms[0].my_won, Some(false));
        assert_eq!(ms[0].my_rating, Some(1288)); // newrating preferred
        assert_eq!(ms[1].my_civ_id, Some(2));
        assert_eq!(ms[0].my_alias.as_deref(), Some("MKD"));
        assert_eq!(ms[0].team_size, 2);
    }
}
```

- [ ] **Step 3: Run to verify failure** — `cargo test -p replay-rs api::tests` — FAIL (types not defined).

- [ ] **Step 4: Implement** (append to `src/api.rs`; field names locked by the Step 1 probe):

```rust
// --- getRecentMatchHistory (recent ranked games for one profile) --------------
// source: same endpoint + normalization rules as scripts/data-pipeline/stream-relic.mjs
// (in production via the 3h cron): AUTOMATCH description = ranked matchmaking;
// outcome 1 = win; newrating falls back to oldrating.

#[derive(Debug, Deserialize)]
struct RecentHistoryResponse {
    #[serde(default, rename = "matchHistoryStats")]
    match_history_stats: Vec<MatchStat>,
    #[serde(default)]
    profiles: Vec<ProfileEntry>,
}

#[derive(Debug, Deserialize)]
struct MatchStat {
    id: i64,
    #[serde(default)]
    completiontime: i64,
    #[serde(default)]
    description: String,
    #[serde(default)]
    mapname: Option<String>,
    #[serde(default)]
    matchtype_id: Option<u32>,
    #[serde(default)]
    matchhistorymember: Vec<MatchMember>,
}

#[derive(Debug, Deserialize)]
struct MatchMember {
    profile_id: i64,
    #[serde(default)]
    civilization_id: Option<u32>,
    #[serde(default)]
    oldrating: Option<i32>,
    #[serde(default)]
    newrating: Option<i32>,
    #[serde(default)]
    outcome: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct ProfileEntry {
    profile_id: i64,
    #[serde(default)]
    alias: Option<String>,
}

/// One recent ranked game from the profile's point of view.
#[derive(Debug, Clone)]
pub struct RecentMatch {
    pub match_id: i64,
    pub completed_unix: i64,
    pub map_raw: Option<String>,
    pub ladder: Option<u32>,
    pub team_size: usize,
    pub my_civ_id: Option<u32>,
    pub my_rating: Option<i32>,
    pub my_won: Option<bool>,
    pub my_alias: Option<String>,
}

/// AUTOMATCH + completed only, newest first, "me" fields joined by profile_id.
fn normalize_recent(doc: RecentHistoryResponse, profile_id: i64) -> Vec<RecentMatch> {
    let alias = doc.profiles.iter()
        .find(|p| p.profile_id == profile_id)
        .and_then(|p| p.alias.clone());
    let mut out: Vec<RecentMatch> = doc.match_history_stats.into_iter()
        .filter(|m| m.description == "AUTOMATCH" && m.completiontime > 0)
        .map(|m| {
            let me = m.matchhistorymember.iter().find(|x| x.profile_id == profile_id);
            RecentMatch {
                match_id: m.id,
                completed_unix: m.completiontime,
                map_raw: m.mapname,
                ladder: m.matchtype_id,
                team_size: m.matchhistorymember.len(),
                my_civ_id: me.and_then(|x| x.civilization_id),
                my_rating: me.and_then(|x| x.newrating.or(x.oldrating)),
                my_won: me.and_then(|x| x.outcome.map(|o| o == 1)),
                my_alias: alias.clone(),
            }
        })
        .collect();
    out.sort_by_key(|m| std::cmp::Reverse(m.completed_unix));
    out
}

/// Recent ranked games for one profile (the API returns roughly the last ~10).
pub fn get_recent_matches(
    client: &reqwest::blocking::Client,
    profile_id: i64,
) -> Result<Vec<RecentMatch>> {
    let url = format!(
        "{}/getRecentMatchHistory?title={}&profile_ids=%5B{}%5D",
        config::API_BASE, config::TITLE, profile_id
    );
    let raw = get_bytes(client, &url, 4)?;
    let doc: RecentHistoryResponse = serde_json::from_slice(&raw)?;
    Ok(normalize_recent(doc, profile_id))
}
```

- [ ] **Step 5:** `cargo test -p replay-rs` — ALL PASS. Delete `target/recent-probe.json`.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(api): get_recent_matches — probed getRecentMatchHistory, fixture-locked serde"
```

---

### Task 7: `recent` subcommand (list latest ranked games)

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `api::get_recent_matches`, `replay_rs::analyze::data::load_civs`.
- Produces: `replay-rs recent --profile-id P [--limit N]`; private helpers `resolve_profile(Option<i64>) -> Result<i64>` and `ago(secs: i64) -> String`.

- [ ] **Step 1: Write the failing test** for the pure helper (in `src/main.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ago_formats_minutes_hours_days() {
        assert_eq!(ago(90), "1m ago");
        assert_eq!(ago(2 * 3600 + 100), "2h ago");
        assert_eq!(ago(3 * 86_400 + 5), "3d ago");
        assert_eq!(ago(-5), "0m ago"); // clock skew never goes negative
    }
}
```

- [ ] **Step 2:** `cargo test -p replay-rs ago_formats` — FAIL (`ago` not defined).

- [ ] **Step 3: Implement** in `src/main.rs`: add the dispatch arm `"recent" => cmd_recent(rest),` and:

```rust
/// --profile-id N, or the explicit AOE2_PROFILE_ID env; absence is a loud error
/// (no-defaults rule: we never guess whose games to fetch).
fn resolve_profile(flag: Option<i64>) -> Result<i64> {
    flag.or_else(|| std::env::var("AOE2_PROFILE_ID").ok().and_then(|v| v.parse().ok()))
        .ok_or_else(|| anyhow::anyhow!(
            "need --profile-id N (or AOE2_PROFILE_ID env). Find yours on aoe2insights.com / aoe2companion.com"))
}

/// Human "how long ago" from a seconds delta (no chrono dep — keep it light).
fn ago(secs: i64) -> String {
    let s = secs.max(0);
    if s < 3600 { format!("{}m ago", s / 60) }
    else if s < 86_400 { format!("{}h ago", s / 3600) }
    else { format!("{}d ago", s / 86_400) }
}

/// `recent --profile-id P [--limit N]` — list recent ranked games, newest first.
fn cmd_recent(args: &[String]) -> Result<()> {
    let mut profile: Option<i64> = None;
    let mut limit: Option<usize> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--profile-id" => profile = Some(take_value(args, &mut i, "--profile-id")?.parse()?),
            "--limit" => limit = Some(take_value(args, &mut i, "--limit")?.parse()?),
            other => bail!("recent: unknown flag {other}"),
        }
        i += 1;
    }
    let profile = resolve_profile(profile)?;
    let client = api::build_client()?;
    let matches = api::get_recent_matches(&client, profile)?;
    if matches.is_empty() {
        bail!("recent: the API returned no recent ranked matches for profile {profile}");
    }
    let civs = replay_rs::analyze::data::load_civs();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;
    println!(
        "recent ranked games — {} (profile {profile})",
        matches[0].my_alias.as_deref().unwrap_or("?")
    );
    println!("  {:<12} {:>8}  {:<20} {:<5} {:<14} {:>5}  {}",
        "match_id", "when", "map", "mode", "civ", "elo", "result");
    for m in matches.iter().take(limit.unwrap_or(usize::MAX)) {
        // 2 members = 1v1; 4/6/8 = 2v2/3v3/4v4. source: stream-relic.mjs keepBySize.
        let mode = if m.team_size == 2 { "1v1".to_string() } else { format!("{0}v{0}", m.team_size / 2) };
        let civ = m.my_civ_id.and_then(|id| civs.get(&id).cloned()).unwrap_or_else(|| "?".into());
        let result = match m.my_won { Some(true) => "win", Some(false) => "loss", None => "?" };
        println!("  {:<12} {:>8}  {:<20} {:<5} {:<14} {:>5}  {}",
            m.match_id, ago(now - m.completed_unix), m.map_raw.as_deref().unwrap_or("?"),
            mode, civ, m.my_rating.map(|r| r.to_string()).unwrap_or_else(|| "-".into()), result);
    }
    println!("\nanalyze one:  replay-rs analyze --match-id <id> --profile-id {profile}");
    Ok(())
}
```

Add to `print_usage` and the top doc comment: `replay-rs recent --profile-id P [--limit N]`.

- [ ] **Step 4:** `cargo test -p replay-rs` — ALL PASS. Live smoke (use the probe profile id from Task 6, or your own):

```bash
cargo build --release && ./target/release/replay-rs recent --profile-id <PID> --limit 5
```

Expected: a header + up to 5 rows, newest first.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(cli): recent subcommand — list latest ranked games for a profile"
```

---

### Task 8: `analyze --latest [N|all]` (fetch + analyze recent games; NDJSON with --json)

**Files:**
- Modify: `src/main.rs` (`cmd_analyze`)

**Interfaces:**
- Consumes: `api::{get_recent_matches, get_replay_files, best_file, download_replay}`, `replay_rs::analyze::{analyze, YouSel}`, `replay_rs::config::REPLAYFILES_BATCH`, Task 7's `resolve_profile`.
- Produces: `analyze --latest [N|all] --profile-id P [--json]`. Terminal: reports print sequentially. `--json`: NDJSON, one compact `Report` per line. Missing replays are warned + skipped; zero successes is a hard error.

- [ ] **Step 1: Write the failing test** for the flag-value parser (pure; in `src/main.rs` tests):

```rust
    #[test]
    fn latest_value_parses_default_number_and_all() {
        assert!(matches!(parse_latest(None), Ok(Latest::N(1))));
        assert!(matches!(parse_latest(Some("3")), Ok(Latest::N(3))));
        assert!(matches!(parse_latest(Some("all")), Ok(Latest::All)));
        assert!(parse_latest(Some("banana")).is_err());
    }
```

- [ ] **Step 2:** `cargo test -p replay-rs latest_value` — FAIL.

- [ ] **Step 3: Implement.** In `src/main.rs`:

```rust
/// --latest's optional value: bare flag = 1, a number = N, "all" = whatever
/// getRecentMatchHistory returns (~last 10).
enum Latest { N(usize), All }

fn parse_latest(v: Option<&str>) -> Result<Latest> {
    match v {
        None => Ok(Latest::N(1)),
        Some("all") => Ok(Latest::All),
        Some(n) => n.parse().map(Latest::N)
            .map_err(|_| anyhow::anyhow!("analyze: --latest takes a number or 'all', got {n}")),
    }
}
```

Extend `cmd_analyze`'s parse loop (`let mut latest: Option<Latest> = None; let mut profile: Option<i64> = None;`):

```rust
            "--latest" => {
                // optional value: consume the next arg only if it isn't another flag
                let peek = args.get(i + 1).map(String::as_str).filter(|v| !v.starts_with("--"));
                if peek.is_some() { i += 1; }
                latest = Some(parse_latest(peek)?);
            }
            "--profile-id" => profile = Some(take_value(args, &mut i, "--profile-id")?.parse()?),
```

After the loop, branch BEFORE the single-input path:

```rust
    if let Some(latest) = latest {
        if input.is_some() {
            bail!("analyze: --latest conflicts with a file / --match-id input");
        }
        let profile = resolve_profile(profile)?;
        let client = api::build_client()?;
        let recent = api::get_recent_matches(&client, profile)?;
        let take = match latest { Latest::All => recent.len(), Latest::N(n) => n.min(recent.len()) };
        if take == 0 {
            bail!("analyze: no recent ranked matches for profile {profile}");
        }
        let ids: Vec<i64> = recent.iter().take(take).map(|m| m.match_id).collect();
        let mut analyzed = 0usize;
        for chunk in ids.chunks(replay_rs::config::REPLAYFILES_BATCH) {
            let per = api::get_replay_files(&client, chunk)?;
            for id in chunk {
                let Some(files) = per.get(id) else {
                    eprintln!("match {id}: replay expired/aged out — skipped");
                    continue;
                };
                let Some(best) = api::best_file(files) else {
                    eprintln!("match {id}: no uploaded replay — skipped");
                    continue;
                };
                let Some(url) = best.url.clone() else {
                    eprintln!("match {id}: replay has no url — skipped");
                    continue;
                };
                let game = match api::download_replay(&client, &url)
                    .and_then(|b| aoe2rec::Savegame::from_bytes(b)
                        .map_err(|e| anyhow::anyhow!("parse: {e}")))
                {
                    Ok(g) => g,
                    Err(e) => { eprintln!("match {id}: {e:#} — skipped"); continue; }
                };
                // Err here = profile not a player in this replay (e.g. an oddball
                // history entry) — warn + skip like the other per-match failures.
                let report = match replay_rs::analyze::analyze(&game, &replay_rs::analyze::YouSel::ProfileId(profile)) {
                    Ok(r) => r,
                    Err(e) => { eprintln!("match {id}: {e:#} — skipped"); continue; }
                };
                if json {
                    println!("{}", serde_json::to_string(&report)?); // NDJSON: one line per game
                } else {
                    println!("\n═══ match {id} ═══");
                    print!("{}", report::render(&report));
                }
                analyzed += 1;
            }
        }
        if analyzed == 0 {
            bail!("analyze: none of the {take} recent matches had a downloadable replay \
                   (replays only exist when uploaded and age out after ~2 weeks)");
        }
        return Ok(());
    }
```

Also honor `--profile-id` on the single-input path (a downloaded `--match-id` replay's recorder isn't necessarily you): after the loop, `if let Some(p) = profile { you = YouSel::ProfileId(p); }` — but only when `--you` wasn't given (explicit name wins):

```rust
    if matches!(you, YouSel::Auto) {
        if let Some(p) = profile { you = YouSel::ProfileId(p); }
    }
```

Update `print_usage` + top doc comment:

```
replay-rs analyze <file>|--match-id N|--latest [N|all] [--you NAME] [--profile-id P] [--json]
```

- [ ] **Step 4:** `cargo test -p replay-rs` — ALL PASS. Live smoke:

```bash
cargo build --release
./target/release/replay-rs analyze --latest 2 --profile-id <PID>
./target/release/replay-rs analyze --latest 2 --profile-id <PID> --json | wc -l   # expect: up to 2 lines
```

Expected: up to 2 sequential reports (or per-match "skipped" warnings on stderr); NDJSON line count ≤ 2 with only-JSON stdout.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(cli): analyze --latest [N|all] — fetch + analyze recent ranked games (NDJSON with --json)"
```

---

### Task 9: README + final verification

**Files:**
- Modify: `scripts/data-pipeline/replay-rs/README.md` (the "Analyze" section)

- [ ] **Step 1:** In the README's Analyze section, update the usage block and add the new capabilities:

```sh
# a local save (the recording player is auto-detected as "you")
./target/release/replay-rs analyze "/path/to/MP Replay ….aoe2record"

# or by match id; --you NAME / --profile-id P pins "you" (name wins if both)
./target/release/replay-rs analyze --match-id 475242532 --profile-id 199325

# machine-readable: stdout becomes ONE pretty-JSON Report (schema_version 1)
./target/release/replay-rs analyze --match-id 475242532 --json

# your latest ranked games (getRecentMatchHistory): list, then analyze
./target/release/replay-rs recent --profile-id 199325 --limit 5
./target/release/replay-rs analyze --latest --profile-id 199325       # newest game
./target/release/replay-rs analyze --latest all --profile-id 199325 --json   # NDJSON, one Report/line
# AOE2_PROFILE_ID env can stand in for --profile-id. Replays only exist when
# uploaded and age out after ~2 weeks — unavailable ones are skipped with a warning.
```

Add one paragraph noting the crate is now lib (`replay_rs`: pure `analyze()` + `config`) + bin (all IO), and that the JSON `Report` (schema v1, snake_case enums, `caveats` included) is the contract a future WASM/site consumer builds on.

- [ ] **Step 2: Full gate run**

```bash
cargo test -p replay-rs 2>&1 | tail -5      # expect: all tests pass, 0 failed
cargo build --release 2>&1 | tail -3        # expect: clean build
./target/release/replay-rs analyze "$HOME/İndirilenler/AgeIIDE_Replay_475242532.aoe2record" | head -20  # pretty report unchanged
```

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "docs(replay-rs): README — lib/bin split, --json contract, recent/--latest usage"
```
