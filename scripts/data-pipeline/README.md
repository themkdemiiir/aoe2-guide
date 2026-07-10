# scripts/data-pipeline — legacy crawl-derived JSON generators

This directory is **not** part of the Cloudflare Pages static build — nothing here
runs during `pnpm build`. The full VM-side data workload (crawl ingestion,
replay parsing, aggregation) now lives in [`pipeline/`](../../pipeline) as a
Rust + Dagster + PostgreSQL stack; see [`docs/rust-migration-plan.md`](../../docs/rust-migration-plan.md)
for the authoritative status of that migration.

What's left here is **3 generators** that read frozen crawl-NDJSON snapshots
(`data-cache/`, not committed) directly and write a few `public/*.json` files
the client fetches lazily. They are not scheduled by Dagster — run by hand on
the box that holds `data-cache/` when those outputs need a refresh:

| Script | Output | Reads |
|---|---|---|
| `build-civ-cube.mjs` | `public/civ-cube.json` + `src/data/civ-cube-dims.json` | crawl NDJSON via `lib/crawl-stream.mjs`, replay-verified map truth, `build-cube-history.sql` history |
| `build-winner-comps.mjs` | `public/winner-comps.json` | CSV produced by `build-winner-comps.sql` |
| `build-winner-refs.mjs` | `public/winner-refs.json` | 3 CSVs produced by `build-winner-refs.sql` + `build-winner-meds.sql` |

Shared `lib/`:

- `buckets.mjs` — ELO bucketing (`eloBucket`) + map canonicalization (`canonMap`); the single source both the JS generators and the paired `.sql` files must stay in sync with.
- `crawl-stream.mjs` — reads all crawl NDJSON sources (deduped by `match_id`), applying the correctness gates from `relic-map.mjs` up front.
- `guide-civs.mjs` — the guide's civ slug set, from `src/data/civilizations.json`.
- `patch-axis.mjs` — crawl record → real game build ("Update 179158"), from `src/data/patch-index.json`.
- `relic-map.mjs` — see the crawl-record gates below.

**Status:** these JSON outputs are currently **frozen** — the crawl NDJSON snapshots they
read no longer grow (ingestion now writes straight to PostgreSQL via `pipeline/`), so
re-running these scripts reproduces the same frozen slice rather than picking up new
matches. `build-winner-comps` also has a Rust/dbt port already built
(`pipeline/dbt/models/winner_comps.sql` + `pipeline/crates/export/src/winner_comps.rs`)
that is **pending cutover** — once it and the other two generators are ported, this
directory goes away. Full roadmap: [`docs/rust-migration-plan.md`](../../docs/rust-migration-plan.md).

## The three crawl-record gates (2026-07 correctness fix)

Every consumer of crawl NDJSON goes through `lib/relic-map.mjs` +
`lib/crawl-stream.mjs`, which enforce, in order:

1. **Ranked RM only** — the record's `ladder` field is Relic's `matchtype_id`;
   only 6 (1v1 RM) and 7/8/9 (team RM) count. The crawl also captures Empire
   Wars, DM, quickplay, and event AUTOMATCH games — those must never enter RM
   stats.
2. **Current id-space era only** — the API's `civilization_id` is **Relic's own
   civ enumeration** (NOT the game/replay `civ_id` space) and it RE-SHUFFLES at
   some DLC/patch boundaries. `src/data/relic-civ-id-map.json` (provenance
   inside) is only valid from its `validFrom` (2025-09-01); older matches are
   excluded rather than mislabeled.
3. **Fail-loud civ mapping** — `relicCivSlug()` throws on an unknown id (a new
   DLC shifting the table must break the regen, not silently vanish).

**Maps:** the API's per-match `mapname` is wrong for ~57% of matches (measured
against 133k replay-parsed matches). Per-match map truth for `build-civ-cube.mjs`
comes only from replay-parsed data (`loadReplayMapTruth()`); matches without a
parsed replay contribute no map slice.
