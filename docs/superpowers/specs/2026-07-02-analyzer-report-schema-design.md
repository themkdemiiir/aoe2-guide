# Analyzer Phase C — lib/bin split, `Report` struct, `--json`

**Status:** design approved 2026-07-02 (brainstormed in-session; decisions locked below).
**Scope:** `scripts/data-pipeline/replay-rs/` only. No site changes.
**Context:** Phases A (de-risk) and B (correctness pass) are done on
`feat/analyzer-foundation`. This phase makes the analyzer consumable by anything
other than a terminal: a `Report` value that serializes to JSON, produced by a pure
library function, with the CLI as its first consumer. The future WASM/browser step
(see the 2026-06-26 game-analyzer design, "WASM direction") builds on exactly this
boundary — which is why the JSON shape is a one-way door and got designed first.

## Decisions locked (from the brainstorm)

1. **Report scope = v1 only.** The schema wraps exactly what the analyzer computes
   today (age-ups, vils@Castle, idle-TC, first military, eco techs, APM, float
   estimate, role/coords, ELO, findings). v2 metrics (eAPM, opening classifier,
   build-order timeline — see the v2-ideas doc) are additive later; nothing in this
   shape blocks them.
2. **`schema_version: 1`** on the top-level `Report`. One `u32`; a future breaking
   change bumps it so a consumer can detect rather than misparse.
3. **`report.rs` (terminal render) stays in the bin.** The lib is pure data +
   analysis; comfy-table/owo-colors are CLI presentation and irrelevant to WASM.
4. **`--json` fully replaces the pretty output.** With the flag, stdout is exactly
   one JSON document (`serde_json::to_string_pretty`) — no table, no color codes.

## Data-correctness pre-check (done 2026-07-01/02)

The schema's civ/map/cost identifier spaces were verified against their sources
before locking the shape:

- `data/civs.tsv` (61 ids) matches `src/data/civ-id-map.json` byte-for-byte; all
  59 real slugs match the aoe2techtree icon set. Ids 46–48/54–56 are Chronicles
  DLC civs (real, in aoe2techtree's separate `chronicles/` tree — kept because raw
  replay `civ_id`s can carry them); id 57 has no source anywhere and stays
  `non-guide-57` rather than a guess.
- `data/costs.json` regenerated at the new `AOE2TT_SHA` pin `b9d494df`
  (DE Update 177723 balance changes included; commit `ffac22d`).
- `data/maps.tsv` (193 gameplay maps) matches its documented Fandom-derived
  methodology; single source of truth for `Family`.

## Architecture

### Cargo targets

```toml
[lib]
name = "replay_rs"
path = "src/lib.rs"

[[bin]]                    # unchanged
name = "replay-rs"
path = "src/main.rs"
```

- `src/lib.rs` (new): `pub mod analyze; pub mod config;` — **`config.rs` moves into
  the lib** (pure AoE2 constants; `analyze::metrics` already imports it). Bin code
  references it as `replay_rs::config`; no constant is duplicated.
- Bin-only modules stay declared in `main.rs`: `api` (network), `extract`,
  `manifest`, `pipeline`, `seed`, `store`, and `report` (moved to `src/report.rs`,
  consuming `replay_rs::analyze::Report`).

**Known follow-up for the WASM phase (not this phase):** `[dependencies]` are
package-wide, so `reqwest`/`rusqlite` would break a naive `wasm32` build of the lib
target. The WASM step already plans a separate wrapper crate; at that point the IO
deps either move behind a cargo feature or the lib extracts into a workspace member.
Phase C deliberately does not pre-build that machinery.

### The pure entry point

```rust
// lib: analyze/mod.rs
pub fn analyze(game: &Savegame, you: &YouSel) -> Report
pub enum YouSel { Auto, Name(String), ProfileId(i64) }
```

> Amended 2026-07-02 (was `Option<&str>`): the `--latest` addendum needs "you"
> resolved by profile id — a downloaded replay's recording player is whoever
> uploaded it, not necessarily you. `PlayerInfo` correspondingly gains
> `profile_id: i64` (already extracted by the bulk pipeline; additive to the
> JSON contract and useful to any site consumer).

- Runs walk → maps lookup → roles/bases/coords → metrics → findings → assembles
  `Report`. No file IO, no network, no printing (the committed `data/*` files are
  `include_str!`-baked, so loading them is pure).
- `you` resolution (name match, else `rec_player`) moves inside, since the result
  (`meta.you`) is part of the Report.
- The bin's `cmd_analyze` becomes: load bytes (file or `--match-id` download via
  `api.rs`) → `Savegame` → `replay_rs::analyze::analyze(&game, you)` → either
  `report::render(&report)` or `serde_json::to_string_pretty(&report)`.
