# AOE2 Guide — Session Handoff

_Last updated: 2026-07-04 (late). Everything below is either **live on aoe2guide.com** or **backlog** for the next session. Read the "Start here" box, then jump to whichever section you're picking up._

> **Start here (next session):** the highest-value unfinished work is now clearly the **analyzer winner-benchmarks** (§B1) — it's what makes the "Winners —" rows and eco-upgrade timings show real numbers, and the user asked for it directly. Everything needed to build it is in §B1 + §C, and the new archive backfill (§B2, now SHIPPED) feeds it more replay data. The map-civ redesign and the aoe.ms backfill both shipped this session (deploy #10, commit `bf1b983`).
>
> **User action still pending:** the VM disk expansion (§B4) — user said they'll run it at home. The backfill is disk-guarded (skips below 20 GB free) so nothing is at risk, but it wants the headroom. Exact commands are in §B4.

---

## 1. What's live now (shipped this arc)

Six deploys over the last few days, all verified on the production domain:

| Area | What changed | Commit(s) |
|---|---|---|
| **Patch axis** | Stats now labelled by real game builds ("Update 179158") instead of month dates, across civs/maps/builds pages. | `5aa2dbf` |
| **Patch auditor** | Every 3-hour VM sweep cross-checks replay-file build numbers against the gamemod→build map and fails loud on a new update. | `a0ec5f8` |
| **SEO / indexing** | Fixed every repo-side cause behind Google Search Console's "not indexed" buckets: trailing-slash links (was 24,873 redirecting), `_redirects` for removed URLs, noindex 404, unique per-page meta descriptions (was 395 duplicates), dropped dead HowTo schema, added Article schema. | `4682e73`, `f54d914` |
| **Build pages redesign** | Compact two-axis table (villager rail + per-resource split columns + checkpoints strip). Splits derived + adversarially verified for 17/43 builds; the rest ship without columns rather than guessed numbers. | `f54d914` |
| **Full-site review** | Dark mode fixed site-wide; analyzer comprehension (badge rename, "~N median" legend, doubled-label fix); 34 source-verified build-order corrections; honest idle-TC + complete-APM metrics; 20+ TR i18n gaps; empty-state panels; a11y. | `bd5b910`, `eb6c747`, `fc8c8f1` |
| **Map civ leaderboard** | "Best civs on this map" rebuilt as a diverging ECharts bar leaderboard anchored on the 50% line (gold above / red below), civ-emblem axis labels, hover tooltip, dataZoom slider to scroll all civs. Tree-shaken ECharts (~181 KB gz, map pages only), dark-mode via token re-read, EN/TR, SSR list kept as no-JS + table-view twin. Fixed 2 review bugs: empty-but-present ladder no longer shows a dead toggle/empty view (lombardia etc.); cube-fetch failure falls back to all-time without mislabeling. | `bf1b983` |
| **Archive replay backfill** | Age archive (`api.ageofempires.com`, what aoe.ms 301s to) added as a replay-rs fallback for matches aged out of getReplayFiles — serves replays YEARS deep (parsed a Nov-2024 + Jan-2026 replay e2e). Manifest `profile_ids` column + migration; 30s-paced, per-run-capped, 429-aware; `backfill-seed.sh` samples one aoestats week per sweep, disk/backlog-guarded. Wired into `sweep.sh`. | `bf1b983` |

### The build-order correctness win (worth knowing)
Several build **intros were stating fabricated civ bonuses** — e.g. "Vikings free Bloodlines" (Vikings have no Bloodlines), "Franks Chivalry extra HP", "Khmer free housing", "Malay cheaper age-up". All were corrected against the techtree-derived civ YAMLs (the ground truth). A new gate in `scripts/verify-build-facts.mjs` now blocks deploys on villager-count/timing/phase inconsistencies.

### The analyzer metric win
The user doubted the idle-TC number — correctly. It was counting the unavoidable Feudal-Age research (~130s) and Loom as "idle". Now it credits Town-Center research as busy time; on the test replay feudal idle dropped **256s → 126s**. APM was also dropping ~20% of commands onto a phantom player; now fixed (35.5 → 42.9 on the test replay).

---

## 2. Backlog — build next (prioritized, actionable)

### B1. Analyzer winner-benchmarks — the "Winners —" rows and eco timings ⭐ TOP PRIORITY
**Why:** the user asked twice ("Eco upgrades must show winners timings", "what does — mean on these rows"). Right now the head-to-head matrix prints an honest but confusing "Winners —" on several rows because we never computed a reference for them.

**What to build (on the VM):** extend the winner-benchmark generators (`scripts/data-pipeline/build-winner-refs.*`, `build-benchmark-vils.*`) with, computed as medians among **ranked-1v1 winners bucketed by elo** from the replay corpus:
- `first_military_ms`, `apm`, `eco_apm`, `mil_apm` (per-metric winner medians) → fills the matrix "Winners —" rows.
- Median research times for Loom / Double-Bit Axe / Wheelbarrow / Horse Collar (from replay command timestamps) → fills the "Eco upgrades" row with "Winners: Loom ~4:10 (93%)".

