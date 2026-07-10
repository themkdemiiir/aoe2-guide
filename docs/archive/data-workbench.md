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

The merged build (aoestats + crawl + summaries) runs from the `scripts/data-pipeline/`
generators (see `lib/buckets.mjs` for the shared elo/map canonicalization);
re-run after refreshing the crawl. Builds in ~30s; the file grows on
`CREATE OR REPLACE` — run `CHECKPOINT;` / rebuild fresh to compact.

## Replay-derived depth — what we have

Two layers of in-game detail, both keyed to `match_id`:

1. **`match_ages`** (historical, from aoestats `replay_summary_raw`): per player
   per age — age-up time, villagers/military/buildings **produced in that age**,
   units/techs. **60M rows across 4.33M matches** (the replay-enhanced subset of
   2022-08→2026-02). Built once via `extract-replay-summaries.py` +
   `build-match-ages.sh` → `match_ages.parquet` → `build-match-ages.sql`. This is
   the max replay-derived data that survives for history (the raw replay files are
   gone — see below).
2. **`replay_events` / `replay_ages`** (forward, from live replay parsing): the
   FULL event timeline (every train/research/build/move, timestamped) for games we
   download + parse while fresh. Same per-age shape as `match_ages`, so history +
   future unify.

## The replay-download path (UNBLOCKED 2026-06-25)

Earlier thought blocked — it wasn't, we used the wrong endpoint. The bare
`matchurls` from `getRecentMatchHistory` are **unsigned** → Azure 403
`PublicAccessNotPermitted`. The working endpoint signs them:

```text
GET .../community/leaderboard/getReplayFiles?matchIDs=%5B<id>%5D&title=age2
→ { replayFiles: [{ profile_id, size, url(SIGNED ?sig=...) }], expiryUnix }
```

- One replay per player; `size == -1` = never uploaded (skip; pick max size>0).
- Rate limit ~120/min, batch ≤10 matchIDs.
- **Retention ~weeks** (probed: ≥18 days downloadable; a 2024 match → `NOT_FOUND`).
  So only RECENT games are fetchable — **history's replays are unrecoverable**
  (aoe2.net, aoestats' old upstream, is sunset; that's why aoestats froze 2026-02).
- **Parser routing:** `aoe2rec` (Rust) parses CURRENT-patch replays; Python `mgz`
  parses only OLD (≤2021) ones — they're complementary, route by era.

## Replay pipeline (`scripts/data-pipeline/`)

- **`replays/`** — Python reference: `getReplayFiles` → download (signed) →
  `aoe2rec` → semantic extract (meta/players/events/ages) → parquet shards, with a
  resumable **SQLite manifest** (every match ends in exactly one status:
  `parsed`/`no_replay`/`expired`/`parse_failed`/`error`). Nothing dropped.
- **`replay-rs/`** — the FAST Rust port (used for the big runs): aoe2rec as a
  **library** (in-process parse, no subprocess) + rayon. ~80 replays/s parse;
  end-to-end ~250 matches/min (download/rate-limit bound). Per-replay panics are
  caught (`catch_unwind`) → `parse_failed`, never crash the run. NDJSON shards are
  gzipped (~27 KB/match; the full ~410k-match recent window ≈ ~10 GiB).
  - `replay-rs seed candidates.csv --db manifest.sqlite` (seed match_ids)
  - `replay-rs run --db manifest.sqlite --out shards --threads 16` (resumable)
  - `vendor/aoe2rec` (the parser clone) is gitignored, not committed.

**Honest ceiling:** millions of raw replay FILES don't exist anywhere anymore
(private blobs + dead aoe2.net). We capture all match RESULTS forward (the crawl)
and full events for the **recent, still-downloadable** window — plus the 4.33M
historical `match_ages` we already own. That is the maximum obtainable.