- `analyze::run(AnalyzeArgs)` (current IO-entangled entry) moves to the bin side;
  the lib keeps no knowledge of `AnalyzeArgs`/`Input` (CLI types move with it).

### `Report` shape (serde, one-way door)

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema_version: u32,          // = 1
    pub meta: ReportMeta,
    pub players: Vec<PlayerMetrics>,  // existing struct + Serialize; fields unchanged
    pub findings: Vec<Finding>,       // FULL list, severity-sorted; terminal's take(8) is a render choice
    pub caveats: Vec<String>,         // the float-estimator + macro-coach-only notes, verbatim
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportMeta {
    pub map_id: u32,
    pub map_name: String,
    pub family: Family,               // snake_case: "open" | "closed" | …
    pub mode: String,                 // "1v1" | "team" — promoted from compare.rs's inline computation
    pub duration_ms: u32,
    pub you: i32,                     // player_number
}
```

- `PlayerMetrics`, `PlayerInfo`, `Finding`, `Family`, `Role`, `Basis`, `Severity`
  gain `Serialize` (+ `#[serde(rename_all = "snake_case")]` on the enums — so
  `Basis::YourElo` emits `"your_elo"`, not `"yourelo"`; single-word variants are
  unaffected). No field is renamed, retyped, or dropped — purely additive to
  Phase B's audited values.
- `caveats` exists so a JSON consumer gets the same honesty footer the terminal
  prints (source-derived ethos: never let the JSON imply more certainty than the
  terminal admits). Content = the two existing note lines.
- `mode` today is computed ad hoc inside `compare.rs`; Phase C computes it once,
  passes it to compare, and records it in `ReportMeta` so consumer and findings
  can never disagree on it.
- Time values stay `*_ms` integers (JSON-safe); formatting is the renderer's job.

### `--json` wiring

- `AnalyzeArgs` gains `json: bool`; `main.rs` parses a bare `--json` flag in the
  existing hand-rolled loop (no clap — standing constraint).
- `report::render()` collapses from six loose params to `render(&Report) -> String`.

## Testing

- Existing metric/compare/report tests keep passing (report test constructs a
  `Report` now — mechanical adaptation).
- New: JSON round-trip test — hand-built `Report` → `to_string` → `from_str`
  (test-only `Deserialize`), assert `schema_version == 1` and spot fields survive
  with the expected snake_case enum spellings.
- New: `analyze()` purity is enforced by the type system (lib has no
  reqwest/file-IO imports) — no test needed beyond compilation.
- Smoke (`#[ignore]`, existing pattern): `analyze` a real local save with `--json`,
  pipe through `jq .schema_version`.

## Addendum (2026-07-02, user-requested): latest played games

Fetch and analyze a player's recent ranked games without hand-copying match ids.
Grounded on the already-validated `getRecentMatchHistory` endpoint (same API +
normalization rules as `scripts/data-pipeline/stream-relic.mjs`, in production via
the 3h cron): `AUTOMATCH` only, completed matches, newest first.

- **`replay-rs recent --profile-id P [--limit N]`** — list recent ranked games:
  match_id, date, map, mode (1v1/team), civ, rating, win/loss. Pure listing; the
  user picks a match_id to analyze, or uses `--latest`.
- **`analyze --latest [N|all] --profile-id P`** — resolve the N most recent
  completed matches (default 1; `all` = everything the history returns — the
  probe returned 129 history entries; the AUTOMATCH subset varies by profile)
  and run the existing download→parse→analyze path on each. Terminal mode prints
  reports sequentially; `--json` mode emits **NDJSON** (one `Report` per line —
  consistent with the pipeline's shard format; a single `--match-id`/file stays a
  single JSON document).
- `--profile-id` is required (numeric, from aoe2companion/aoe2insights); an
  `AOE2_PROFILE_ID` env var may serve as explicit default. Per the no-defaults
  rule, absence of both is a loud error, never a guess.
- All network code stays **bin-side** (`api.rs` grows a `get_recent_matches`),
  preserving the pure-lib boundary.
- Implementation must start by probing the endpoint once and locking the serde
  structs against the real response (field names like `matchhistorystats`,
  `profiles` alias join) — source-derived, no guessed schemas.
- Honest caveat surfaced in output: replays only exist for uploaded matches and
  age out after ~2 weeks; unavailable replays are reported per-match and skipped,
  not fatal.

## Non-goals

- No v2 metrics (eAPM, opening classifier, timeline) — additive later.
- No wasm-bindgen wrapper, no workspace split, no feature flags — WASM phase.
- No JSON Schema file / versioned docs beyond `schema_version` — YAGNI for one
  consumer; revisit when the site consumer lands.