**Then** wire the new fields into `src/pages/[lang]/analyzer.astro` (the matrix rows + eco-upgrade row). Data-honesty rules unchanged: exact slices get colored deltas, estimates get `~`, and where a reference genuinely can't be computed **omit the winners segment** rather than print a dash.

**Pipeline how-to is in §C.** The replay data has `first_military_ms`, `apm`, `eco_apm`, `mil_apm` per player already (`PlayerMetrics`), plus per-tech research times.

### B2. aoe.ms replay backfill — ✅ SHIPPED 2026-07-04 (`bf1b983`)
**Done.** `aoe.ms/replay/` is a bare **301** to `api.ageofempires.com/api/GameStats/AgeII/GetMatchReplay/?gameId=..&profileId=..&matchId=..` — hit the API directly. Returns a **ZIP** (one `AgeIIDE_Replay_<id>.aoe2record`), NOT gzip. Probed 2026-07-04 across 2022→2026: ~⅓ hit rate, 404s are **per-match** (both 1v1 participants agree), harsh 429 wall — safe at **30s spacing**. Critically, **aoe2rec parses the old replays** (e2e: Nov-2024 → 1721 events, Jan-2026 → 4947).

Implementation: `replay-rs` manifest gained `profile_ids` (migration on open); seed CSV carries participant ids; getReplayFiles-misses with ids drain through a serial, 30s-paced fallback (`--archive-limit`, default 60/run; 429→retryable, 404-all→expired, bad-zip→parse_failed terminal). `backfill-seed.sh` walks aoestats weeks backwards from a cursor, samples 2500 RM matches/week, guards ≥20 GB free + <3000 backlog. Both wired into `sweep.sh`. **It's a sampler, not a mirror** (~500 paced req/day vs ~230k RM matches/week). Full endpoint + design notes in the `project_archive_backfill` memory.

**Follow-ups if you want more from it:** raise `SAMPLE_PER_WEEK`/`--archive-limit` once the disk is expanded; the extra replay coverage directly improves B1's winner medians and pushes the replay-verified map slices further back than the ~June-2026 window.

### B3. Clean-code refactor backlog (safe, non-urgent polish)
All verified net-positive and behavior-preserving by the review; just not shipped:
- Consolidate the title-casers (`civName` in `builds/[build].astro`, `titleCase` in `BuildOrderSteps.astro`) into the existing `src/lib/display.ts` `displayName`.
- Extract the `guideCivs` loader (copy-pasted in ~10 pipeline generators) into `scripts/data-pipeline/lib/guide-civs.mjs`.
- Pipeline dedup: `SOURCE_DATE` → `AOESTATS_END_MONTH` (buckets.mjs), `monthKey` → patch-axis.mjs, inline `pct` → stats.mjs.
- `pub` → `pub(crate)` on the six `replay-rs` analyze submodules (they have no external callers).

### B4. VM disk expansion ⚠️ USER ACTION (user will run at home)
`data-cache/replays/shards/events.ndjson.gz` is **9.7 GB** and is the **only copy** of parsed replay data (source replays expire upstream in weeks). It sits on a root LV that's **~69% full (65 G of 98 G)**. The disk expansion is **still unapplied**: `sda3` is 498 G but the LV is only 100 G, and `sdb` (800 G) is entirely unpartitioned. The new backfill is disk-guarded (skips below 20 GB free, currently 29 GB) so nothing is at risk, but it wants the headroom. Exact paste-ready commands (need sudo on VM 100, `ssh mkd@192.168.111.7`):

```bash
sudo pvresize /dev/sda3                              # grow PV into the 498G partition (~398G freed)
sudo lvextend -l +100%FREE /dev/ubuntu-vg/ubuntu-lv  # grow LV into the freed extents
sudo resize2fs /dev/ubuntu-vg/ubuntu-lv              # grow ext4 online (no unmount, safe)
df -h /                                               # expect ~490G total
```

Takes root 98 G → ~490 G. `sdb` (800 G) can be added as a second PV later for more. See the `project_infra` memory.

---

## 3. Flagged — need your decision (small, not started)

