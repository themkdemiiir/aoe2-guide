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

All `materialized: view` (task M5a's whole result set is a few hundred rows per model — cheap to
recompute; see `dbt_project.yml`'s doc for why a later, much larger M5b/c model might choose
`table` instead). Every model resolves `civ_id -> slug` itself via a `civs` JOIN — see
`pipeline/crates/export/src/lib.rs`'s doc for why the Rust side deliberately does NOT duplicate
that lookup. Per-bucket/map/patch/civ minimum-sample thresholds (the old JS generators'
`MIN_ELO`/`MIN_MAP`/etc.) are applied once, in `pipeline/crates/export/src/civ_meta.rs` — not
duplicated into these views.

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
