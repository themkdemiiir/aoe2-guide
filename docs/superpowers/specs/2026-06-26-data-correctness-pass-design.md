# Data-Correctness Pass — Design (Phase B)

**Status:** Proposed · 2026-06-26 · branch `feat/analyzer-foundation`
**Depends on:** the ground-truth audit + per-bug fix investigation (this session).
**Related:** [analyzer foundation effort], [game-analyzer design], [data-workbench].

## Why

User priority for this effort is **data correctness above all**. A repo audit surfaced six
correctness defects (one high-severity silent mis-coaching bug, one user-visible dead-source bug,
and four "fail-loud / one-source" rule violations). This pass fixes **every code-fixable issue
now** and explicitly **defers data-infra-blocked work** (loading forward `replay_ages` into DuckDB,
flank/pocket per-elo medians) to Phase E.

## The in-repo vs VM-regeneration boundary

Every change here is **code, committed in-repo**. But some corrected *data artifacts* can only be
regenerated where the data lives — the VM DuckDB (`mkd@192.168.111.7`) and the desktop crawl backup.
So each fix is split:

- **In-repo (this branch):** SQL/JS/Rust source edits, the new shared module, the new validation
  gate, and all unit tests. Reviewable and CI-checkable here.
- **Regeneration (user runs after merge):** re-run the affected generator to refresh the committed
  JSON/benchmark artifact. Captured in the **Regeneration runbook** at the end.

This means the *bug* is fixed in-repo immediately; the *stale data it produced* is refreshed when the
user runs the runbook. Both halves are required for the user to see corrected numbers.

---

## Fix 1 — 1v1 benchmark has no 1v1 data (HIGH) · `bench-1v1-mode`

**Root cause.** `build-benchmark.sql` classifies mode with `CASE WHEN team_size = 1 THEN '1v1' ELSE
'team'`, but **no row in `games` ever has `team_size = 1`**: every producer sources the 1v1 ladder's
`team_size` as `NULL` then `COALESCE(…, 2)`. So the CASE always returns `'team'`, and the committed
`benchmark.json` contains only `{all, team}` modes. The analyzer (`compare.rs`) correctly derives
`mode = '1v1'` for 2-player replays and asks `data.rs::slice` for the 1v1 slice, which doesn't exist,
so it falls back to the team-dominated `(civ, map, 'all', 'all')` rollup. **1v1 players are coached
against team-game age timings.** The reliable signal is the existing `games.ladder` column
(`'1v1' | 'team' | 'co_1v1' | 'co_team'`), populated for both crawl and aoestats rows.

> Note: the SQL header's "age-time corpus is almost entirely TEAM games" claim is itself a *symptom*
> of this bug (everything resolved to `'team'`). It must be **re-measured by `ladder`** after the fix,
> not trusted.

