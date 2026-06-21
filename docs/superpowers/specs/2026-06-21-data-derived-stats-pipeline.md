# Data-Derived Stats Pipeline — Design Blueprint

**Date:** 2026-06-21
**Status:** Design (web-verified source research). Not yet implemented.
**Context:** Make civ meta / matchups / map-civ stats fully source-derived from real
ladder data, replacing the hardcoded `sync-meta-stats.mjs` array and hand-curated map
meta. Build-order frequency from replays is a documented gap (no API serves replays).

## Verified source reality (2026-06-21)

| Source | Status | Provides | Catch |
|---|---|---|---|
| aoestats.io parquet dumps | FROZEN | Sep 2022 -> Feb 2026 historical (civ/map/winner/elo per match) | Raw dumps empty since 2026-02-08; last non-empty 2026-02-01->02-07 (118,661 matches). Web aggregates current but not downloadable as raw. |
| World's Edge Link API (`aoe-api.worldsedgelink.com`) | LIVE | Leaderboard + getRecentMatchHistory (civ_id, map, winner, elo); lastmatch 2026-06-17 | Host moved off dead reliclink.com. NO replay download (matchurls empty). Reverse-engineered, no SLA. |
| mgz replay parser (PyPI `mgz`) | LIVE | Action-level build orders + age-up timings | Needs .aoe2record files no API serves -> Phase 5 stretch. |

I have confirmed the load-bearing facts. The preservation logic at line 402-403 confirms: `build-civilizations.mjs` writes `patch` + the rebuilt `civEntries` array, carrying forward only `strategy` prose per-civ — any `meta` block is wiped on regen. No `civ-meta`/`matchups`/`map-civ-stats`/`build-meta` files exist yet. `sync-meta-stats.mjs` is the hardcoded scrape, wired as `pnpm sync:meta`. This grounds the blueprint.

---

# DATA-DERIVED AOE2 GUIDE — UNIFIED BLUEPRINT

## 1. The verdict on data acquisition

**There is no single clean live source in mid-2026. There are two viable sources, and you need BOTH — one for depth (frozen), one for freshness (live but raw).**

| Source | Status (verified 2026-06-21) | Gives you | The catch |
|---|---|---|---|
| **aoestats.io Parquet dumps** | UP but **FROZEN**. Last non-empty week `2026-02-01→02-07` (118,661 matches). Every dump from 02-08 is `num_matches:0`. | Pre-aggregated, clean, civ+map+winner+elo+ladder per row. ~Sep 2022 → Feb 2026 historical archive downloadable. | Stops at Feb 2026. Patch-162286 era only. No replays. No action-level build orders. |
| **World's Edge Link API** (`aoe-api.worldsedgelink.com`) | **LIVE**, HTTP 200, lastmatch 2026-06-17. | `getLeaderBoard2` → profile_ids → `getRecentMatchHistory`: per-match `civilization_id`, `mapid`, `resulttype`, team ratings, timestamps. | Reverse-engineered, no SLA, can break/block. Civs are numeric IDs. **No replay download** (`matchurls` empty). Per-match polling — you assemble the corpus yourself. |
| **mgz replay parser** | LIVE (PyPI 1.8.51). | Action-level BUILD/RESEARCH/QUEUE log → real openings + age-up times. | Needs `.aoe2record` files **no API serves**. Only viable on locally-acquired replays. **De-scope for v1.** |

**The decisive call:**

- **aoestats historical archive is your bootstrap corpus.** Download the full Sep-2022→Feb-2026 weekly Parquet set once. This is clean, large, and gives you defensible patch-162286-era civ/map/matchup rates immediately. Snapshot-date it honestly as "frozen, 2026-02-07."
- **World's Edge Link is your freshness engine.** Crawl the 1v1 RM leaderboard (id=3) for active profile_ids, pull `getRecentMatchHistory` per player, dedupe by `match_id`. This is the ONLY path to post-Feb-2026 data. It yields civ + map + winner + elo — everything the meta/matchup/map surfaces need. It does **not** yield build orders.
- **Build-order frequency / age-up timings stay source-derived from Hera transcripts** (as they are today) until you have a local replay corpus. mgz is a Phase-4 stretch, not a v1 dependency. Do not fake-derive openings from data you cannot acquire.

