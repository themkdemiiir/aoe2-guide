# Data Explorer — design spec (2026-07-05)

**Goal:** A free, client-side "ask any slice" explorer over the win-rate data the
site already ships. Lets a visitor freely combine civ × map × elo × patch and pivot
the result into a chart — going beyond the fixed civ/map pages.

**Hard constraint (user, 2026-07-05): $0 marginal cost.** This kills Plan E's
original DuckDB-WASM + R2-Parquet architecture (R2 charges per GET). We don't need
it: the shipped `public/civ-cube.json` (745 KB, 40,264 rows = 53 civs × 9 elos ×
57 maps × 14 patches → games+wins) already answers every aggregate question, and it's
already a free static file on Cloudflare Pages.

## Architecture

- **Page:** `src/pages/[lang]/explorer.astro` → `/en/explorer/`, `/tr/explorer/`.
- **Data:** lazy-`fetch("/civ-cube.json")` **on this page only** (same file the civ/map
  filters already load; cached across the site). No new data artifact, no pipeline
  change, no R2, no WASM.
- **Query = client-side cube sum.** Reuse the exact summing already written for the
  civ slice filter (CivStats) and the tier-list filter (civs/index): pick indices for
  civ/elo/map/patch, sum `games`+`wins` over matching rows, `wr = Σwins/Σgames`.
  Cumulative "X+" elo cuts supported (sum bands ≥ edge), same as the toggle already shipped.
- **Result rendering (ECharts, via `lib/echarts`):**
  - A headline number: win rate + games for the exact slice (with a low-sample honesty note).
  - A **"break down by"** pivot: choose elo / map / patch → chart the win rate for the
    fixed slice across that dimension (bar for elo/map, line for patch — reuse the F2
    civ-chart option builders / honesty conventions: 50% reference, diverging colors).

## UI (guided builder — no SQL)

Controls (all themed `.select-field`):
- **Civilization** (required; "Any" = whole-field average is not meaningful, so require one)
- **Map** — Any / a specific map
- **Elo** — Any / a band / cumulative 1650+ · 1800+ · 2000+
- **Patch** — All-time / a specific patch
- **Break down by** — (none) / Elo / Map / Patch → drives the chart

No free-form SQL: bounded to the cube's dimensions, so it's always instant and always free.

## Scope

**In v1 (free, cube-backed):** win rate + games for any civ × map × elo × patch slice;
the pivot chart across one dimension; low-sample honesty.

**Deferred (needs the 114M-game / 60M-age raw data on R2 → costs money):**
distribution questions (feudal-time histograms, APM spread), per-match / free-SQL
queries. Explicitly out of scope until the user opts into paying for R2.

## Components / isolation

- `src/lib/cube-query.ts` — a small pure module: `loadCube()`, `sliceWinRate(cube, sel)`,
  `breakdown(cube, sel, dim)`. Depends only on the cube shape. Testable in isolation
  (Vitest) — this is the one piece with real logic, so it gets unit tests.
- `explorer.astro` — the page: SSR the controls + a no-JS fallback line; a client script
  wires the controls → `cube-query` → ECharts. Depends on `cube-query` + `lib/echarts`.

## Non-goals

- No new build/VM/pipeline step. No R2. No DuckDB-WASM. No SQL box.
- Bilingual (EN+TR) like every page; English-canonical slug `/explorer`.

## Testing

- Vitest for `cube-query` (slice sum + cumulative + breakdown correctness against a
  tiny fixture cube).
- Gates: `pnpm check` · `pnpm test` · `pnpm build` · screenshot verify EN + a pivot chart.
