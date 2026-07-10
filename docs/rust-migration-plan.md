# JS → Rust pipeline migration plan

Goal: all data/pipeline generation in strict Rust; retire the JS/DuckDB pipeline. Derived from a
full per-script assessment (2026-07). This is the execution roadmap — check items off as they land.

## Already in Rust (done — nothing to do)

`civ-meta.json`, `map-meta.json`, all 4 `civ-matchups*.json`, `benchmark*.json`, and the
crawl→ingest→aggregate chain are produced by existing dbt views + the `export` crate + `pipeline crawl`.
The old `refresh-*` / `aggregate-*` / `collect-relic` / `build-benchmark*` scripts are **superseded**.

## Two tracks

The work splits by whether it touches the **live `scripts/data-pipeline/sweep.sh` cron** (still
collecting ranked data into DuckDB on the VM every ~3h).

### Track A — gated on the pipeline cutover (deploy Rust crawl → retire `sweep.sh`)

Deleting the superseded JS and porting the 3 DB generators is only safe **after** the Rust crawl
replaces `sweep.sh` as the live collector. Order:

1. **Cutover deploy** ⛔ *(gate: live production change)* — Dagster runs `pipeline crawl` as the sole
   collector; wire the Rust `export` as the JSON refresh; retire `sweep.sh`.
2. **Delete superseded JS** (post-cutover): `collect-relic`, `aggregate-{maps,patches,rich}`,
   `refresh-{civ,map,matchups,team}-current`, `build-benchmark{,-vils}`, then `stream-relic` +
   `ingest-stream` (the live cron pair). Sweep orphaned `lib/*` after.
3. **Port 3 DB generators** to dbt view + `export` subcommand (mirror `benchmark_ecotech`):
   - `build-winner-comps` → `winner-comps.json` *(medium)*
   - `build-winner-refs` → `winner-refs.json` *(large; 3 views: openings, ecotech, meds)*
   - `build-civ-cube` → `civ-cube.json` + `-dims.json` *(large; +crawl-recent merge decision)*
4. **Port cron audits** (cutover prep, run in parallel with JS before flipping): `check-patch-axis`
   (→ dbt `patch_axis` join), `backfill-map-current` (→ fold into Rust ingest).

### Track B — independent (safe to port anytime, not gated)

Static-game-data build tools → strict Rust binaries (new `pipeline/crates/refdata`). Each reads a
pinned-SHA source (commit it to `reference-data/`), writes to a `--out` dir (never `src/data/`
directly — Astro reads the committed file), and matches the JS output shape exactly.

- `build-units` → `unit-stats.json` *(small)* ← **starting here**
- `build-game-facts` → `game-facts.json` *(small; shares `units.csv`)* ← with units
- `build-unit-tech-names` → `unit-names.json` + `tech-names.json` *(medium)*
- `build-icon-map` → `icon-map.json` *(medium)*
- `build-civilizations`, `build-unit-descriptions` *(large — tight Astro coupling; Rust emits JSON,
  a thin JS wrapper still stitches the YAML)* — do last, or leave if content ownership stays in Astro.

## Keep as JS (not worth porting)

`audit-yaml-translations`, `check-translations`, `i18n-coverage` (pnpm build/CI gates),
`import-from-md` (dev scaffold), `build-og` (satori/resvg image gen), `build-llms` (Astro-content
index). These are Astro/pnpm build glue, not data pipeline.

## Landmines

- `relic-civ-map.test.mjs` guards the two-civ-id-space bug (real prod incident) — keep an equivalent
  guard before deleting `lib/relic-map.mjs`.
- `role-tr.mjs` is dead but has a test — delete both together or neither (else CI reddens).
- `aoe2-sources.mjs` holds pinned SHAs used by 5 scripts — keep it authoritative until the last JS
  consumer is ported (don't create two drifting SHA sources).
- `costs.json` — ✅ done: static data, moved to `reference-data/`, `build-costs.mjs` deleted.

## Confirmation gates (⛔ — need explicit go each time)

1. **Cutover deploy** — live production data-pipeline change on the VM.
2. **Push to main** — live site deploy (run the full CI gate first).
3. **History force-push** — irreversible purge of `MONETIZATION_PLAN.md` (+ history-only files).
