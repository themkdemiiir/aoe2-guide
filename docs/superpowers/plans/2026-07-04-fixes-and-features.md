# AOE2 Guide — Fixes & Features Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clear the remaining backlog — two correctness/cleanup fixes and two analyzer features — and tee up the Data Explorer for its own design cycle.

**Architecture:** Four independent, separately-shippable plans (A–D). A and B are pure repo changes (deploy on push). C touches the VM pipeline + the analyzer page. D is a Rust-WASM + i18n change. E is NOT an implementation plan — it is the design brief the Data Explorer needs before it can be planned.

**Tech Stack:** Astro 5 (static), TailwindCSS v4 (CSS-first), TypeScript strict, Rust (`replay-rs` + `replay-wasm`), DuckDB on the VM, ECharts (map pages), Vitest, `pnpm`.

## Global Constraints

- **Static-output only.** No SSR, no API routes, no server runtime. (CLAUDE.md)
- **Source-derived, no defaults.** Every AoE2 value from a cited source; generators fail loud on missing source, never default/fallback.
- **Bilingual gate.** Any new user-visible EN string needs a real TR twin or `audit-yaml-translations` fails the build. Slugs are English-canonical.
- **Do NOT hand-edit generated civ YAML fields** (bonuses, team bonus, unique-tech effects, unique units, **tagline**) — regenerate via `pnpm build:civilizations`. Only `strategy` prose is hand-written and carried forward verbatim.
- **Two civ-id spaces, never conflate:** `src/data/civ-id-map.json` = game/replay `civ_id`; `src/data/relic-civ-id-map.json` = Relic API `civilization_id`.
- **Gates before push (deploy fires on push):** `pnpm check` · `pnpm test` (365 vitest) · `pnpm build` (prebuild: validate-icons, verify-build-facts, validate-data, validate-stats, audit-yaml-translations, build-og) · `pnpm exec biome ci .` on changed files · `cargo test` (in `replay-rs`) when Rust changed.
- **VM workflow:** edit generator locally → `rsync` to VM → run there → `scp`/`rsync` output JSON back. DuckDB write-lock held by the `duckui` tmux session — use `~/bin/duckdb -readonly` (or plain in-memory for parquet-only) for reads; don't kill the session. `LC_ALL=C` before any `sed [a-z]` range on this Turkish-locale machine.

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
