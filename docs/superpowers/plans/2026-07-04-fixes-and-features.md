# AOE2 Guide — MASTER Plan (Fixes + Features + Full UI/UX Overhaul)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **This is the single master plan — execute §0's order top-to-bottom, non-stop.**

**Goal:** One consolidated, sequential plan covering everything left: correctness bugs, the two backlog fixes, the two analyzer features, the full site-wide UI/UX overhaul (from the 13-agent audit), and the chart-unification + Data-Explorer design work. Do it all in one run, in §0's order.

**Architecture:** Plans **A–F** are the detailed, bite-sized implementation plans (A Mapuche fix, B refactors, C winner-benchmarks, D TR fix-codes, E Data-Explorer design brief, F chart unification). The **UI/UX Overhaul** section (correctness bugs C2–C5, themes T1–T6, Phases 1–4) came from a site-wide audit of the real rendered pages. **§0 — Master Execution Order** interleaves both into one non-stop sequence and resolves the overlaps (Plan F IS the chart half of theme T1; do not do charts twice).

**Tech Stack:** Astro 5 (static), TailwindCSS v4 (CSS-first), TypeScript strict, Rust (`replay-rs` + `replay-wasm`), DuckDB on the VM, ECharts, Vitest, `pnpm`.

## Global Constraints

- **Static-output only.** No SSR, no API routes, no server runtime. (CLAUDE.md)
- **Source-derived, no defaults.** Every AoE2 value from a cited source; generators fail loud on missing source, never default/fallback.
- **Bilingual gate.** Any new user-visible EN string needs a real TR twin or `audit-yaml-translations` fails the build. Slugs are English-canonical.
- **Do NOT hand-edit generated civ YAML fields** (bonuses, team bonus, unique-tech effects, unique units, **tagline**) — regenerate via `pnpm build:civilizations`. Only `strategy` prose is hand-written and carried forward verbatim.
- **Two civ-id spaces, never conflate:** `src/data/civ-id-map.json` = game/replay `civ_id`; `src/data/relic-civ-id-map.json` = Relic API `civilization_id`.
- **Gates before push (deploy fires on push):** `pnpm check` · `pnpm test` (365 vitest) · `pnpm build` (prebuild: validate-icons, verify-build-facts, validate-data, validate-stats, audit-yaml-translations, build-og) · `pnpm exec biome ci .` on changed files · `cargo test` (in `replay-rs`) when Rust changed.
- **VM workflow:** edit generator locally → `rsync` to VM → run there → `scp`/`rsync` output JSON back. DuckDB write-lock held by the `duckui` tmux session — use `~/bin/duckdb -readonly` (or plain in-memory for parquet-only) for reads; don't kill the session. `LC_ALL=C` before any `sed [a-z]` range on this Turkish-locale machine.

---

## §0 — MASTER EXECUTION ORDER (do these top-to-bottom, non-stop)

Each line links to its detailed section. Effort: S/M/L. Commit + run gates after each. **Gate every push:** `pnpm check` · `pnpm check:all` (biome, whole tree) · `pnpm test` · `pnpm i18n:coverage` · `pnpm build` (+ `cargo test` when Rust changed). Keep CI green.

### Stage 1 — Correctness bugs (fix first; safe, mostly S)
- [x] **C1** civ-matchups inline-script TS-generic → **DONE** (`9bc9930`, shipped 2026-07-04)
- [ ] **Plan A** — Mapuche civ-type contradiction (strategy prose vs source tagline) · S
- [ ] **C2** — `toLocaleString()` no locale → "2.316 games" on EN · S · `matchups/index.astro:244,419`
- [ ] **C3** — empty-unit state leaks raw kebab slug in `<code>` + collapsed space + dup badge · S · `UniqueUnitBlock.astro:82`, `i18n/ui.ts`
- [ ] **C4** — hardcoded English `<h2>Upgrade Chain</h2>` on TR routes · S · `units/[unit].astro:64`, `i18n/ui.ts`
- [ ] **C5** — no loading state on the CivStats patch/cube fetch (stale UI then snaps) · S

### Stage 2 — UI quick wins (Phase 1; all S, highest visible impact/hour) — see §UI-Phase-1
- [ ] Chart baseline honesty + 50% reference line (byElo/byPatch/byMap) — **do as the first slice of Plan F, not throwaway** · S
- [ ] Elo/patch axis legibility (abbreviate buckets, always-rotate) · S
- [ ] `gold-500`-as-text contrast fail (~1.8:1) → route text-gold through `gold-700` on the light token block · S · `globals.css`, `Header.astro`
- [ ] Build-order value legibility (AA floor on unchanged cells; hairline dash for empty) · S · `BuildOrderSteps.astro`
- [ ] Right-align + `font-mono tabular-nums` on the 3 numeric tables · S
- [ ] Build-step row `hover`/`focus-visible` feedback (rows are clickable+focusable, zero feedback today) · S
- [ ] `focus-visible` ring on `<select>`/`<summary>`; 44px hit areas (hamburger/search) · S · `globals.css`, `Header.astro`
- [ ] Unit page: suppress `0f`/`0s`, `tabular-nums`, dark label contrast · S · `units/[unit].astro`
- [ ] BuildCard "TEMPO" eyebrow → "AGE"/"REACHES"; fill off pure-green toward ember/gold · S
- [ ] Hero provenance: real patch id + gold tabular numerals · S · `[lang]/index.astro`
- [ ] Build "BUIL" header slice → icon or explicit i18n abbreviation · S

### Stage 3 — Shared foundations (Phase 2 primitives + refactors) — see §UI-Phase-2
- [ ] **Plan B** — consolidate title-casers into `displayName`; extract `loadGuideCivs` · S (cleanup; do before touching the same files below)
- [ ] Shared **numeric-cell / table primitive** (retrofit unit table → build-order → matchups → CivStats tiles → analyzer table) · M
- [ ] Shared **section-eyebrow component** (promote the home mono-kicker+tick motif site-wide; unify H2 scale) · M
- [ ] Themed **`<select>` component** (parchment fill, gilded chevron, 44px, gold focus) · M
- [ ] Dark-mode **token discipline** (burnished gold, logo-tile token, muted `wr-bad`) · M · `globals.css`
- [ ] **Plan F / F1** — extract the shared ECharts foundation (`src/lib/echarts.ts`) from MapCivRankings (no visual change; regression-screenshot) · M

### Stage 4 — Chart unification (theme T1 = Plan F/F2–F5) — see Plan F
- [ ] **F2** civ charts → ECharts · M · [ ] **F3** unit charts (net-new) · M · [ ] **F4** analyzer charts → ECharts · M · [ ] **F5** remove Chart.js · S

### Stage 5 — Analyzer data features
- [ ] **Plan C** — winner-benchmarks (Phase 1 first-military + eco timings; Phase 2 APM with fidelity gate) · M–L
- [ ] **Plan D** — TR fix-codes (Phase 1 static + Phase 2 templated) · M

### Stage 6 — Per-page deeper redesigns (Phase 3) — see §UI-Phase-3
- [ ] Matchups ranked best/worst table + diverging encoding · L
- [ ] Glossary real anchor links + sticky A–Z + type-to-filter · M
- [ ] Learn scroll-spy TOC + chapter pager + h3 signature · M
- [ ] Analyzer bold, keyboard-operable drop zone + two-phase loading · M
- [ ] Map/Home header dignity + palette fixes (tier bars, spotlight anchoring, dock floating toolbar) · M

