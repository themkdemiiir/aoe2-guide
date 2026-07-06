# orchestration — Dagster DAG for the AOE2 guide's offline pipeline

Conducts the whole pipeline: `fetch/replay (backfill) -> ingest (dims/aoestats) -> dbt (aggregate)
-> export (Rust)`. Library-first: [Dagster](https://docs.dagster.io/) +
[`dagster-dbt`](https://docs.dagster.io/integrations/libraries/dbt/dagster-dbt) for the aggregation
DAG, [Dagster Pipes](https://docs.dagster.io/concepts/dagster-pipes) (`PipesSubprocessClient`) to
wrap the compiled Rust binaries — no hand-rolled scheduler.

**Scope of this project as it stands**: code + local validation (Task M6). The node2 deployment
and the live Relic crawl are held operator steps — see "What's left for node2" below. Nothing here
executes a live crawl.

## Layout

| Path | Purpose |
|---|---|
| `aoe2_pipeline/definitions.py` | Entry point (`Definitions`) — assets, checks, resources |
| `aoe2_pipeline/resources.py` | `PostgresResource` (the one `DATABASE_URL` -> connection/env path) |
| `aoe2_pipeline/paths.py` | Pointers to `pipeline/dbt` + the Rust release binaries |
| `aoe2_pipeline/env.py` | `DATABASE_URL` -> `PGHOST`/`PGPORT`/... (mirrors `pipeline/dbt/env_from_database_url.sh`) |
| `aoe2_pipeline/assets/dbt_assets.py` | `pipeline/dbt`'s 12 models via `@dbt_assets` + a translator adding the real (non-dbt) upstream deps |
| `aoe2_pipeline/assets/ingest_assets.py` | `dims` + `aoestats_import` — Pipes-wrapped, upstream of dbt |
| `aoe2_pipeline/assets/backfill_assets.py` | `replay_backfill` — the now->old partitioned backfill's asset STRUCTURE (placeholder body; see below) |
| `aoe2_pipeline/assets/export_assets.py` | `export_civ_meta` / `export_matchups` / `export_benchmark` — Pipes-wrapped `export` bin, downstream of the matching dbt models |
| `aoe2_pipeline/checks/data_quality.py` | `@asset_check`: no gameplay match missing map/elo |

## Setup

```sh
cd orchestration
uv venv --python 3.12 .venv                 # same uv/py3.12 pin M5a used for dbt (system py3.14 is too new for dbt-core)
uv pip install --python .venv/bin/python -e ".[dev]"

# Build the Rust bins Pipes invokes (gate 4):
cd ../pipeline && cargo build --release --workspace

# Generate pipeline/dbt's manifest.json (dagster-dbt reads this static file — see dbt_assets.py's
# doc for why we deliberately don't call `DbtProject.prepare_if_dev()` at import time):
cd dbt
set -a; source ../.env; set +a
source env_from_database_url.sh
.venv/bin/dbt parse --profiles-dir .
```

## Run / validate

`DATABASE_URL` (git-ignored `pipeline/.env`) is the ONLY source of the connection — source it
before any command that touches the live DB. `dagster definitions validate` deliberately does
**not** need it (see `definitions.py`'s guarded `derive_pg_env` call and `dbt_assets.py`'s "no
`prepare_if_dev`" note) since it only loads the already-generated `manifest.json`:

```sh
cd orchestration

# Gate 1 — defs load + asset graph resolves, no DB required:
.venv/bin/dagster definitions validate -m aoe2_pipeline.definitions

# Gate 2 — materialize the dbt aggregation DAG + one export asset against the LIVE DB:
set -a; source ../pipeline/.env; set +a
.venv/bin/dagster asset materialize -m aoe2_pipeline.definitions \
  --select "civ_meta,civ_meta_by_map,civ_meta_by_patch,civ_meta_openings,civ_meta_ageup,patch_axis,matchups_1v1,matchups_1v1_by_map,matchups_1v1_by_elo,matchups_team,benchmark_ageup,benchmark_vils,export_civ_meta" \
  --config-json '{"ops": {"export_civ_meta": {"config": {"out_dir": "/tmp/civ-meta-out"}}}}'

# Interactive UI (optional, needs the `dev` extra):
.venv/bin/dagster dev -m aoe2_pipeline.definitions
```

`export_matchups`/`export_benchmark` take the same `--out`-directory config shape. **Never** pass
`src/data` (or any committed-JSON path) as `out_dir` — see each export asset's docstring.

### The ingest/backfill assets are NOT materialized by the gates above

- `dims` is a safe, idempotent refdata loader — but isn't part of the dbt->export chain this
  task's gate 2 exercises, so it's left for a deliberate operator run.
- `aoestats_import` requires an explicit `archive_dir` config (no default, by design — see its
  docstring) pointing at a local `m_*.parquet`/`p_*.parquet` archive; this task does not assume one
  exists on every machine that clones this repo.
- `replay_backfill` is a **structural placeholder** — see `assets/backfill_assets.py`'s module doc.
  Its Pipes target (`pipeline/crates/pipeline`'s `pipeline` bin) is today an inert skeleton (no
  network, no DB). Materializing it is harmless (it just logs and exits), but it accomplishes
  nothing yet and is not part of any required gate.

### Data-quality check

`no_gameplay_match_missing_map_or_elo` is attached to `aoestats_import` (today's sole writer of
`matches`/`match_players`) but queries the tables directly, so it validates the live DB regardless
of which asset last wrote to it. It runs automatically whenever `aoestats_import` materializes; to
exercise it standalone against the live DB (as this task's gate 3 did) without re-running the
importer, invoke the check function directly via Dagster's own test utility:

```python
import os
import dagster as dg
from aoe2_pipeline.checks.data_quality import no_gameplay_match_missing_map_or_elo
from aoe2_pipeline.resources import PostgresResource

pg = PostgresResource(database_url=os.environ["DATABASE_URL"])
ctx = dg.build_asset_check_context(resources={"postgres": pg})
result = no_gameplay_match_missing_map_or_elo(ctx)
assert result.passed, result.metadata
```

## Resources

- **`postgres`** (`PostgresResource`): the one `DATABASE_URL` -> connection path, used both by the
  data-quality check and (via `.database_url`) fed into every Pipes-wrapped Rust asset's `env=`.
  Config is `dagster.EnvVar("DATABASE_URL")` — resolved at run-launch time, not at defs-load time,
  so `dagster definitions validate` never needs the secret.
- **`dbt`** (`DbtCliResource`): points at `pipeline/dbt`, with `dbt_executable` set to
  `pipeline/dbt/.venv/bin/dbt` — that venv already has `dbt-postgres` installed (task M5a); this
  project's own venv deliberately does not install it a second time.
- **`pipes_subprocess_client`** (`PipesSubprocessClient`): the stock, un-customized Dagster Pipes
  subprocess runner used by every Rust-bin asset.

## Metadata store

This task's local validation uses Dagster's default (ephemeral/local, sqlite-backed `$DAGSTER_HOME`)
instance — sufficient for a single-operator, hobby-scale project with no schedules/sensors yet. If
node2 later runs a persistent `dagster-daemon` (schedules driving the backfill, sensors, etc.),
point `DAGSTER_HOME/dagster.yaml` at the same Postgres via `dagster-postgres`'s storage config —
not needed for this task.

## Versions (pinned exactly — see `pyproject.toml`)

| Package | Version | Note |
|---|---|---|
| `dagster` | `1.13.12` | matches `deploy/README.md`'s pin |
| `dagster-dbt` | `0.29.12` | the version that resolves against `dagster==1.13.12` — `dagster-dbt` does not share dagster core's version line |
| `dagster-pipes` | `1.13.12` | pulled in transitively by `dagster` |
| `dagster-webserver` | `1.13.12` | dev extra only, for `dagster dev` |
| `psycopg[binary]` | `3.3.4` | the data-quality check's DB client |

`dbt-core`/`dbt-postgres` are NOT installed in this project's venv — `pipeline/dbt/.venv` (task
M5a) already has them; see `dbt_executable` above.

## What's left for node2 (held, out of scope for this task)

1. **Deploy**: add a `dagster` service block to `deploy/docker-compose.yml` (the file's own comment
   already flags this as an M5/M6 follow-up) — build from `pipeline/Dockerfile` or a small
   dedicated image bundling this `orchestration/` project + a `dagster-daemon` for schedules.
2. **The real discover -> download -> parse -> ingest run loop**: `pipeline/crates/pipeline`'s
   `pipeline` bin needs to become a real CLI (discover via `fetch`, parse via `replay`, join via
   `pipeline::to_batch`, load via `ingest`) with `--partition-date`/`--limit`/`--dry-run` flags —
   a Rust milestone, not a Dagster one. `assets/backfill_assets.py` already documents the exact
   shape it expects.
2b. **Launch the actual backfill**: once that bin exists, launch a `replay_backfill` backfill over
   `replay_backfill_partitions` in now->old order (`DailyPartitionsDefinition` only defines the
   calendar — see the asset's module doc) — an operator action via the Dagster UI/CLI, not code.
3. **A real `aoestats_import` run** (if the node2 box holds the full historical archive) with its
   actual `archive_dir` — this task never assumes one exists locally.
4. Optionally: point `DAGSTER_HOME` at a persistent instance (Postgres-backed via
   `dagster-postgres`) if node2 runs schedules/sensors continuously.

None of the above touched any Rust crate's code, `pipeline/dbt`'s SQL, the site, migrations, or
committed JSON — this task's diff is scoped to `orchestration/**` only.