**Realistic effort on the 64GB/20-core box:**

- **First full aoestats ingest:** ~180 weekly dumps × 2 Parquet files. Download is I/O-bound, ~1–3 GB total compressed, **30–90 min** depending on aoestats bandwidth. DuckDB aggregation over the full corpus (civ/map/matchup/elo slices): **minutes**, not hours — DuckDB reads Parquet natively, 20 cores is overkill but pleasant. Total first run: **half a day** including writing the scripts.
- **Self-collect bootstrap (World's Edge):** to match aoestats sample sizes you want ~100k+ recent matches. At polite throttling (say 2–4 req/s, ~50 matches/player, heavy dedup), seeding from the top ~20k ranked profiles yields a usable corpus in **a few hours to a day** of background crawling. Run it `run_in_background` / cron on the Proxmox box, not interactively.
- **Incremental monthly refresh:** re-crawl `getRecentMatchHistory` for active profiles since last snapshot, append new match rows to the Parquet store, re-run DuckDB aggregation, regenerate derived JSON. **15–40 min wall-clock**, fully automatable as a monthly cron. aoestats re-poll is a cheap `GET /api/db_dumps` check in case it ever thaws.

**Honest bottom line:** civ tiers, win/play rates, matchups, and per-map civ strength are all **achievable and genuinely source-derived** via aoestats-historical + self-collected-Relic. Build-order *frequency from real matches* is **not cleanly achievable** without a local replay pipeline — keep those Hera-sourced and labeled as such.

---

## 2. The unified pipeline architecture

One pipeline, five stages, on the Proxmox box. Everything snapshot-dated, source-attributed, fail-loud, and **stored outside `civilizations.json`** so civ regen never wipes it.

```
COLLECT ──► PARSE/NORMALIZE ──► STORE (Parquet) ──► AGGREGATE (DuckDB) ──► DERIVE JSON ──► MERGE
```

### Stage 1 — COLLECT (`scripts/data-pipeline/collect-*.mjs`)
- `collect-aoestats.mjs` — `GET /api/db_dumps`, download every non-empty weekly Parquet to `data-cache/aoestats/raw/<date_range>/{matches,players}.parquet`. **Skips `num_matches:0` dumps loudly** (logs the freeze, never writes zeros). Idempotent via checksum match.
- `collect-relic.mjs` — crawl `getLeaderBoard2?leaderboard_id=3` for profile_ids → `getRecentMatchHistory` per player. Self-throttle, cache responses, dedupe by `match_id`. Append normalized rows to `data-cache/relic/raw/<snapshot>/matches.parquet`. Resumable (checkpoint last-crawled profile).

### Stage 2 — PARSE / NORMALIZE (`scripts/data-pipeline/lib/normalize.mjs`)
- Map numeric `civilization_id` → slug via a new `src/data/civ-id-map.json` (derive from existing icon/civ data + aoctechtree). **Fail-loud on any unknown id** — no silent drop, no default.
- Unify both sources into ONE row schema: `{match_id, civ_slug, map_slug, winner, elo, elo_bucket, ladder, game_type, patch_era, source, snapshot_week}`.
- `source` column distinguishes `aoestats-frozen` vs `relic-live` so provenance survives into aggregation.

### Stage 3 — STORE (Parquet lake)
- `data-cache/lake/players/*.parquet` — partitioned by `snapshot_week`. This is the durable analytical store, **gitignored** (regenerable, large).
- mgz replay store (`data-cache/replays/`) reserved for Phase 4, unused in v1.

### Stage 4 — AGGREGATE (`scripts/data-pipeline/aggregate.mjs`, DuckDB)
One DuckDB process, four query families, all reading `lake/players/*.parquet`:
- **civ meta:** `civ → {games, win_rate, play_rate}` sliced by `elo_bucket`, `patch_era`. Wilson 95% CI. `games >= 400` or → `NR`.
- **matchups:** self-join on `match_id`, ordered pairs `{civA, civB} → {games, win_rate, ci95}`. `games >= 400` per pair or → `null`. No symmetry assumption.
- **map-civ:** `(map, civ) → {games, win_rate, play_rate}` sliced by elo. `games >= 200`; flag `200–500` as `lowConfidence`.
- **(Phase 4) build-meta:** openings + age-up p25/p50/p75 from mgz replay actions. `n >= 200`.

### Stage 5 — DERIVE JSON + MERGE (`scripts/data-pipeline/emit-*.mjs`)
Writes four **standalone** language-agnostic data files (the durability decision — none of these live in `civilizations.json`):

| New file | Feeds | Replaces |
|---|---|---|
| `src/data/civ-meta.json` | CivCard, civs pages, index, builds | hardcoded `meta` from sync-meta-stats |
| `src/data/matchups.json` | matchups/index.astro (currently data-less) | nothing — net-new, kills the empty page |
| `src/data/map-civ-stats.json` | map pages, CivCard map context | hand-curated `recommendedCivs`/`teamComps.civs` |
| `src/data/build-meta.json` (Phase 4) | builds/[build] | nothing — augments Hera data |

Each file carries a header: `{snapshot, source, patch, ladder, eloBucket, minGames, tierMethod, ...}`.

**The carryover fix (the critical durability hole):** Because `build-civilizations.mjs:402-403` rewrites `civs[]` from scratch carrying only `patch` + per-civ `strategy`, **meta must never live inside `civilizations.json`.** It lives in `civ-meta.json`, and pages read both collections at render time keyed by slug. This is strictly better than the old "merge then get wiped" pattern — there is nothing to wipe. No edit to `build-civilizations.mjs` is even required if consumers read `civ-meta.json` directly; if you prefer a merged shape, add a **post-build splice step** (`emit-civ-meta.mjs` runs AFTER `build:civilizations`), never inside it.

**Pipeline orchestration** — new `package.json` scripts:
```
"data:collect"   → collect-aoestats + collect-relic
"data:aggregate" → aggregate.mjs (DuckDB)
"data:emit"      → emit-civ-meta + emit-matchups + emit-map-civ-stats
"data:refresh"   → collect → aggregate → emit (the monthly cron target)
```
`validate-data.mjs` extended: **fail-loud** if any derived file is missing `snapshot`/`source`, has `winRate` without `games`, or `snapshot` older than 120 days (stale-source banner trigger). Wired into `prebuild` so CI fails on stale/unsourced data.

---

## 3. What it removes (the kills)

- **`scripts/sync-meta-stats.mjs` — DELETED entirely.** The 49-civ hardcoded `AOESTATS_DATA` array (frozen, missing Khitans/Jurchens/RoR civs, naïve win-rate→letter tiers) dies. `pnpm sync:meta` is repointed to `data:emit` or removed.
- **Hand-curated map meta — REPLACED.** `recommendedCivs` and `teamComps.{2v2,4v4}.{flank,pocket}.civs` arrays across all 13 `src/content/maps/*.yaml` become build-generated top-N-by-winRate from `map-civ-stats.json`. The editorial prose `teamComps.*.strategy.{en,tr}` **stays** (it's genuine human strategy, not a fabricated stat).
- **The data-less matchups page — FILLED.** `matchups/index.astro` gets real `pairs[a][b]` data for the first time.
- **Naïve tier cuts — REPLACED** by sample-gated, CI-adjusted tiers with `NR` for thin slices.

**Explicitly NOT removed (honesty boundary):**
- `src/data/counters.json` — unit-vs-unit *mechanics*, a different surface; not a ladder stat. Leave it.
- Build-order `time` fields and `civsRecommended` — stay **Hera-sourced** until a real replay corpus exists. Do not fake-derive openings from data you can't acquire.

---

## 4. Phased build plan (highest value / lowest risk → last)

**Phase 0 — Pipeline skeleton + civ-id map (½ day).** `scripts/data-pipeline/` dir, `civ-id-map.json` (fail-loud on unknown id), `normalize.mjs`, DuckDB dep on the Proxmox box. Lowest risk, unblocks everything.

**Phase 1 — Civ meta from frozen aoestats (highest value, lowest risk).** `collect-aoestats.mjs` + `aggregate.mjs` (civ slice) + `emit-civ-meta.mjs` → `civ-meta.json`. Repoint CivCard/civs pages to read it. Add `NR`/"Insufficient data" badge + snapshot banner. **Delete `sync-meta-stats.mjs`.** Ships real, sourced tiers day one off the clean historical corpus. No live-API risk.

**Phase 2 — Matchups (fills the empty page).** Add the self-join query + `emit-matchups.mjs` → `matchups.json`. Wire `matchups/index.astro`. Same frozen corpus, no new collection risk. High user-visible value.

**Phase 3 — Map-civ stats (replaces hand-curated meta).** Map query + `emit-map-civ-stats.mjs` → `map-civ-stats.json`. Build-generate `recommendedCivs`/`teamComps.civs`. Keep prose. Medium effort (role/flank-pocket heuristic), still off frozen corpus.

**Phase 4 — Freshness engine (highest risk, do last).** `collect-relic.mjs` World's Edge crawler → appends live rows to the lake → re-aggregate. This is where the reverse-engineered, no-SLA risk lives. Run as background/cron. Snapshot label flips from "frozen Feb 2026" to "self-collected, <date>" once the live corpus crosses sample thresholds.

**Phase 5 — (stretch) Replay/build-order intelligence.** Only if you build a local `.aoe2record` acquisition path. mgz → `build-meta.json` openings + age-up p50. Until then, builds stay Hera-sourced. Genuinely optional.

---

## 5. Honest caveats

- **Sample-size thresholds are non-negotiable:** civ/matchup `games >= 400`, map-civ `games >= 200`, build `n >= 200`. Below threshold → emit `null`/`NR`, never a point estimate. Wilson 95% CI on every rate. This is the core fail-loud guarantee against "53% off 12 games."
- **Dead-source risk is real and dual:** aoestats is already frozen (Feb 2026 cutoff — Phases 1–3 ship a *snapshot*, labeled as such, not "current meta"). World's Edge can break/block without notice (Phase 4 has no SLA). The architecture survives both: frozen corpus always renders with an honest date; if Relic dies, you fall back to the frozen snapshot, never to fabrication.
- **Patch staleness:** everything pre-Phase-4 is patch-162286 era. If a balance patch lands after Feb 2026, the frozen rates are *wrong-but-honest*. The 120-day staleness check in `validate-data.mjs` forces a visible "stats stale since 2026-02-07" banner rather than silent rot.
- **Build orders are the honest gap:** real-match opening frequency and age-up timings require replays you cannot download via any API. Until a local replay pipeline exists, builds remain Hera-transcript-sourced and **labeled** `source.author: Hera` — not dressed up as match-frequency data.
- **Graceful degradation everywhere:** missing pair → existing `t('matchups.noMeta')` string. Thin map-civ slice → omitted, "Insufficient data." `lowConfidence` (200–500 games) → explicit caveat badge. One-sided matchup corpus → show the available direction, blank the inverse (no symmetry assumption). Missing derived file or absent `snapshot`/`source` → build **fails**, no defaults shipped.

**Files created:** `src/data/civ-meta.json`, `src/data/matchups.json`, `src/data/map-civ-stats.json`, `src/data/civ-id-map.json`, `src/data/build-meta.json` (Phase 5); `scripts/data-pipeline/{collect-aoestats,collect-relic,aggregate,emit-civ-meta,emit-matchups,emit-map-civ-stats}.mjs`, `scripts/data-pipeline/lib/normalize.mjs`.
**Files modified:** `scripts/validate-data.mjs` (provenance + staleness fail-loud), `package.json` (`data:*` scripts, drop `sync:meta`), `src/components/content/CivCard.astro` (NR/snapshot guard), `src/pages/[lang]/matchups/index.astro`, all 13 `src/content/maps/*.yaml` (civ arrays become build-generated).
**Files deleted:** `scripts/sync-meta-stats.mjs`.
**Key invariant:** all derived stats live in standalone `src/data/*.json`, NEVER inside `civilizations.json` — `build-civilizations.mjs:402-403` rewrites that file from scratch, so anything merged in is wiped. Standalone files + read-both-at-render is the durable pattern; no edit to the civ builder required.
