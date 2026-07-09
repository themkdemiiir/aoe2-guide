# Matchups page — data audit + fixes (2026-07-08)

Plan for the civ-vs-civ matchups page (`src/pages/[lang]/matchups/index.astro`). Triggered by:
britons-vs-franks at **+1650 elo shows "az veri" (too few) despite 81,952 total matches**, and a
request for a proper **civ × elo × map** breakdown with correct data.

## TL;DR

The **underlying data is correct** — the exported matchup JSON matches the live DB almost exactly, so
there is **no pipeline/export bug**. The problems are all in the **page's presentation layer**: an elo
bug that hides high-elo data, a map section that only renders at "all elos", and a headline number
that's silently dominated by low-elo games. All fixes are site-side (Astro); no re-export needed.

## Audit evidence (source of truth = live Postgres, 2026-07-08)

### The data is faithful
britons(1) vs franks(2) 1v1, games by the britons player's elo bucket — **DB vs exported JSON**:

| bucket | DB games | export games | britons WR (DB) |
|---|---|---|---|
| `<1000` | 54,744 | 54,749 | **43.0%** |
| 1000–1199 | 16,311 | 16,313 | 47.3% |
| 1200–1399 | 6,611 | 6,612 | 45.3% |
| 1400–1649 | 3,099 | 3,099 | 45.4% |
| 1650–1799 | 581 | 581 | 47.3% |
| 1800–1999 | 357 | 357 | 49.0% |
| 2000–2199 | 117 | 117 | 53.8% |
| 2200–2499 | 98 | 98 | 38.8% |
| 2500+ | 26 | — (dropped: < minGames 30) | 53.8% |

(Tiny count diffs = export snapshot timing; effectively identical. `2500+` correctly dropped below the
`minGames=30` threshold.)

### The `<1000` dominance is REAL, not corrupt
Overall `match_players.elo_bucket` distribution across **108M** rows: `<1000` = **39.6%**, then
28.3% / 18.6% / 10.1% / 1.9% / … — a **smooth curve**, and a raw sub-1000 histogram ramps cleanly
(900-band 12.3M → 100-band 35K) with **no sentinel spike**. So `<1000` is a genuine large low-elo
ranked population. britons/franks skew even lower (67% `<1000`) plausibly because both are
beginner-favorite civs.

### The real problem
**The "all elos" headline (44%) is dragged down by the huge `<1000` sample (54.7K games @ 43%).** At
competitive elo it's 47–54% — a materially different matchup — and the **+1650 view that would show
this is exactly what the bug breaks.** Correct data, wrong presentation.

## The 3 fixes (all in `src/pages/[lang]/matchups/index.astro`)

### Fix 1 — Elo dimension: stop dropping per-bucket game counts (the "az veri" bug)
**Cause (confirmed):** building the client payload (~line 114), non-"all" elo buckets keep **only the
win rate**, dropping games:
```js
enc[i] = i === 0 ? [Math.round(v[0]*10)/10, v[1]] : Math.round(v[0]*10)/10;  // ← non-all loses v[1]
```
So the client's cumulative "+1650" summation (~lines 467–475) reads `bg = Array.isArray(rec) ? rec[1] : 0`
→ `bg = 0` for every band → `g` stays 0 → falls back to all-elos with *"too few at this bracket"* —
even though +1650 has **1,153 games**.

**Change:** store `[winRate, games]` for **all** buckets:
```js
enc[i] = [Math.round(v[0]*10)/10, v[1]];
```
Then the cumulative and single-bucket paths both find real game counts and show the sliced number.
**Cost:** the inline payload grows by ~1 int per (pair × bucket) — modest; verify the built page size.
The `< 150 games → low-sample` note (lines 277/506) then works correctly per bracket.

### Fix 2 — Map dimension: show "By map" at every elo bracket + add game counts
**Cause:** `perMapHtml` only renders at "all elos" (~line 515: `cut == null && +eloVal === 0`), so it
vanishes the moment you pick a bracket. And `pm` (~line 145) stores only `Math.round(m.winRate)` — no
per-map game count, so it can't be a true *distribution*.

**Change:**
- Render the map section regardless of the elo selection (drop the `+eloVal === 0` gate; keep the
  SSR default in sync, ~line 161+).
- Store `[winRate, games]` per map (mirror Fix 1) so the section can show **games-per-map** (the
  distribution) alongside the win rate, sorted by play count.
- Note: `civ-matchups-by-map.json` is per-map winrate only (not per elo); showing it under any elo
  bracket means "map split, all elos" — label it so it isn't mistaken for elo-filtered. (A true
  elo×map cube would need a new export; out of scope unless wanted.)

### Fix 3 — Don't present a low-elo-dominated number as "the matchup"
The headline is honest but misleading given 40–67% of the sample is `<1000`. Options (pick one):
- **(a)** Surface the **elo win-rate curve** up front (a tiny sparkline / the per-bucket table), so the
  43%→54% swing is visible, not buried behind a dropdown.
- **(b)** Default the bracket to a **competitive floor** (e.g. 1400+) with an obvious "all elos" toggle.
- **(c)** Keep "all elos" default but add a one-line caption ("headline includes all elos; skews low").

Recommendation: **(a)** — most informative, source-derived, no arbitrary cutoff.

## What is NOT changing
- No pipeline/dbt/export change — the data is verified correct.
- `minGames = 30` threshold stays (it's reasonable; `2500+` genuinely has < 30).

## Verification & deploy
- Author on a branch off **`main`** (site deploys from `main`; this is NOT the pipeline branch).
- `pnpm build` + `pnpm check:all` (biome/type gate) must be green before merge — CF Pages
  auto-deploys on push to `main`, so nothing lands in production until the branch is merged.
- Spot-check the built page: britons-vs-franks at +1650 shows ~47–54% with a real game count (no
  "az veri"), and the map section renders under a selected bracket.
- Heavy site build should run on the VM per the compute rule, not the desktop.

## Open decisions for the user
1. Fix 3 flavor: **(a)** elo curve / (b) competitive default / (c) caption?
2. Map section: winrate-by-map only, or add games-per-map distribution (recommended)?
3. Apply the same three fixes to the **team** matchups view (`civ-matchups-team.json`) too?
