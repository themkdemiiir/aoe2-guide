# AOE2 data workbench (analytical DB + replay pipeline scope)

This is the **offline analysis workbench**, separate from the static site. The
site ships precomputed JSON; this DB is for exploring the raw data with SQL.

## The DuckDB database (live)

- File: `~/aoe2-guide/data-cache/aoe2.duckdb` on the VM `mkd@192.168.111.7`.
- Built from: aoestats Parquet archive (`~/aoestats/*.parquet`, 2022-08 → 2026-02)
  **+** the self-collected Relic crawls (`data-cache/relic-patched` 1v1,
  `data-cache/relic-team` team, current to 2026-06), **deduped by `match_id`**
  (aoestats `game_id` == Relic `match_id`; ~308k overlap). aoestats wins on
  overlap (it carries openings + age timings); the crawl adds the recent months.
- **Scale: 115,967,171 player-rows across 32,460,486 unique matches.**

### Web UI

DuckDB's built-in browser UI, on the VM in a detached tmux session:
```
tmux new-session -d -s duckui '~/bin/duckdb ~/aoe2-guide/data-cache/aoe2.duckdb -cmd "CALL start_ui_server();"'
```
It binds `127.0.0.1:4213`. From the desktop, tunnel + open `http://localhost:4213`:
```
ssh -L 4213:localhost:4213 -N -f mkd@192.168.111.7
```
DuckDB is single-writer: while the UI holds the file, query *through the UI* (a
second `duckdb <file>` errors on the lock). For ad-hoc CLI analysis, read the
source files in-memory instead (`read_parquet(...)` / `read_json(...)`).

### Tables

- **`games`** (fact, 116M rows) — one row per player-appearance. Columns:
  `source` (aoestats|crawl), `ladder` (1v1|team|co_1v1|co_team), `match_id`,
  `played_at`, `month`, `map` (clean canonical, e.g. `arabia`), `team_size`,
  `profile_id`, `civ` (slug), `rating`, `elo_bucket`, `won`, `opening`,
  `feudal_t`/`castle_t`/`imperial_t` (age-up time in seconds; aoestats only,
  ~13.9M games), `patch`.
- **`civs`** (dim) — `slug`, `is_guide` (true for the 53 guide civs), `appearances`.
  Non-guide relic civs (e.g. unnamed id 57) are flagged `is_guide=false`.
- **Summaries** (guide civs only, rebuilt from `games`):
  `civ_winrate` (civ×ladder×elo), `map_civ` (best civs per map),
  `civ_opening` (opening mix + win rate), `civ_timing` (avg feudal time +
  win-rate-vs-speed).

### Rebuild

`scripts/data-pipeline/build-duckdb.sql` (crawl-only) is in the repo. The full
merged build (aoestats + crawl + summaries) is the SQL run from this session;
re-run after refreshing the crawl. Builds in ~30s; the file grows on
`CREATE OR REPLACE` — run `CHECKPOINT;` / rebuild fresh to compact.

## What in-game events we have — and don't

The 116M rows are **match results** + what aoestats extracted from replays:
**opening** + **age-up times** (~13.9M 1v1 games) + duration. That's it for
in-game events.

Full event timelines (build order, every unit, fights, resource milestones)
live **only inside the replay file** (`.aoe2record`) and are **not recoverable
for history** — Relic expires replay download links after ~weeks, so 2022-2025
replays are gone. More VMs/RAM don't help; the bottleneck is replay availability,
not compute (one VM crunches 116M rows in seconds). Earliest data is 2022-08
(aoestats' start), not DE launch (2019-11).

## Replay-parsing pipeline — scope (going-forward full events)

The feasible version of "time every important event": parse replays of **new**
games while their links are still fresh, accumulating full-event data forward.

**Phase 0 — feasibility PoC (make-or-break):** can we *programmatically download*
a recent ranked replay given a `match_id`/`profile_id`, and parse it? aoestats
clearly parsed replays, so a path exists, but the Relic replay-download endpoint
is undocumented — **verify this first**; if replays aren't fetchable, the project
stops here. Parse with the **`mgz`** Python library (standard AoE2 record parser).

**Phase 1 — pipeline:** crawl new ranked match_ids (extend `collect-relic`) →
download each replay within the freshness window → `mgz` parse → extract events
(age-ups, first TC, first military, key buildings, resign/win time, APM, …) →
store as `events(match_id, profile_id, event_type, t_seconds, …)` in DuckDB/parquet,
keyed to `games`. Run on a **dedicated 2nd VM** (download + parse is CPU/I/O
heavy; this is where extra RAM/cores earn their keep).

**Phase 2 — analysis:** aggregate the event timeline (e.g. real age-up curves,
build-order frequencies, timing-vs-winrate) and optionally surface a slice on the
site as precomputed JSON.

**Risks:** replay endpoint availability/rate limits (Phase 0); replay expiry
window (fetch within days); not all games are recorded; sustained volume
(tens of thousands of ranked games/day) must fit the 2nd VM's throughput.