### Stage 7 — Net-new (Phase 4) — see §UI-Phase-4
- [ ] Unit **counters block** (Strong/Weak vs → bonus-damage hbars) · L · highest-value missing unit element
- [ ] Unit stat-table depth (train time, armor classes, ROF, LOS; kill duplicate readout) · M
- [ ] Analyzer **sample-replay** path (bundle a small `.aoe2record`) · M
- [ ] Counters page grouped table (winner/loser/reason) · M
- [ ] **Cumulative "X+" elo toggle** on civ/map/matchup stat views · S · **client-side, no pipeline change**: the cube already stores games+wins per band, so "1650+" = sum of bands ≥1650 (exact `Σwins/Σgames`). Add a band-vs-cumulative toggle; cutoffs on existing band edges (1650+, 1800+, 2000+) so sums are clean. Pools far more games per view for stable win rates. NOTE: does NOT apply to the analyzer winner-**medians** (can't sum medians; peer-band + rollup fallback is correct there, and the backfill is already thickening every band). See §UI-Phase-4 note.

### Stage 8 — Data Explorer (Plan E) — NEEDS A BRAINSTORM FIRST
- [ ] Run `superpowers:brainstorming` on the 6 open design decisions → spec → then implement. Not code-ready. · L

---

## Plan A — Mapuche civilization-type contradiction (fix)

**Why:** The Mapuche page contradicts itself. The **tagline** (generated from the aoe2techtree source) reads "a Cavalry and Counter-units civilization"; the hand-written **strategy** prose reads "an Infantry civilization." The generated tagline reflects the source of truth, so the frozen strategy prose is the error.

**Files:**
- Modify: `src/content/civilizations/mapuche.yaml:41` (EN strategy) and `:49` (TR strategy)
- Verify against: `scripts/build-civilizations.mjs:497-499` (tagline template) — the `specialty` there is source-derived.

**Interfaces:** none (content-only).

- [ ] **Step 1: Confirm the source-of-truth civ type (do not guess)**

The tagline is regenerated from the techtree source. Confirm the current generated value is authoritative rather than trusting either sentence:

```bash
cd /home/mkdemiiir/Masaüstü/aoe2-guide
grep -n "tagline" -A2 src/content/civilizations/mapuche.yaml
# Expect en: "Mapuche — a Cavalry and Counter-units civilization from South America."
# (generated at build-civilizations.mjs:497 from the source `specialty`)
```

If you can run it, re-derive to be certain the source still yields the same specialty:
```bash
pnpm build:civilizations && git diff --stat src/content/civilizations/mapuche.yaml
# Expect: tagline UNCHANGED (still "Cavalry and Counter-units") → it is the source truth.
```
Expected: the tagline is "Cavalry and Counter-units"; the strategy line is the inconsistent one.

- [ ] **Step 2: Fix the EN strategy prose to match the source**

In `src/content/civilizations/mapuche.yaml`, change line 41 from:
```yaml
    Mapuche are an Infantry civilization from South America.
```
to:
```yaml
    Mapuche are a Cavalry and Counter-units civilization from South America.
```

- [ ] **Step 3: Fix the TR strategy prose to match**

Change line 49 from:
```yaml
    Mapuche, Güney Amerika'dan bir Piyade uygarlığıdır.
```
to (mirroring the generated TR tagline's phrasing at `:7`, "Süvari ve Karşıt Birlik"):
```yaml
    Mapuche, Güney Amerika'dan bir Süvari ve Karşıt Birlik uygarlığıdır.
```

- [ ] **Step 4: Verify no contradiction remains and the build passes**

```bash
grep -niI "infantry\|piyade\|cavalry\|süvari" src/content/civilizations/mapuche.yaml
# Expect: tagline + strategy now agree; the bonus line "Infantry, Slingers…" (a real
# unit-class bonus, unrelated to the civ TYPE) legitimately still mentions Infantry.
pnpm build
```
Expected: build passes (audit-yaml-translations green — both fields have real TR twins).

- [ ] **Step 5: Commit**

```bash
git add src/content/civilizations/mapuche.yaml
git commit -m "fix(content): Mapuche strategy prose matches source-derived civ type"
```

---

## Plan B — Clean-code refactors (safe, behavior-preserving)

**Why:** Three copy-pasted title-casers and an 8×-duplicated `guideCivs` loader, all flagged net-positive by the earlier review. Behavior-preserving.

**Files:**
- Modify: `src/pages/[lang]/builds/[build].astro:30,76` — replace local `civName` with `displayName`
- Modify: `src/components/content/BuildOrderSteps.astro:37-42,212` — replace local `titleCase` with `displayName`
- Reuse: `src/lib/display.ts` — existing `displayName(slug)` (splits on `[-_]`, capitalizes each word; a superset of both local casers, which split on `-` only — identical output for hyphen slugs)
- Create: `scripts/data-pipeline/lib/guide-civs.mjs`
- Modify (8 files): `scripts/data-pipeline/{aggregate-maps,aggregate-rich,aggregate-patches,build-civ-cube,refresh-team-current,refresh-civ-current,refresh-map-current,refresh-matchups-current}.mjs`

**Interfaces:**
- Produces: `loadGuideCivs(): Set<string>` from `scripts/data-pipeline/lib/guide-civs.mjs`.

### B1. Consolidate the title-casers

- [ ] **Step 1: Replace `civName` in `builds/[build].astro`**

Delete the local definition at line 30:
```ts
const civName = (s: string) => s.split("-").map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(" ");
```
Add to the frontmatter imports:
```ts
import { displayName } from "@/lib/display";
```
Change the usage at line 76 from `{civName(civSlug)}` to `{displayName(civSlug)}`.

- [ ] **Step 2: Replace `titleCase` in `BuildOrderSteps.astro`**

Delete the local `titleCase` function (lines 37-42). Add to imports:
```ts
import { displayName } from "@/lib/display";
```
Change the two usages at line 212 from `titleCase(iconSlug)` to `displayName(iconSlug)`.

- [ ] **Step 3: Verify identical rendered output**

```bash
pnpm check && pnpm build
# Spot-check a build page renders civ + icon names unchanged (no "black_forest" case regressions):
pnpm preview --port 4331 &
# (compare a /en/builds/<slug>/ page before/after — names must be identical)
```
Expected: `check` + `build` pass; rendered names unchanged.

- [ ] **Step 4: Commit**

```bash
git add "src/pages/[lang]/builds/[build].astro" src/components/content/BuildOrderSteps.astro
git commit -m "refactor: consolidate title-casers into lib/display displayName"
```

### B2. Extract the `guideCivs` loader

- [ ] **Step 1: Create the shared module**

Create `scripts/data-pipeline/lib/guide-civs.mjs`:
```js
// Single source for "which civs the guide covers" — the slug set used to filter
// crawl/replay rows down to guide civs. Was copy-pasted across ~8 generators.
import { readFileSync } from "node:fs";
import path from "node:path";

/** Set of guide civ slugs from src/data/civilizations.json. */
export function loadGuideCivs() {
  const file = path.resolve("src/data/civilizations.json");
  return new Set(JSON.parse(readFileSync(file, "utf8")).civs.map((c) => c.slug));
}
```

- [ ] **Step 2: Replace the duplicated loader in each of the 8 generators**

In each file, remove the local `const guideCivs = new Set(...readFileSync("src/data/civilizations.json")...)` block and add near the top:
```js
import { loadGuideCivs } from "./lib/guide-civs.mjs";
const guideCivs = loadGuideCivs();
```
(Keep the variable name `guideCivs` so all downstream `.has(...)` call sites are untouched.)

- [ ] **Step 3: Verify each generator still runs and emits identical output**

These run on the VM against the DuckDB. For a repo-side smoke check that the import resolves and the set is non-empty:
```bash
node -e "import('./scripts/data-pipeline/lib/guide-civs.mjs').then(m=>{const s=m.loadGuideCivs();if(s.size<40)throw new Error('guideCivs too small: '+s.size);console.log('guideCivs:',s.size)})"
```
Expected: prints the civ count (≥40). Full output-parity is verified on the VM at the next manual aggregate run.

- [ ] **Step 4: Commit**

```bash
git add scripts/data-pipeline/lib/guide-civs.mjs scripts/data-pipeline/*.mjs
git commit -m "refactor(pipeline): extract shared loadGuideCivs loader"
```

---

## Plan C — Analyzer winner-benchmarks (B1) ⭐

**Why:** The analyzer's head-to-head matrix prints "Winners —" (an em-dash) on the first-military, APM, eco-APM, mil-APM, and eco-upgrade rows because no winner reference was ever computed. The user asked twice: "eco upgrades must show winners timings" and "what does — mean on these rows."

**Key constraint (from recon):** these medians **cannot** come from the aoestats/`match_ages` source the current `build-winner-refs.sql` uses — that corpus has research tech *names* per age-window but **no per-event timestamps and no APM**. They must be computed from the **`replay_events` shard** (`load-replays.sql` → table `replay_events`: `match_id, player_number, t_ms, kind, target_id`), filtered to ranked-1v1 winners, bucketed by elo via a join to a rating source. New data section lands in the existing `public/winner-refs.json` (already keyed by `r.bucket`, already fetched by `loadRefs()`), so the frontend plumbing is minimal.

**Two honest phases:**
- **Phase 1** — first-military time + eco-upgrade research *times* (Loom/DBA/Wheelbarrow/Horse-Collar). These are **unambiguous** (`min(t_ms)` with a target filter), high-confidence, and directly answer the "eco timings" ask.
- **Phase 2** — APM / eco-APM / mil-APM medians. Riskier: the winner median must use the **same APM definition** the engine emits per-player, or the delta is meaningless — so it carries an explicit fidelity-validation gate.

**Files:**
- Create: `scripts/data-pipeline/build-winner-meds.sql`
- Modify: `scripts/data-pipeline/build-winner-refs.mjs:18-43` (add a `medsByBucket` merge block)
- Modify: `src/pages/[lang]/analyzer.astro` — the first-military row (`:404-408`), the `att[]` APM rows (`:420-424`), and the `tech[]` eco rows (`:377-391`)
- Reference (do not edit): `scripts/data-pipeline/replay-rs/src/analyze/metrics.rs:12-23` (`WATCHED_TECHS` ids: Loom 22, DBA 202, Wheelbarrow 213, Horse-Collar 14), `config.rs:59` (`ECO_UNIT_IDS = [83,13,128,354,17]`), `metrics.rs:161-178` (`classify_cmd` eco/mil split), `scripts/data-pipeline/lib/buckets.mjs` (elo boundaries — mirror exactly)

**Interfaces:**
- Produces: `winner-refs.json` gains `medsByBucket: { <bucket>: { winners_n, first_military_ms, loom_ms, dba_ms, wheelbarrow_ms, horse_collar_ms, apm, eco_apm, mil_apm } }` (Phase 2 adds the last three).
- Consumes (frontend): `refs.medsByBucket[r.bucket]`.

### Phase 1 — first-military + eco-upgrade research times

- [ ] **Step 1: Confirm the rating-source join column on the VM**

The medians need elo, but `replay_players` has `won` and no rating. Confirm where per-player rating lives:
```bash
export SSH_AUTH_SOCK=/run/user/1000/ssh-agent.socket
ssh mkd@192.168.111.7 '~/bin/duckdb -readonly ~/aoe2-guide/data-cache/aoe2.duckdb -c "DESCRIBE games" | grep -iE "rating|profile|match_id"'
```
Expected: a per-(match_id, profile_id) `rating` column. Note its exact name for the SQL below.

- [ ] **Step 2: Write `build-winner-meds.sql`**

Create `scripts/data-pipeline/build-winner-meds.sql` (elo `CASE` mirrors `lib/buckets.mjs` boundaries exactly):
```sql
-- Winner medians from the replay EVENT corpus (the only source with timestamps):
-- first-military + eco-tech research times, among ranked-1v1 WINNERS, per elo bucket.
SET temp_directory='/tmp/duck_tmp';
COPY (
  WITH onev1 AS (
    SELECT match_id FROM replay_players GROUP BY match_id HAVING count(*) = 2
  ),
  winners AS (
    SELECT p.match_id, p.player_number, g.rating
    FROM replay_players p
    JOIN onev1 USING (match_id)
    JOIN games g ON g.match_id = p.match_id AND g.profile_id = p.profile_id
    WHERE p.won = TRUE AND g.rating IS NOT NULL
  ),
  bucketed AS (
    SELECT match_id, player_number, CASE
      WHEN rating < 1000 THEN '<1000'      WHEN rating < 1200 THEN '1000-1199'
      WHEN rating < 1400 THEN '1200-1399'  WHEN rating < 1650 THEN '1400-1649'
      WHEN rating < 1800 THEN '1650-1799'  WHEN rating < 2000 THEN '1800-1999'
      WHEN rating < 2200 THEN '2000-2199'  WHEN rating < 2500 THEN '2200-2499'
      ELSE '2500+' END AS bucket
    FROM winners
  ),
  per_player AS (
    SELECT b.bucket, b.match_id, b.player_number,
      min(CASE WHEN e.kind='train'    AND e.target_id NOT IN (83,13,128,354,17) THEN e.t_ms END) AS first_military_ms,
      min(CASE WHEN e.kind='research' AND e.target_id=22  THEN e.t_ms END) AS loom_ms,
      min(CASE WHEN e.kind='research' AND e.target_id=202 THEN e.t_ms END) AS dba_ms,
      min(CASE WHEN e.kind='research' AND e.target_id=213 THEN e.t_ms END) AS wheelbarrow_ms,
      min(CASE WHEN e.kind='research' AND e.target_id=14  THEN e.t_ms END) AS horse_collar_ms
    FROM bucketed b
    JOIN replay_events e USING (match_id, player_number)
    GROUP BY 1,2,3
  )
  SELECT bucket, count(*) AS winners_n,
    CAST(median(first_military_ms) AS BIGINT) AS first_military_ms,
    CAST(median(loom_ms) AS BIGINT)           AS loom_ms,
    CAST(median(dba_ms) AS BIGINT)            AS dba_ms,
    CAST(median(wheelbarrow_ms) AS BIGINT)    AS wheelbarrow_ms,
    CAST(median(horse_collar_ms) AS BIGINT)   AS horse_collar_ms
  FROM per_player GROUP BY 1 HAVING count(*) >= 50 ORDER BY 1
) TO '/tmp/winner-meds.csv' (HEADER, DELIMITER ',');
```

- [ ] **Step 3: Run it on the VM and sanity-check the output**

```bash
scp scripts/data-pipeline/build-winner-meds.sql mkd@192.168.111.7:/tmp/
ssh mkd@192.168.111.7 'cd ~/aoe2-guide && ~/bin/duckdb data-cache/aoe2.duckdb ".read /tmp/load-replays.sql" ".read /tmp/build-winner-meds.sql" && column -t -s, /tmp/winner-meds.csv'
```
Expected: one row per elo bucket; `first_military_ms` in a believable range (~250000–500000 ms = 4-8 min), research times ordered sensibly (Loom before Wheelbarrow). A bucket with <50 winners is absent (honest omission).

- [ ] **Step 4: Merge into `winner-refs.json`**

In `build-winner-refs.mjs` after the `ecoTechByCastle` block, add:
```js
const medsByBucket = {};
for (const r of rows(process.argv[4] ?? "/tmp/winner-meds.csv")) {
  medsByBucket[r.bucket] = {
    winners: +r.winners_n,
    firstMilitaryMs: +r.first_military_ms,
    loomMs: +r.loom_ms, dbaMs: +r.dba_ms,
    wheelbarrowMs: +r.wheelbarrow_ms, horseCollarMs: +r.horse_collar_ms,
  };
}
const out = { source, generated, openings, ecoTechByCastle, medsByBucket };
```
Run on the VM, `scp public/winner-refs.json` back, confirm the new key:
```bash
node -e "const w=require('./public/winner-refs.json'); console.log(Object.keys(w.medsByBucket), w.medsByBucket['1400-1649'])"
```
Expected: buckets listed; a sample object with `firstMilitaryMs` etc.

- [ ] **Step 5: Wire the first-military row (analyzer.astro:404-408)**

Add a `meds` lookup next to the existing `shares` line (~373): `const meds = r && refs?.medsByBucket?.[r.bucket];`. Then change the first-military row to reference the median, following the existing honesty guard (colored delta only when the median exists; else keep `noRef`):
```js
{
  label: L["analyzer.tile.military"],
  you: mmss(sel.first_military_ms), opp: opp ? mmss(opp.first_military_ms) : "—",
  ref: meds?.firstMilitaryMs != null ? mmss(meds.firstMilitaryMs) : noRef,
  dRef: meds?.firstMilitaryMs != null && sel.first_military_ms != null ? meds.firstMilitaryMs - sel.first_military_ms : null,
  fmtDelta: "secs", colored: meds?.firstMilitaryMs != null, help: L["analyzer.help.military"],
},
```
(Match the exact `dRef`/`fmtDelta` field names the row renderer already uses at `:323/:436-452` — confirm against the `Row` type at `:295-302`.)

- [ ] **Step 6: Wire the eco-upgrade timing rows (tech[], :377-391)**

For the Wheelbarrow row (and optionally add Loom/DBA/Horse-Collar rows), compare the player's own tech time (`wbTime(sel)`, id 213) against `meds.wheelbarrowMs`:
```js
ref: meds?.wheelbarrowMs != null ? `${L["analyzer.row.winnersResearch"]} ${mmss(meds.wheelbarrowMs)}` : (shares ? `${shares.wheelbarrow}% ${L["analyzer.row.winnersResearch"]}` : noRef),
dRef: meds?.wheelbarrowMs != null && wbTime(sel) != null ? meds.wheelbarrowMs - wbTime(sel) : null,
fmtDelta: "secs", colored: meds?.wheelbarrowMs != null,
```
This turns "Eco upgrades — Winners —" into "Winners Loom ~4:10" style timings — the exact ask.

- [ ] **Step 7: Gates + visual verify + commit**

```bash
pnpm check && pnpm test && pnpm build
# Load /en/analyzer/ with the test replay: first-military + wheelbarrow rows now show a
# winner time + colored delta instead of "—". Confirm a low-sample bucket still shows "—".
git add scripts/data-pipeline/build-winner-meds.sql scripts/data-pipeline/build-winner-refs.mjs \
        public/winner-refs.json "src/pages/[lang]/analyzer.astro"
git commit -m "feat(analyzer): winner medians for first-military + eco-upgrade timings"
```

### Phase 2 — APM / eco-APM / mil-APM medians (with fidelity gate)

- [ ] **Step 1: Pin the engine's APM definition, then replicate it in SQL**

Read `walk.rs` (APM = one count per raw `Operation::Action`; `cmd_split` via `classify_cmd` at `:200`) and `metrics.rs:161-178` (`classify_cmd` eco/mil rule). Determine exactly which `replay_events.kind` values correspond to a counted action and to the eco/mil split. Extend `build-winner-meds.sql` with per-player `apm = action_count / duration_min`, `eco_apm`, `mil_apm`, then `median()` per bucket. Duration comes from the `meta` shard / `replay_meta` (match duration).

- [ ] **Step 2: FIDELITY GATE — validate SQL APM against the engine**

Pick 3 known match_ids in the corpus. For each, run `replay-rs analyze --match-id <id> --json` (engine's per-player apm/eco_apm/mil_apm) and compare to the SQL-computed per-player values for the same match/player.
Expected: they match within rounding. **If they diverge, the SQL definition is wrong — fix it before computing medians**, or the matrix would compare incomparable numbers (a data-honesty violation).

- [ ] **Step 3: Add the three medians to `medsByBucket` and wire the att[] rows**

Extend the `SELECT` + the `.mjs` merge with `apm`, `eco_apm`, `mil_apm`. Wire the three `att[]` rows (`:420-424`) exactly like Step 5 above (ref = median, dRef only when present, colored true). Higher APM is "better" so the delta sign convention there is `you - ref` (confirm direction against a known-good example so the color isn't inverted).

- [ ] **Step 4: Gates + verify + commit**

```bash
pnpm check && pnpm test && pnpm build
# /en/analyzer/: APM/eco-APM/mil-APM rows now show winner medians + correctly-signed deltas.
git commit -am "feat(analyzer): winner APM/eco-APM/mil-APM medians (validated vs engine)"
```

---

## Plan D — Analyzer Turkish fix-strings (i18n)

**Why:** The analyzer's "Top Fixes" render in English on `/tr/` pages. The engine emits English prose in `Finding.metric` + `Finding.note`; the page prints them verbatim. The sibling fields `basis` and `severity` are **already** stable enums the frontend localizes via `L[\`analyzer.sev.${f.severity}\`]` — this plan applies that exact, proven pattern to the fix text.

**Design:** Add a stable `code` (and, for templated notes, a `vars` map) to `Finding`. **Keep** the English `metric`/`note` — the CLI renderer (`report.rs:122-149`) still uses them and must not regress. The browser prefers `L[\`analyzer.fix.${f.code}\`]`, falling back to `f.metric`/`f.note` when a code is unmapped (safe during rollout). Adding an optional field is additive to the JSON contract (no `SCHEMA_VERSION` bump needed; `model.rs:172`).

**Scope note (phasing):** 10 of the 15 findings have static notes (idle-TC ×3, Wheelbarrow, Loom, floating, military, scouting ×2) → **Phase 1**, ships alone. 5 have `format!` templates with dynamic values (`{civ}`, `{ref_desc}`, `{ref_v}`, `{map_slug}`, gap, opponent name) → **Phase 2**, which also localizes the 4 `ref_desc` variants. Each phase is independently shippable.

**Files:**
- Modify: `scripts/data-pipeline/replay-rs/src/analyze/model.rs:159-168` (add `code` + `vars` to `Finding`)
- Modify: `scripts/data-pipeline/replay-rs/src/analyze/compare.rs:149-316` (emit a code at each `mk` site; add `vars` for templated ones)
- Modify: `src/i18n/ui.ts` (add `analyzer.fix.*`, `analyzer.fixnote.*`, `analyzer.fixref.*` EN+TR entries)
- Modify: `src/pages/[lang]/analyzer.astro:664-681` (`findingsList` — resolve code→localized string)
- Rebuild: `pnpm build:wasm` → commit `src/wasm/pkg/` + `public/analyzer/pkg/replay_wasm_bg.wasm`
- Test: `scripts/data-pipeline/replay-rs/src/analyze/compare.rs` (assert every finding carries a non-empty `code`)

**Interfaces:**
- Produces: `Finding.code: &'static str` (stable, snake_case, one per emit site, e.g. `idle_tc_dark_high`), `Finding.vars: BTreeMap<&'static str, String>` (empty for static notes).
- Consumes (frontend): `L[\`analyzer.fix.${code}\`]` (metric label), `L[\`analyzer.fixnote.${code}\`]` (note, possibly with `{placeholders}` filled via the existing `fill()` helper at `analyzer.astro:609`).

### Phase 1 — codes for the 10 static findings

- [ ] **Step 1: Add fields to `Finding` (write the test first)**

In `compare.rs` tests, add:
```rust
#[test]
fn every_finding_carries_a_code() {
    // Build a report from the bundled test path or a synthetic metrics set that
    // triggers findings, then:
    for f in &report.findings {
        assert!(!f.code.is_empty(), "finding for metric {:?} has no code", f.metric);
    }
}
```
Run: `cargo test --offline every_finding_carries_a_code` → Expected: FAIL (no `code` field yet / compile error).

- [ ] **Step 2: Add the fields to the struct**

`model.rs:159-168`:
```rust
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub player_number: i32,
    pub code: &'static str,                              // NEW: stable fix code
    pub metric: String,                                  // keep: English label (CLI)
    pub your: String,
    pub reference: String,
    pub basis: Basis,
    pub severity: Severity,
    pub note: String,                                    // keep: English note (CLI)
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub vars: std::collections::BTreeMap<&'static str, String>, // NEW: template values
}
```

- [ ] **Step 3: Thread `code` through the `mk` helper**

`compare.rs:306-316` — add `code` as the first payload param, default `vars` empty:
```rust
fn mk(pn: i32, code: &'static str, metric: &str, your: &str, reference: &str,
      basis: Basis, severity: Severity, note: &str) -> Finding {
    Finding { player_number: pn, code, metric: metric.to_string(), your: your.to_string(),
        reference: reference.to_string(), basis, severity, note: note.to_string(),
        vars: std::collections::BTreeMap::new() }
}
```

- [ ] **Step 4: Assign a unique code at each of the 10 static emit sites**

Map (site line → code):
`166→"idle_tc_dark_high"`, `169→"idle_tc_dark_med"`, `173→"idle_tc_feudal"`, `248→"no_wheelbarrow"`, `252→"no_loom"`, `262→"floating_res"`, `270→"late_military"`, `280→"never_scouted"`, `282→"scouted_late"`. Pass the code as the new second arg to each `mk(pn, "<code>", …)`.

- [ ] **Step 5: Run the test + full Rust suite**

Run: `cargo test --offline` → Expected: PASS (all codes present; existing 36 tests still green).

- [ ] **Step 6: Add EN+TR i18n entries for the 10 codes**

In `src/i18n/ui.ts`, mirror the existing `analyzer.sev.*` shape. For each code add a `analyzer.fix.<code>` (metric label) and `analyzer.fixnote.<code>` (note) in BOTH `en` and `tr` blocks. Example:
```ts
// en:
"analyzer.fix.no_loom": "Loom",
"analyzer.fixnote.no_loom": "No Loom by Feudal — cheap villager survivability vs early aggression.",
// tr:
"analyzer.fix.no_loom": "Loom",
"analyzer.fixnote.no_loom": "Feodal'e kadar Loom yok — erken saldırıya karşı ucuz köylü dayanıklılığı.",
```
(Repeat for all 10. `name`-like proper nouns e.g. "Loom"/"Wheelbarrow" may be identical EN/TR — allowed by the audit's proper-noun allow-list, but prefer a real TR note string.)

- [ ] **Step 7: Localize the render in `findingsList`**

`analyzer.astro:676` — resolve metric + note via the code with English fallback:
```js
const metricTxt = L[`analyzer.fix.${f.code}`] ?? f.metric;
const noteTxt = L[`analyzer.fixnote.${f.code}`] ?? f.note;
```
Then use `esc(metricTxt)` / `esc(noteTxt)` in place of `esc(f.metric)` / `esc(f.note)`.

- [ ] **Step 8: Rebuild WASM, run gates, verify on a TR page**

```bash
pnpm build:wasm            # rebuilds engine; updates src/wasm/pkg/ + public/analyzer/pkg/
pnpm check && pnpm test
# Verify: load /tr/analyzer/, drop the test replay, confirm the 10 static fixes render in Turkish.
```
Expected: static findings localized on `/tr/`; `/en/` unchanged.

- [ ] **Step 9: Commit**

```bash
git add scripts/data-pipeline/replay-rs/src/analyze/{model.rs,compare.rs} src/i18n/ui.ts \
        "src/pages/[lang]/analyzer.astro" src/wasm/pkg public/analyzer/pkg
git commit -m "feat(analyzer): localize static Top-Fix strings via fix-codes (TR)"
```

### Phase 2 — templated findings + ref_desc

- [ ] **Step 1: Codes + vars for the 5 templated sites**

Sites & codes: `198→"feudal_slow"`, `207→"castle_slow"`, `214→"imperial_slow"`, `224→"villagers_castle"`, `235→"castle_vs_pro"`, `295→"feudal_vs_opp"`. At each, set `code` and populate `vars` with the dynamic pieces already computed locally, e.g.:
```rust
let mut f = mk(pn, "feudal_slow", "Feudal up-time", your, reference, basis, sev, &note_en);
f.vars.insert("civ", civ.to_string());
f.vars.insert("ref_kind", ref_kind_str);   // one of: exact|mapmode|mapall_1v1|mapall
f.vars.insert("bucket", bucket.to_string());
f.vars.insert("mode", mode.to_string());
f.vars.insert("map", map_slug.to_string());
out.push(f);
```
(Keep the English `note_en` via the existing `format!` for the CLI.)

- [ ] **Step 2: i18n entries with placeholders + the 4 ref_desc variants**

```ts
// note templates (en shown; add tr twins):
"analyzer.fixnote.feudal_slow": "Slower to Feudal than the {refDesc} for {civ}.",
// ref_desc variants (composed into {refDesc}):
"analyzer.fixref.exact":       "{bucket} {mode} median on {map}",
"analyzer.fixref.mapmode":     "{map} {mode} median (all elo)",
"analyzer.fixref.mapall_1v1":  "{map} median (no 1v1 baseline — team-heavy rollup)",
"analyzer.fixref.mapall":      "{map} median (all elo/mode)",
```

- [ ] **Step 3: Frontend composes refDesc then fills the note**

In `findingsList`, before rendering a templated finding:
```js
const v = f.vars ?? {};
const refDesc = v.ref_kind ? fill(L[`analyzer.fixref.${v.ref_kind}`] ?? "", { ...v, map: mapName(v.map) }) : "";
const noteTpl = L[`analyzer.fixnote.${f.code}`] ?? f.note;
const noteTxt = f.vars ? fill(noteTpl, { ...v, refDesc, civ: civName(v.civ) }) : noteTpl;
```
(`fill` exists at `analyzer.astro:609`; `mapName`/`civName` = localize the slug via the already-loaded data — reuse the page's existing name lookups.)

- [ ] **Step 4: Rebuild, gate, verify, commit**

```bash
pnpm build:wasm && pnpm check && pnpm test
# Verify on /tr/analyzer/ with the test replay: age-timing + villager + pro findings now Turkish.
git add -A && git commit -m "feat(analyzer): localize templated Top-Fix strings + ref_desc (TR)"
```

**Follow-up (out of scope, note for later):** `PlayerMetrics.opening` (`analyzer.astro:700,726`) and `report.caveats` (`:766`) are the same English-passthrough class — localize with the same code pattern in a later pass.

---

## Plan E — Data Explorer (design brief, NOT an implementation plan)

**Status: needs a brainstorm before it can be planned.** The engine question is settled (research verdict 2026-07-04): **DuckDB-WASM in the browser querying partitioned Parquet on R2** — the browser is network-bound, so no faster engine helps, and chDB-WASM/DataFusion-WASM are disqualified (95 MB blob that CF Pages rejects / no official browser build). Effort goes into **Parquet layout + pre-aggregation**, not engine choice.

**Design decisions to resolve in brainstorming (each blocks implementation):**

1. **Scope of queries.** Which questions can a visitor ask? (e.g. "win rate of civ X on map Y at elo Z over patch P", "distribution of feudal times for civ X", free-form SQL, or a guided query builder?) This sets the Parquet schema and partition keys.
2. **Partition scheme.** What columns to sort/partition by so row-group zonemaps prune hard — likely civ, patch, elo bucket, map. Row-group sizing. Whether to ship one big table or several rollups.
3. **Publish pipeline.** New VM step: aggregate → write partitioned Parquet → upload to R2 (needs an R2 bucket + token; the R2 backup-sync task, if done first, establishes the credentials + rclone setup).
4. **Query UI.** Guided controls (dropdowns → generated SQL) vs a SQL box vs both. How results render (table + a chart, reusing the ECharts setup from MapCivRankings?).
5. **Page + loading.** A dedicated `/[lang]/explorer` page; DuckDB-WASM (~3-6 MB) lazy-loaded only there, wrapped for ClientRouter (`astro:page-load` init). Loading/empty/error states.
6. **Cost + CORS guardrails.** R2 charges per-GET (range requests = many GETs); set R2 CORS headers; cap/debounce queries.

**Prerequisite:** the archive backfill should accumulate more replay depth first (it feeds the interesting queries), and the R2 backup-sync task (separate, small) establishes the R2 credentials this reuses.

**Next action for E:** run `superpowers:brainstorming` on the Data Explorer with the six decisions above as the agenda, producing a spec → then a real implementation plan.

---

## Plan F — Unify all charts on ECharts (drop Chart.js)

**Why (user, 2026-07-04):** the site runs **two** chart libraries and one page type has none — civ pages + analyzer use **Chart.js**, the new map leaderboard uses **ECharts**, unit pages have only stat tables. Standardize on ECharts for one consistent, polished system everywhere.

**Architecture — foundation first.** `MapCivRankings.astro` already contains the reusable ECharts patterns (theme-token reading, dark-mode `MutationObserver`, ClientRouter `astro:page-load`/`before-swap` lifecycle, tooltip styling). Extract those into a shared module so every chart is consistent and the lifecycle logic isn't copy-pasted (the "one canonical form" rule). Then migrate each page onto it; remove Chart.js last.

**Bundle note:** ECharts (~181 KB gz) is heavier than Chart.js (~51 KB), but it becomes **one shared chunk cached across all chart pages**, and Chart.js is removed — so it's one system, not two. Civ + analyzer pages already pay for a chart lib today.

**Phases (each self-contained + shippable):**

### F1 — Extract the shared ECharts foundation (safe refactor, no visual change)
- Create `src/lib/echarts.ts`: `themeColors()` (reads `--color-wr-good/-bad/-ink/-stone-700/-parchment/-gold-500` from `getComputedStyle`, with the same hardcoded fallbacks as MapCivRankings), and a `mountChart(el, buildOption, { onThemeChange })` helper that does `echarts.init`, wires a `ResizeObserver` + a `data-theme` `MutationObserver`, and returns a `dispose()` for `astro:before-swap`.
- Rewire `MapCivRankings.astro`'s script to import from `src/lib/echarts.ts` instead of its inline copies. **Regression gate:** screenshot `/en/maps/arabia/` light + dark before/after — pixel-identical. No new user-visible strings.

### F2 — Civ pages → ECharts (first visible win; highest traffic)
- Recon `CivStats.astro` first (3 charts: win-rate-by-elo bar, by-patch trend line, by-map bar-list). Migrate each to ECharts via the F1 foundation, matching the map-leaderboard polish (diverging gold/red around 50% where applicable, emblem/label axis, parchment tooltip). Keep the existing data flow (`civ-meta.json` + the cube fetch).
- Gates + light/dark screenshots of a civ page. Chart.js import stays until F5.

### F3 — Unit pages → new ECharts charts (net-new; NEEDS DESIGN SIGN-OFF)
- Unit pages have **no** charts today. **Open decision (confirm before building):** what to visualize — e.g. a stat-comparison bar (this unit's cost / HP / attack / armor vs its class average), and/or a counters strip (this unit vs the units that counter it). Recon `UnitStatTable.astro` + `src/pages/[lang]/units/[unit].astro` + the unit stat data in `src/data/` for available fields.
- Build via the F1 foundation; EN/TR labels; gates + screenshots.

### F4 — Analyzer charts → ECharts
- Recon the two Chart.js charts in `analyzer.astro` (production-over-time multi-series line — villagers solid, military dashed; APM-over-time line). Migrate both to ECharts multi-series line via the F1 foundation. The head-to-head **matrix stays an HTML table** (not a chart). Careful with the ClientRouter + `astro:page-load` wrapping already present.
- Gates + verify with the test replay on `/en/analyzer/` and `/tr/analyzer/`.

### F5 — Remove Chart.js
- Delete the `chart.js` import from `CivStats.astro` + `analyzer.astro` (now unused), remove `chart.js` from `package.json`, run `pnpm install`. Grep to confirm zero `chart.js`/`new Chart` references remain. Full gates + a bundle-size sanity check (one shared ECharts chunk, no Chart.js chunk).

**Prerequisite recon** (before F2/F3/F4 bite-sized steps): read each page's current chart code + data source — same discipline as Plan C. F1 is fully known (from building MapCivRankings) and can start immediately.


---

## UI/UX OVERHAUL — full detail (from the 13-agent audit)

_This is the detail behind §0 Stages 1-8. Bug C1 is DONE; C2-C4 re-verified real. Chart items (theme T1) are executed as Plan F — do not duplicate._


**Session annotations (2026-07-04):**
- **C1 is already FIXED and shipped** (commit `9bc9930`): the civ-matchups inline-script TS-generic bug that killed the ladder/map/view controls on every civ page. Verified error-free. The table below keeps it for the record, struck through.
- **C2, C3, C4 independently re-verified as real** against the source (unlocalized `toLocaleString` at matchups/index.astro:244,419; raw kebab slug in `<code>` at UniqueUnitBlock.astro:82; hardcoded English `<h2>Upgrade Chain</h2>` at units/[unit].astro:64).
- Execution order: **correctness bugs (C2-C5) → Phase 1 quick wins → Phase 2 shared primitives (incl. Plan F) → Phase 3 per-page → Phase 4 net-new.** Phase 2 primitives (table cell, eyebrow, select) are connective tissue — build them before the per-page passes so Phase 3 stays thin.

---


## 1. State of the UI (3-sentence summary)
The illuminated-codex identity is strong and fully realized on the **home cover** (mono eyebrows + gold hairline ticks, Cinzel headings, ember/gold restraint), but that language **evaporates on every interior page** — Civ, Map, Unit, Matchups, Learn read like a plainer, more default site. The site's core signal — win rate — is **systematically misrendered** (zero/truncated baselines, no 50% reference line, unreadable overlapping axis ticks, three unrelated chart systems), and enumerable data is drawn **four incompatible ways** with no shared numeric-cell convention. Underneath the polish sit a handful of **real correctness bugs** (a dead JS toggle shipping on every civ page, locale-broken number formatting, untranslated headings, slug-leaking empty states) that must be separated from styling work.

---

## 2. CORRECTNESS BUGS — fix first (not UI polish)

| # | Bug | Sev | Eff | File(s) |
|---|-----|-----|-----|---------|
| ~~C1~~ ✅ DONE | `<script data-astro-rerun>` ships raw TS generic `querySelectorAll<HTMLElement>` to the browser → SyntaxError aborts the whole script → **1v1/Team toggle, map `<select>`, and active-state are dead on every civ page** (confirmed in built HTML). Fix: make it a processed module (add an import like CivStats does) or drop the four `<HTMLElement>` annotations; re-verify the generic is gone from `dist/`. | **high** | S | `src/components/content/CivMatchups.astro` |
| C2 | `games.toLocaleString()` takes no locale arg → renders **"2.316 games" on the EN page** (European separator), non-deterministic across visitors. Fix: `toLocaleString(lang === 'tr' ? 'tr-TR' : 'en-US')`. | med | S | `src/pages/[lang]/matchups/index.astro` |
| C3 | Empty state renders **"Stats not yet available forcomposite-bowman"** — collapsed space + raw kebab slug in `<code>` + a duplicate "stats pending" Badge saying the same thing. Fix: interpolate localized display `name`, fix spacing, drop the redundant marker. (Reported by 3 auditors.) | med | S | `src/components/content/UniqueUnitBlock.astro`, `src/i18n/ui.ts` |
| C4 | Hardcoded English `<h2>Upgrade Chain</h2>` + unit description untranslated on TR routes. Fix: add `units.upgradeChain` i18n key. | med | S | `src/pages/[lang]/units/[unit].astro`, `src/i18n/ui.ts` |
| C5 | Patch/cube data fetch has no loading state — stale UI sits frozen then snaps (x-states auditor). Fix: intermediate loading state. | med | S | (CivStats data-slice fetch) |

---

## 3. Deduplicated themes (merged across auditors)

- **T1 — Chart/dataviz honesty (Plan F territory).** Zero/truncated baselines crush the 48–62% real spread into the top third; no 50% reference line despite captions claiming "50% = average"; elo x-axis ticks (exactly ~10 buckets, guard fires at >10) overlap into a smear; values are tooltip-only; dark-mode "bad" bars flip to loud salmon; three unrelated chart vocabularies (Chart.js bars vs map hbar-list vs ECharts). *Flagged by civ, x-tables, cross-cutting, x-a11y, matchups, analyzer auditors.* Files: `CivStats.astro`, `MapCivRankings.astro`, `matchups/index.astro`, `analyzer.astro`, `globals.css`.
- **T2 — Shared table / numeric-cell system.** Four incompatible renderings of enumerable facts; the one real `<table>` (unit page) is the least polished (left-aligned values, no `tabular-nums`, no header, no dividers); build-order values center-aligned; analyzer player table left-aligned. Files: `units/[unit].astro`, `BuildOrderSteps.astro`, `CivMatchups.astro`, `CivStats.astro`, `analyzer.astro`.
- **T3 — Typography / eyebrow hierarchy.** Codex eyebrow+tick motif is home-only; same-rank H2s render at 2–3 different sizes per page (civ, map); h3 collapses into bold body runs (learn); format labels use stray Inter semibold. Files: home + all interior page files, `globals.css`.
- **T4 — Mobile / a11y.** `gold-500` as small text/links ≈1.8:1 on parchment (fails WCAG); sub-44px tap targets (hamburger, search, chart dataZoom handle ~10px); missing `focus-visible` on `<select>`/`<summary>`; floating toolbar overlaps 3rd spotlight card at 390px; build split-columns crush the step text on mobile; chain stats `hidden sm:flex` disappear on mobile. Files: `globals.css`, `Header.astro`, `BuildOrderSteps.astro`, `MapCivRankings.astro`, `BaseLayout.astro`, `units/[unit].astro`.
- **T5 — States / interaction.** No row hover/focus on clickable+focusable build steps; native OS `<select>` breaks identity everywhere; analyzer loading = bare mono text for a 1.4MB WASM fetch; elo filter silently drops the per-map block; empty states leak slugs (see C3). Files: `BuildOrderSteps.astro`, `CivStats.astro`, `MapCivRankings.astro`, `FilterBar.astro`, `matchups/index.astro`, `analyzer.astro`.
- **T6 — Per-page depth gaps.** Matchups is one-pair-at-a-time with no ranked table; comparator panels don't row-align; glossary "Related terms" are plain text not links + no sticky nav/filter on a 5600px page; learn TOC has no scroll-spy + weak pager + hidden course position; analyzer drop zone is timid/iconless + not keyboard-operable + no sample path; map header thinner than data-only pages; unit page reads sparse/unfinished.

---

## 4. Phased roadmap (impact-vs-effort ordered)

### Phase 1 — Quick wins (high-impact / low-effort, mostly S) — do right after correctness
- **[high/S]** Chart baseline honesty: replace `suggestedMin: Math.min(40,…)-1` with hard `min/max` (~40–62) + a 1px 50% reference line on byElo/byPatch/byMap — *implement as the first slice of Plan F, not a throwaway.* `CivStats.astro`
- **[high/S]** Elo axis legibility: drop rotation threshold to `>6`/always, abbreviate buckets (`<1.0k`, `1.0–1.1k`…`2.2k+`); same for byPatch → use stored `patchLabel`. `CivStats.astro`
- **[high/S]** `gold-500` text contrast: route text-sized gold through `gold-700` on the light token block only (prose links, eyebrow labels, "View all →"); dark-mode gold already passes. `globals.css`, `Header.astro`
- **[high/S]** Build-order value legibility: floor unchanged cells to ≈`stone-700/70` (AA), keep changed values bold in resource hue, render empty cells as a hairline dash not a 30%-opacity digit. `BuildOrderSteps.astro`
- **[med/S]** Right-align numeric cells + `font-mono tabular-nums`: build-order (currently center), unit stat table (currently left+Inter), analyzer player table (currently left). Fixes column scanning in three places. `BuildOrderSteps.astro`, `units/[unit].astro`, `analyzer.astro`
- **[med/S]** Build step rows: add themed `hover:bg-gold-500/5` + `focus-visible` gold ring (rows are `cursor-pointer` + `tabindex=0` today with zero feedback). `BuildOrderSteps.astro`
- **[med/S]** `focus-visible` gold ring for `<select>` and `<summary>` in the global rule (currently only `a`/`button`); bump hamburger/search to 44px hit areas. `globals.css`, `Header.astro`
- **[med/S]** Unit page: suppress zero-cost resources (`0f`/`0s`), set stat values in `font-mono tabular-nums`, lift dark-mode label contrast (`dark:text-stone-400`). `units/[unit].astro`
- **[med/S]** BuildCard: relabel "TEMPO" eyebrow to "AGE"/"REACHES" (bar encodes target age, not pace) and move fill off pure green toward the ember/gold ramp. `BuildCard.astro`
- **[low/S]** Hero provenance line: replace "current patch" with the real patch id from civMeta; lift numerals to gold tabular-nums, mute connectives. `[lang]/index.astro`
- **[low/S]** Build "BUIL" header (`RES_LABEL[k].slice(0,4)`) → proper icon or explicit i18n abbreviation (won't localize; reads as a glitch). `BuildOrderSteps.astro`

### Phase 2 — Systemic (the reusable foundations, M)
- **[high/M] Shared numeric-cell / table primitive** — one convention (numeric = `text-right font-mono tabular-nums`, label left, hairline `stone-700/15` dividers, optional `bg-parchment/70` zebra with a *real* step, header-row treatment); retrofit the unit `<table>` first, then build-order, matchups lists, CivStats tiles, analyzer table. Resolves T2 + the "four incompatible tables" finding. `units/[unit].astro`, `BuildOrderSteps.astro`, `CivMatchups.astro`, `CivStats.astro`, `analyzer.astro`
- **[med/M] Chart unification — reuse existing Plan F**, don't re-plan. Bring byElo/byPatch into the "by map" hbar-with-inline-value language (thin marks, 4px rounded data-ends, single hairline grid, value printed at bar end, red reserved for genuinely-below-average). One bar language across all win-rate views + matchup bars + analyzer production chart (add the missing solid=vils / dashed=military legend). `CivStats.astro`, `MapCivRankings.astro`, `matchups/index.astro`, `analyzer.astro`
- **[med/M] Shared section-eyebrow component** — promote the home mono-kicker + gold-tick motif to a component used on every interior section head, encoding something true ("RANKED · 1v1 RM", "TECH TREE", "CHAPTER n"). Unify all page-level H2s to one Cinzel scale + one divider rule. Resolves T3. Interior pages + `globals.css`
- **[med/M] Themed `<select>` component** — parchment fill, hairline border, custom gilded chevron, mono `text-xs`, 44px min-height, gold `focus-visible`; apply to FilterBar, CivMatchups, MapCivRankings, CivStats. Removes the most obvious "default SaaS" element. `FilterBar.astro`, `globals.css`, data-page components
- **[med/M] Dark-mode token discipline** — hand-select burnished dark gold (lower L / higher C) instead of auto-lighten; give the logo tile its own dark fill token; pull `wr-bad` to muted oxblood so charts stay validated on the dark ground. `globals.css`

### Phase 3 — Per-page deeper redesigns
- **[high/L]** Matchups: add a **ranked best/worst matchup table** for the selected civ from `h2hElo[a]` (opponent icon+name, win% right-aligned mono, games) — turns a lookup tool into a coaching tool. `matchups/index.astro`
- **[high/S]** Matchups: replace binary 50% color cut with a **diverging encoding + neutral 48–52% band** (per-map + h2h), add a hairline 50% tick + civ-bound legend to the signature bar, add a low-sample honesty state. `matchups/index.astro`
- **[med/L]** Matchups: restructure the two brochure panels into an **attribute-aligned comparison** (shared row labels, civ A/B columns). `matchups/index.astro`
- **[high/M]** Glossary: make **"Related terms" real anchors** (typed `related: [slug]` in schema, `id={slug}` per entry, gold chips); add a **sticky A–Z nav + type-to-filter**; turn the section letter into an illuminated drop-cap chapter break. `glossary/index.astro`, glossary schema/yaml
- **[high/M]** Learn: **scroll-spy TOC** (IntersectionObserver → `aria-current`), **numbered "Chapter n of 8" eyebrow**, **bordered prev/next pager cards**, distinct h3 signature, one codex flourish (lead drop-cap / fleuron divider). `learn/[chapter].astro`, `TableOfContents.astro`, `globals.css`
- **[high/M]** Analyzer: make the **drop zone the one bold element** (focal glyph + warm gold rest state), promote "Choose a file" to a real ≥44px button, make it **keyboard-operable** (`role="button"`, Enter/Space, visible focus), add a two-phase loading state (engine → replay). `analyzer.astro`
- **[med/M]** Map page: give the curated header the same dignity as data pages (mono eyebrow + larger Cinzel h1 + `type · size · N games`); explain or fold the "Recommended Civilizations" list (currently a redundant, unexplained 5-chip block next to the 53-civ chart); extract one `CivChip` size; widen the dataZoom grab to ≥24px on mobile. `maps/[map].astro`, `MapCivRankings.astro`
- **[med/M]** Home: fix tier-distribution bar palette (B/C/D indistinguishable tans → wider ΔL + hairline separators + inline counts); anchor spotlight win-rates to 50% with sample context; replace the "Counter an opponent" card's duplicated description-as-count; per-type map icons; dock the floating toolbar so it stops overlapping the 3rd spotlight card at 360–430px; extend eyebrows into interior sections. `[lang]/index.astro`, `BaseLayout.astro`

### Phase 4 — Net-new
- **[med/L]** Unit page **counters block** — start with a cheap "Strong vs / Weak vs" pair using existing `counters.strongVs/weakVs/beats` strings, graduate to bonus-damage hbars (Plan F marks) once data is wired. This is the highest-value missing element for a combat unit. `units/[unit].astro`, `i18n/ui.ts`
- **[med/M]** Unit page density: turn the Upgrade Chain into a pure cross-tier comparison (HP 4→35→40), make the stat table the authoritative block that adds what the chain omits (train time, armor classes, ROF, LOS) — kills the duplicate five-number readout; keep a compact stat readout visible on mobile (don't `hidden sm:flex` it). `units/[unit].astro`
- **[med/M]** Analyzer **sample-replay path** — bundle a small `.aoe2record` in `public/` + "Analyze a sample" secondary action so browse-only visitors see the payoff. `analyzer.astro`
- **[med/M]** Counters page: render age relations as a grouped table (winner/loser/reason, hard vs soft tag if source has it) instead of flat pills. `matchups/counters.astro`

---

## 5. Reuse existing planned work
- **Plan F (chart unification) already covers T1.** Phase 1's baseline/axis fixes and Phase 2's one-bar-language item are **slices of Plan F, not new work** — schedule them under it. The "by map" hbar-with-inline-value component is the already-on-theme reference every other win-rate view (byElo, byPatch, matchup bar, analyzer production/matrix) should converge to; do not invent a fresh chart system.
- **The shared numeric-cell primitive (Phase 2) and the shared eyebrow/select components** are the connective tissue that lets Phase 3 per-page redesigns be thin — build those before the per-page passes so the deep redesigns consume them rather than re-styling ad hoc.
- **Dark-mode fixes converge on tokens in `globals.css`** — fix once at the token (gold, `wr-bad`, logo tile) rather than per-component.

Key files by touch frequency: `src/components/content/CivStats.astro`, `src/components/content/BuildOrderSteps.astro`, `src/pages/[lang]/units/[unit].astro`, `src/pages/[lang]/matchups/index.astro`, `src/styles/globals.css`, `src/components/content/MapCivRankings.astro`.