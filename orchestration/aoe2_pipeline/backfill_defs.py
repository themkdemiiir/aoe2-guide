"""Focused Dagster code location for running JUST the replay backfill on its hourly schedule.

`definitions.py` is the FULL pipeline (dbt aggregation DAG + ingest + export + backfill), but its
`@dbt_assets` require a generated `pipeline/dbt/target/manifest.json` (+ the dbt venv) to even
IMPORT. The backfill needs none of that — so this code location wires ONLY the backfill asset, its
`dims` lineage dep, the auto-running hourly schedule, and the two resources the backfill uses. It
imports cleanly with just `dagster` + `psycopg` installed, so the daemon can run the backfill on the
VM without first standing up the whole dbt aggregation layer.

Run the daemon (schedules fire) + a UI on :3000, detached, with a persistent DAGSTER_HOME and
`DATABASE_URL` in the environment (the schedule provides `archive_floor`; the asset launches the
resource-capped `aoe2-pipeline:latest` container — see `assets/backfill_assets.py`):

    set -a; source ~/aoe2-pipeline/pipeline/.env; set +a
    DAGSTER_HOME=~/dagster_home .venv/bin/dagster dev -m aoe2_pipeline.backfill_defs

This Dagster process is only the CONDUCTOR; all backfill work runs inside Docker. When the full dbt
layer is later deployed on the VM, switch the daemon over to `aoe2_pipeline.definitions`.
"""

from dagster import Definitions, EnvVar, PipesSubprocessClient
from dagster_docker import PipesDockerClient

from .assets.backfill_assets import (
    replay_backfill,
    replay_backfill_job,
    replay_backfill_schedule,
)
from .assets.ingest_assets import dims
from .resources import PostgresResource

defs = Definitions(
    # `dims` is included only as the backfill's declared lineage dep (already loaded in the live DB);
    # the schedule's job selects `replay_backfill` alone, so `dims` is never auto-materialized here.
    assets=[dims, replay_backfill],
    jobs=[replay_backfill_job],
    schedules=[replay_backfill_schedule],
    resources={
        "postgres": PostgresResource(database_url=EnvVar("DATABASE_URL")),
        # replay_backfill launches its container via PipesDockerClient (reads Pipes messages from
        # container logs); `dims` (lineage dep) still uses the plain subprocess client.
        "pipes_docker_client": PipesDockerClient(),
        "pipes_subprocess_client": PipesSubprocessClient(),
    },
)
