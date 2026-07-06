# pipeline/dbt — the civ-meta aggregation DAG

A minimal dbt project (`dbt-core 1.11.12` + `dbt-postgres`, matching `deploy/README.md`'s pinned
versions) that turns `matches`/`match_players` into the views `pipeline/crates/export` reads to
build `civ-meta.json`. Task M5a's aggregation half of the "dbt scaffold + Rust exporter" pattern —
see `.superpowers/sdd/task-M5a-export-report.md` for the full writeup.

## Models

| Model | Grain | Feeds |
|---|---|---|
| `civ_meta` | (civ, ladder, elo_bucket \| `'all'`) | `civs.<slug>.<ladder>.{games,winRate,ci95,playRate,tier}` + `.byElo` |
| `civ_meta_by_map` | (civ, ladder, map) | `.byMap` |
| `civ_meta_by_patch` | (civ, ladder, build) | `.byPatch` |
| `civ_meta_openings` | (civ, ladder, opening), ranked | `.openings` (top 3) |
| `civ_meta_ageup` | (civ, ladder) | `.ageUp` (median seconds) |
| `patch_axis` | (build), 1v1-ladder-only, global | top-level `patches` |
| `matchups_1v1` | (civ, opp), 1v1 | `civ-matchups.json` |
| `matchups_1v1_by_map` | (civ, opp, map), 1v1 | `civ-matchups-by-map.json` |
| `matchups_1v1_by_elo` | (civ, opp, elo_bucket \| `'all'`), 1v1 | `civ-matchups-by-elo.json` |
| `matchups_team` | (civ, opp), team, cross-team pairs | `civ-matchups-team.json` |
| `benchmark_ageup` | (civ, map \| `'all'`, elo_bucket \| `'all'`, mode \| `'all'`), GROUPING SETS | `benchmark.json` base cells |
| `benchmark_vils` | (civ, map \| `'all'`, elo_bucket \| `'all'`, mode \| `'all'`), WINNERS only, GROUPING SETS | `benchmark.json`'s `vils_castle` overlay |

All `materialized: view` (task M5a's whole result set is a few hundred rows per model — cheap to
recompute; see `dbt_project.yml`'s doc for why a later, much larger M5b/c model might choose
`table` instead). Every model resolves `civ_id -> slug` (and `map_id -> slug`) itself via a `civs`/
`maps` JOIN — see `pipeline/crates/export/src/lib.rs`'s doc for why the Rust side deliberately does
NOT duplicate that lookup. Per-bucket/map/patch/civ minimum-sample thresholds (the old JS
generators' `MIN_ELO`/`MIN_MAP`/etc.) are applied once, in `pipeline/crates/export/src/{civ_meta,
matchups}.rs` — not duplicated into these views. The two `benchmark_*` models are the one
exception: their `HAVING count(*) >= 50` lives IN the view, matching the old `build-benchmark*.sql`
scripts they mirror (GROUPING SETS' four/three grains are only meaningfully comparable
post-aggregation).

Task M5b (`.superpowers/sdd/task-M5b-exporters-report.md`) added the `matchups_*`/`benchmark_*`
models; `winner-refs`/`winner-comps`/`civ-cube` are deferred to M5c (see that report for why —
short version: `replay_events`/`replay_ages` are still empty in the live corpus, and
`winner-comps.json` needs a per-unit-type breakdown column `match_ages` doesn't have yet).

## Setup

```sh
cd pipeline/dbt
uv venv --python 3.12 .venv          # dbt-core doesn't yet support very new CPython releases;
                                      # 3.12 is a known-good pin, independent of the system python3.
uv pip install --python .venv/bin/python dbt-core==1.11.12 dbt-postgres
```

## Run (against the live/dev database)

dbt-postgres's `type: postgres` adapter has no raw-DSN mode, but this repo's one source of truth
for the connection is `DATABASE_URL` (git-ignored `pipeline/.env`, same variable every Rust
pipeline binary reads). `env_from_database_url.sh` derives the discrete libpq env vars
`profiles.yml` needs FROM `DATABASE_URL`, so the secret is never duplicated:

```sh
cd pipeline/dbt
set -a; source ../.env; set +a       # loads DATABASE_URL
source env_from_database_url.sh      # derives PGHOST/PGPORT/PGUSER/PGPASSWORD/PGDATABASE
.venv/bin/dbt parse            --profiles-dir .
.venv/bin/dbt run              --profiles-dir .   # all models
.venv/bin/dbt run              --profiles-dir . --select civ_meta   # just the primary model
.venv/bin/dbt test             --profiles-dir .
```

Never echoes `DATABASE_URL`/`PGPASSWORD` — if you need to double check the derived values, inspect
`PGHOST`/`PGUSER`/`PGDATABASE` only.