**Fix (in-repo):**
- `build-benchmark.sql`: classify mode from `ladder`, not `team_size`:
  `CASE WHEN ladder='1v1' THEN '1v1' WHEN ladder='team' THEN 'team' ELSE ladder END AS mode`
  (select `ladder` in the `g` CTE). Decide co-op handling explicitly: **keep `co_1v1`/`co_team`
  distinct** (do not fold into 1v1/team — co-op games can have >2 players and don't match the
  analyzer's `is_team_game` semantics).
- `build-benchmark.sql`: add a **same-mode rollup** grouping set so a per-map 1v1 median survives
  bucket sparsity: `GROUP BY GROUPING SETS ((civ,mp,bucket,mode),(civ,mp,mode),(civ,mp),(civ))`.
- `replay-rs/src/analyze/data.rs` `Benchmark::slice`: insert a same-mode rollup step in the fallback
  ladder — exact `(civ,map,bucket,mode)` → `(civ,map,'all',mode)` → `(civ,map,'all','all')` → `None`
  — so a 1v1 request prefers a 1v1 median before degrading to all-mode.
- `replay-rs/src/analyze/compare.rs`: when `mode=='1v1'` but the slice came from the all-mode rollup
  (no 1v1 cell existed), set the finding's `ref_desc` to **"no 1v1 baseline — team rollup on {map}"**
  so it's honestly flagged, not silently compared.
- Correct the stale SQL header note.

**Tests:** `data.rs` unit test asserting same-mode rollup is preferred over all-mode for a 1v1
request; update `benchmark_slice_is_map_specific_with_fallback` (currently asserts 1v1 always falls
back). `cargo test` green.

**Regeneration (VM):** re-run `build-benchmark.sql` → `bench.csv` → `build-benchmark.mjs` →
`benchmark.json`. **Verify** `grep ',1v1,' bench.csv | wc -l` > 0 (pre-fix it is 0) and capture the
`n` (row-count) column as evidence the 1v1 cells clear the `HAVING count >= 50` gate rather than being
sparse noise. If 1v1 cells are genuinely too thin even at the map rollup, the same-mode rollup +
honest `ref_desc` degrade gracefully (no silent mis-coaching).

---

## Fix 2 — civ tier sourced two ways; one is dead (HIGH-visibility) · `tier-consolidation`

**Root cause.** Live tier = self-collected `src/data/civ-meta.json` (read by `CivStats` + matchups as
`civMeta.civs[slug]["1v1"]?.tier`). Dead tier = a `meta` block on `civilizations.json`, only ever
written by `scripts/sync-meta-stats.mjs` (a hard-coded 2026-05-23 **aoestats.io** scrape) and never
emitted by `build-civilizations.mjs` — so it's **null for all 53 civs**. `CivCard.astro` and
`builds/[build].astro` still read the dead source → their **Tier badges never render**. Re-running
`pnpm sync:meta` would inject an older, differently-sourced stat set — a trap. Violates the
one-canonical-source rule.

**Fix (fully in-repo, no sourcing):**
- `CivCard.astro`: import `civ-meta.json`; read `tier = civMeta.civs[slug]?.["1v1"]?.tier ?? null`;
  render the badge from `tier` (keep `specialty` from civ-data). Null-safe.
- `builds/[build].astro`: replace the `civilizations.json` import with `civ-meta.json`; drop the
  `civs`/`civBySlug` lines; source recommended-civ tier via a `civTier(slug)` helper.
- `src/content/config.ts`: delete the dead `meta: z.object({…})` field from the `civData` schema.
- Delete `scripts/sync-meta-stats.mjs`; remove `"sync:meta"` from `package.json`.
- `civilizations.json` needs **no edit** (the key is already absent; the schema removal is enough).

**Tests:** `pnpm check` (TS) passes; `grep -rn '\.meta?.tier\|sync:meta\|sync-meta-stats' src package.json`
returns zero code hits; visual: a known-tier civ (e.g. `armenians` = 1v1 tier A) renders "Tier A" on
its CivCard and under recommended-civ chips; a civ missing from `civ-meta.json` renders no badge.

---

## Fix 3 — null-rating fabrication + copy-pasted bucket/canon logic · `silent-defaults-dedup`

**Root cause.** The 9-bucket elo classifier and map-canonicalizer were copy-pasted into ~10
generators instead of reconciled at one ingestion boundary. Three *incompatible* null-rating
behaviors evolved for the same operation:
1. DuckDB faces (`build-duckdb.sql`, `ingest-stream.mjs`) correctly emit `'unknown'`. ✅
2. Every JS generator does `eloBucket(rating ?? 0)` → fabricates `'<1000'` for a missing rating.
3. The JS-embedded SQL `CASE`s in `aggregate-{maps,matchups,rich}` **omit the `IS NULL` branch**, so
   in SQL a null rating falls through every `WHEN` to `ELSE '2500+'` — fabricating the **top** bucket
   (a second, opposite-direction silent fabrication).

Both `?? 0` and the bare `ELSE` invent a rating band never observed → violate the fail-loud rule.

**Fix (in-repo):**
- **New** `scripts/data-pipeline/lib/buckets.mjs`, the single ingestion-boundary source:
  - `ELO_BUCKETS` (the 9 labels) + `ELO_BUCKETS_WITH_ALL`.
  - `eloBucket(rating)` → returns **`null`** for `null`/`undefined` (never a fabricated band); the
    9-way ladder otherwise. *Caller decides* what to do with null.
  - `eloCaseSql(col, { nullLabel = 'unknown' } = {})` → builds the exact `CASE … IS NULL THEN
    '<nullLabel>' …` string from `ELO_BUCKETS`, so JS-embedded SQL shares thresholds **and always
    carries an `IS NULL` branch**.
  - `canonMap(raw)` → the single JS mirror of the SQL canon regex.
- **JS consumers** (`build-civ-cube`, `refresh-{civ,map,team,matchups}-current`, `aggregate-rich`):
  import the helper, delete local copies. Replace `eloBucket(rating ?? 0)` with a **null-guarded skip
  + counter** (`if (eb == null) { skippedNullElo++; continue; }`) and log the dropped count. Rationale:
  the cube/marginals are *presentation* data feeding a user-facing dropdown — an "unknown elo" filter
  option is meaningless, so **drop-with-count** (not `'unknown'`) is the honest choice here; DuckDB
  keeps `'unknown'` as a legitimate analytical category.
- **JS-embedded SQL** (`aggregate-{maps,matchups,rich}`): replace the hand-written `ELO` string with
  `eloCaseSql('p.new_rating')` — this **adds the missing `IS NULL` branch**, killing the null→'2500+'
  fabrication. Replace hard-coded label arrays with `ELO_BUCKETS[_WITH_ALL]`.
- `aggregate-civmeta.mjs`: make the existing `(p.rating ?? 0) < MIN_ELO` intent explicit
  (`p.rating == null || p.rating < MIN_ELO`).
- `build-duckdb.sql` (pure `.sql`, cannot import JS): no code change; add a comment marking it the
  **documented paired SQL face** of `buckets.mjs` — keep thresholds in sync.

**Tests:** new `lib/buckets.test.mjs` (Vitest): boundary cases + `eloBucket(null) === null`; a parity
test that the labels in `eloCaseSql()` equal `ELO_BUCKETS` and the string contains the `IS NULL`
branch; `canonMap` cases; a `build-civ-cube` fixture with one null-rating row asserting it's dropped
and `skippedNullElo === 1`.

**Regeneration:** `build-civ-cube` + `refresh-*` run **locally** (desktop crawl) → re-emit
`public/civ-cube.json`, `civ-meta.json` marginals, `map-meta.json`, matchups. `aggregate-*` run **on
the VM** (DuckDB + Parquet). Verify diffs are limited to previously-misbucketed null rows.

---

## Fix 4 — costs free-on-missing, freshness overstated, no stats gate · `costs-freshness-gate`

**(4a) `build-costs.mjs` silently frees no-Cost entries.** `cost = (c = {}) => …` + `|| 0` means an
absent `Cost` becomes `{0,0,0,0}`. Verified against `.cache/aoe2-data/data.json`: **every** entry
currently has a Cost (245 units / 40 buildings / 194 techs); legit zeros exist only as a *present*
Cost (e.g. Konnik infantry, Cartography, Tracking). So an **absent** Cost is a data gap → must fail
loud. **Fix:** drop the defaulting param; collect entries where `v.Cost == null` into `missing[]`;
**throw** (before `writeFileSync`) listing `kind/id`. Present-but-empty/zero Costs still pass.

**(4b) Aggregate freshness stamps overstate.** `aggregate-{maps,matchups,rich}` write
`generated = new Date()` (run date) while reading the **frozen aoestats archive**. **Fix:** add a
`sourceDate` constant (the archive corpus month) emitted alongside `generated`; keep `generated` as
the run timestamp. *(SOURCE_DATE value is an open sourcing item — see below.)*

**(4c) Big stat JSONs bypass the prebuild gate.** Only `civilizations.json` is validated. **Fix:**
new `scripts/validate-stats.mjs` (valibot + `exit(1)`, mirroring `validate-data.mjs`) over
`civ-meta`, `map-meta`, `civ-matchups{,-by-map,-by-elo,-team}`, `public/civ-cube.json`. Assert:
required top-level keys (`source` non-empty, `generated` `YYYY-MM-DD`); per leaf stat — `games` a
positive int, `winRate` finite in **[0,100]** (verified scale), `ci95` pair in range, `tier ∈
{S,A,B,C,D}`, no NaN/null where a number is required; a **scale guard** (if every `winRate ≤ 1`, fail
"looks like 0-1 fraction"); for the cube, validate it's integer tuples `[civ,elo,map,month,games,wins]`
with in-range indices and `wins ≤ games`. **Must accept both `source` strings** (self-collected *and*
aoestats archive) — assert only that `source` is non-empty. Wire `&& node scripts/validate-stats.mjs`
into `prebuild` right after `validate-data.mjs`; add a `"validate:stats"` script.

**Tests:** `build-costs` succeeds on current data; negative fixture (deleted Cost) throws.
`validate-stats` exits 0 on committed JSONs; fixtures (fraction-scale winRate, `tier:'Z'`,
`games:null`) each fail; `pnpm build` fails when a stat JSON is corrupted.

---

## Fix 5 — civ-independent age-up conversion mis-coaches Malay · `civ-aware-ageup`

**Root cause.** `compare.rs` converts age-up **click** time to **completion** time by adding fixed
`FEUDAL_RES_S=130 / CASTLE_RES_S=160 / IMP_RES_S=190` at four sites. **Baseline confirmed sourced:**
`data.json` Tech 101/102/103 `ResearchTime` = 130/160/190 ✅ (correct for the 52 no-bonus civs). But
it's applied regardless of civ, even though the civ slug is in scope. Any age-speed civ gets its
completion time over-estimated → false "slower to age X" findings. In current in-repo data, **only
Malay** deviates: `civilizations.json` malay bonus = "Advancing to the next Age is +66% faster".

**Fix (mechanism, in-repo):** add `fn age_research_s(civ: &str) -> (f64,f64,f64)` returning the
triple, defaulting to `(130,160,190)` and overriding only for sourced age-speed civs; compute once
per player and substitute at all four call sites. A slug-keyed code constant (not a JSON file) is
right for a single deviating civ ("keep it light"); revisit a baked table if more civs are added.

**Malay factor — SOURCED (resolved 2026-06-26).** The only deviating civ in current in-repo data.
- Bonus wording: **"Advancing to the next Age is 66% faster"** (Malay civ bonus — AoE2 wiki; Liquipedia
  *Feudal Age* states "Researching Feudal Age is 66% faster").
- Base age-up research = **130/160/190s** (in-repo `.cache/aoe2-data/data.json` Tech 101/102/103
  `ResearchTime`; corroborated by Liquipedia *Feudal Age* "Research time: 2:10" = 130s).
- Application: "66% faster" is a **research-rate multiplier** (rate × 1.66 ⇒ `time = base / 1.66`),
  **not** a flat 66% time reduction. Disambiguated by the wiki's stated Malay times **Feudal 78s**
  (= 130 ÷ 1.66; a reduction reading would give 44s, which no source states) and **Castle 96s**
  (= 160 ÷ 1.66). ⇒ Malay age-up = **78.3 / 96.4 / 114.5s**.
- Encode `const MALAY_AGE_FACTOR: f64 = 1.0 / 1.66;` with the citation above in a code comment.
  *(Discarded: a "65s Imperial" search snippet — inconsistent with the ÷1.66 mechanic the Feudal/Castle
  figures establish; not used.)*

Also flag for later (not this fix): Portuguese team bonus "Technologies research +25% faster" — does
it apply to age-up? Needs confirmation; if yes it requires team-context modeling the per-player lookup
doesn't do yet.

**Tests:** `age_research_s("franks") == (130,160,190)`; `age_research_s("malay")` < baseline on all
three (once a sourced factor is set); a regression test with a Malay player whose click time breaches
+60s under 130 but not under the reduced duration asserts **no** false "Feudal up-time" finding;
existing franks tests stay green; `cargo test`.

---

## Implementation order (each fix TDD: test → implement → verify)

1. **Fix 2** (tier consolidation) — pure in-repo, no sourcing, immediate user-visible win.
2. **Fix 3** (`buckets.mjs` + null-guards) — foundational shared module + Vitest.
3. **Fix 4** (costs fail-loud + freshness + `validate-stats` gate) — in-repo + prebuild wiring.
4. **Fix 5** (civ-aware age-up) — Rust + `cargo test`; Malay factor sourced (÷1.66, see Fix 5).
5. **Fix 1** (1v1 mode) — SQL + Rust + `cargo test`; `benchmark.json` regen on VM.

## Open sourcing items (surface, don't guess)

- ~~Malay age-up factor (Fix 5)~~ — **RESOLVED**: ÷1.66 ⇒ 78.3/96.4/114.5s, cited in Fix 5.
- **`SOURCE_DATE`** (Fix 4b) — the real frozen aoestats archive month (memory says ~2026-02; confirm
  before hard-coding, since it becomes a displayed freshness fact). Non-blocking: defaults to `2026-02`
  with a `// confirm` note if unconfirmed at implementation time.
- **1v1 age-timed density** (Fix 1) — VM query confirming 1v1 cells clear `count >= 50`; informs
  whether the honest `ref_desc` degrade fires often. Confirmed during the VM regeneration step.

## Non-scope (deferred to Phase E)

Loading forward `replay_ages` into DuckDB; rebuilding benchmarks from our own apples-to-apples
replay data (resolving the completion-vs-click timing basis at the source); flank/pocket per-elo
medians. These are data-infra-blocked (crawl/load must finish first), per the agreed "defer
data-blocked" scope.

## Regeneration runbook (user runs after merge)

On the **VM** (`mkd@192.168.111.7`, DB must be unlocked — `tmux kill-session -t duckui` first):
1. `build-benchmark.sql` → `bench.csv` → `node build-benchmark.mjs` → `benchmark.json` (Fix 1).
   Verify `grep ',1v1,' bench.csv | wc -l > 0`.
2. Re-run `aggregate-{maps,matchups,rich}` (Fix 3 SQL-null + Fix 4b sourceDate) → refresh
   `map-meta.json`, `civ-matchups*.json`, `civ-meta.json`.

**Locally** (desktop crawl backup):
3. `build-civ-cube.mjs` + `refresh-{civ,map,team,matchups}-current.mjs` (Fix 3 null-guards) →
   refresh `public/civ-cube.json` + marginals.
4. `node scripts/data-pipeline/build-costs.mjs` (Fix 4a — must still succeed).
5. `pnpm validate:stats && pnpm build` — the new gate must pass.

## Verification

`pnpm check` · `pnpm test` (incl. new `buckets.test.mjs`) · `cargo test` in `replay-rs` ·
`pnpm build` (exercises `validate-stats`) · the grep gates in Fix 2/3 · visual tier-badge check.