- **Analyzer "Top Fixes" text is English on TR pages.** The WASM engine emits English fix strings; the page prints them verbatim. Fix = have the engine emit stable fix-codes and map code→localized string in `ui.ts`. Medium effort.
- **Mapuche page contradicts itself** — the hero says "Cavalry and Counter-units civilization", the article body says "Infantry civilization". One is wrong; needs checking against the aoe2techtree source (don't guess).
- **5 builds have villager counts that don't reconcile even in Hera's own notes** — e.g. `28pop-turks-fast-imp` (Feudal note totals 29 but says 30 villagers), `30pop-fast-imp-generic` (split sums 29 vs 34 stated). The sources themselves are ambiguous, so these were left as-is rather than guessed. If you can check Hera's videos, that resolves them.

---

## 4. Key technical context (so the next session doesn't relearn it)

### VM / data pipeline
- SSH: `export SSH_AUTH_SOCK=/run/user/1000/ssh-agent.socket` then `ssh mkd@192.168.111.7`. Repo clone at `~/aoe2-guide`; DuckDB at `~/bin/duckdb` (v1.5.4); archive parquet in `~/aoestats`.
- **Edit generator locally → `rsync` to VM → run there → `rsync`/`scp` the output JSON back.** Write SQL to a file and `scp` it (nested quoting in `ssh '...'` breaks on double-quoted strings). Set `SET temp_directory=...` for spills.
- A 3-hour cron runs `sweep.sh` (stream crawl → ingest → replay parse → **`check-patch-axis.mjs`** auditor). It only captures data; the aggregate/refresh/build-winner scripts are run **manually** after.
- The DuckDB write lock is held by a tmux `duckui` session — use `~/bin/duckdb -readonly` for audits; don't kill the session.

### Two civ-id spaces (never conflate — this was a real production bug)
- `src/data/civ-id-map.json` = **game/replay** `civ_id` (analyzer + replay data).
- `src/data/relic-civ-id-map.json` = **Relic API** `civilization_id` (every crawl consumer, via `scripts/data-pipeline/lib/relic-map.mjs`). Era-scoped; re-shuffles at DLC boundaries.

### replay-rs (the analyzer engine)
- `scripts/data-pipeline/replay-rs/` — pure `analyze/` module (metrics/walk/compare/model) + IO behind a `cli` feature. `replay-wasm/` wraps it for the browser.
- **`vendor/aoe2rec/` is gitignored (clone-on-build).** Do NOT put patches there — they vanish on re-clone. The APM `pid()` exhaustive match lives in tracked `src/analyze/walk.rs`; regenerate its 59-arm list if the vendored enum changes.
- Rebuild the browser engine with `pnpm build:wasm` (needs `wasm-pack` + `rustup target add wasm32-unknown-unknown`). Output committed to `src/wasm/pkg/` (glue) + `public/analyzer/pkg/` (binary).
- Test replay for e2e: `/tmp/claude-1000/.../scratchpad/test-replay.aoe2record` (re-download if stale).

### Frontend gotchas
- Under `<ClientRouter />` (view transitions), scripts run **once per session**. Interactive `is:inline` scripts need `data-astro-rerun`; bundled scripts that import (e.g. Chart.js) must wrap init in an `astro:page-load` listener (a `data-astro-rerun` attribute forces them to `is:inline`, breaking imports).
- Data-viz colors that must swap in dark mode: use the CSS tokens `--color-wr-good` / `--color-wr-bad` / `--color-fav` in inline styles (browser resolves `var()` live). Chart.js JS values need `getComputedStyle` + a `data-theme` MutationObserver. Shared brand hues live in `src/lib/palette.ts`.
- `is:inline` scripts can't import, so `esc()`/`tierOf()` are hand-mirrored (with "// mirror of…" comments) in a few pages. Kept intentionally.
- **Turkish system locale breaks `sed [a-z]` ranges** (excludes dotless-i). Prefix `LC_ALL=C` for any regex ranges in Bash on this machine.

### Gates (all must be green before pushing to main — deploy fires on push)
`pnpm check` (astro/TS) · `pnpm test` (365 vitest) · `pnpm build` (runs prebuild: `validate-icons`, **`verify-build-facts`** incl. the new structural gates, `validate-data`, `validate-stats`, `audit-yaml-translations`, `build-og`). Plus `pnpm exec biome ci .` and `cargo test` (in replay-rs) when Rust changed. Any new user-visible EN string needs a real TR twin or `audit-yaml-translations` fails the build.

_Known pre-existing noise:_ `knip` doesn't scan `scripts/data-pipeline` as entries, so it false-flags `AOESTATS_END_MONTH` + ~32 files. Not real dead code; don't chase it.

### Still your-side-only (dashboard, can't be done from the repo)
From the SEO work: add a Cloudflare redirect rule **`www.aoe2guide.com/* → https://aoe2guide.com/$1` (301)** — `www` currently serves a full duplicate of the site and is the main driver of GSC's "alternative canonical" bucket. Then in Search Console hit **Validate fix** on the "404" and "redirect" rows.

---

## 5. Quick verification recipes

```bash
# Full local gate
pnpm check && pnpm test && pnpm build && pnpm exec biome ci .

# Rust engine
cd scripts/data-pipeline/replay-rs && cargo test --offline
cargo run --offline -- analyze <replay.aoe2record> --json   # trace metrics

# Rebuild browser engine after a Rust change
pnpm build:wasm

# Headless screenshot check (Playwright is a devDep; scripts must live under node_modules/ for resolution)
pnpm preview --port 4327 &
# write node_modules/.shot.mjs using: import { chromium } from "playwright"
# dark mode: fresh context + addInitScript(() => localStorage.setItem("theme","dark")) BEFORE goto
```
