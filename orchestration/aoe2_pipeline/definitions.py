"""Dagster entry point.

    dagster definitions validate -m aoe2_pipeline.definitions
    dagster dev                  -m aoe2_pipeline.definitions
    dagster asset materialize    -m aoe2_pipeline.definitions --select ...

See `../README.md` for the full setup/run/validate/materialize commands (especially: source
`pipeline/.env` first — every DB-backed resource here reads `DATABASE_URL` from the environment
and nowhere else).
"""

import os

from dagster import Definitions, EnvVar, PipesSubprocessClient
from dagster_dbt import DbtCliResource

from .assets.backfill_assets import replay_backfill
from .assets.dbt_assets import civ_meta_dbt_assets
from .assets.export_assets import export_benchmark, export_civ_meta, export_matchups
from .assets.ingest_assets import aoestats_import, dims
from .checks.data_quality import no_gameplay_match_missing_map_or_elo
from .env import derive_pg_env
from .paths import DBT_EXECUTABLE, DBT_PROJECT_DIR
from .resources import PostgresResource

# dbt-postgres's `type: postgres` profile has no raw-DSN mode (see pipeline/dbt/profiles.yml) —
# derive the discrete PGHOST/PGPORT/PGUSER/PGPASSWORD/PGDATABASE vars its `env_var(...)` calls
# need FROM DATABASE_URL, exactly like `pipeline/dbt/env_from_database_url.sh` does for a plain
# shell `dbt` invocation (see env.py's doc). Guarded on DATABASE_URL being present so `dagster
# definitions validate` — which never touches the DB — still loads clean without it sourced.
if "DATABASE_URL" in os.environ:
    os.environ.update(derive_pg_env(os.environ["DATABASE_URL"]))

# Metadata store: the default (an ephemeral/local $DAGSTER_HOME sqlite instance) is what this
# task's local validation uses — the M6 brief allows "our Postgres... or the default", and a
# hobby-scale, single-operator orchestration project has no need for a shared Postgres-backed
# Dagster instance yet. If node2 later runs a persistent daemon (schedules/sensors/multi-user),
# revisit via `dagster-postgres`'s `DAGSTER_PG_DB` storage config in `dagster.yaml`.
defs = Definitions(
    assets=[
        civ_meta_dbt_assets,
        dims,
        aoestats_import,
        replay_backfill,
        export_civ_meta,
        export_matchups,
        export_benchmark,
    ],
    asset_checks=[no_gameplay_match_missing_map_or_elo],
    resources={
        "postgres": PostgresResource(database_url=EnvVar("DATABASE_URL")),
        "pipes_subprocess_client": PipesSubprocessClient(),
        "dbt": DbtCliResource(
            project_dir=str(DBT_PROJECT_DIR),
            profiles_dir=str(DBT_PROJECT_DIR),
            dbt_executable=str(DBT_EXECUTABLE),
        ),
    },
)
