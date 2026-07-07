"""The ongoing recent→old replay backfill, orchestrated by Dagster and RUN AS A RESOURCE-CAPPED
DOCKER CONTAINER.

`pipeline backfill` (see `pipeline/crates/pipeline/src/backfill.rs`) walks the historical
`source='aoestats'` corpus newest-first, downloads each match's real replay from the age archive,
and UPGRADES it in place to a rich `source='replay'` row. It is resumable purely from DB state (a
`replay_backfill_misses` table + the source flip), so the container is stateless — no volume, safe
to `--rm` every run.

Unlike the other assets here (which launch the native release binaries directly via Pipes), this
one launches the DOCKERIZED binary (`pipeline/Dockerfile --build-arg BIN=pipeline`, tagged
`aoe2-pipeline:latest`) so the archive reach-back runs under HARD OS-level resource caps
(`--cpus`/`--memory`/`--pids-limit`) — the operator's "resources limited" requirement, enforced by
cgroups rather than only app-level rate limiting. The app-level bounds (gentle archive rate, low
concurrency, small ingest batches, a per-run match `--limit`) still apply INSIDE the container; the
cgroup caps are a second, independent ceiling so the reach-back can never starve the VM or the DB
container.

`DATABASE_URL` is forwarded into the container BY NAME (`docker run --env DATABASE_URL`, no
`=value`), so the secret is taken from this process's environment and NEVER appears on the command
line / in `docker inspect` — the same no-leak discipline the Rust binaries follow (a past leak
rotated the DB password). The value is supplied to the subprocess env via the `postgres` resource.

The image is built once by a deploy step, from the repo ROOT context (the shared Dockerfile embeds
repo-root refdata files — see its comments): `docker build -f pipeline/Dockerfile
--build-arg BIN=pipeline -t aoe2-pipeline:latest .`, NOT per run — this asset only runs it.
"""

from datetime import timedelta

from dagster import (
    AssetExecutionContext,
    Config,
    DefaultScheduleStatus,
    MaterializeResult,
    PipesSubprocessClient,
    RunRequest,
    ScheduleEvaluationContext,
    asset,
    define_asset_job,
    schedule,
)

from ..resources import PostgresResource
from .ingest_assets import dims

# Absolute path so the subprocess finds `docker` regardless of the launcher's PATH (the native-binary
# assets get away with a bare name only because they pass an absolute binary path themselves).
DOCKER_BIN = "/usr/bin/docker"


class ReplayBackfillConfig(Config):
    """One backfill run's knobs. Everything except `archive_floor` has a bounded default; the
    floor is REQUIRED (no default) — the operator/schedule must state the archive's current
    retention edge explicitly rather than have this silently guess a date and waste archive calls
    on guaranteed-404 matches (the pipeline's fail-loud/no-defaults rule)."""

    archive_floor: str
    """`YYYY-MM-DD` (UTC): don't attempt matches older than this. The age archive retains only a
    rolling ~12-month window, so older matches only 404. The `replay_backfill_schedule` computes a
    ROLLING floor each tick so it tracks the sliding window without manual edits."""

    image: str = "aoe2-pipeline:latest"
    """The pre-built pipeline image (`--build-arg BIN=pipeline`)."""

    limit: int = 300
    """Max matches to attempt this run — a bounded chunk; the schedule cadence, not this, drives
    total throughput."""

    rate: int = 30
    """Archive requests/min (GCRA) — deliberately gentle."""

    concurrency: int = 3
    """In-flight fetch+parse workers (also the archive HTTP client's concurrency)."""

    batch_size: int = 20
    """Matches per upgrade-ingest transaction."""

    cpus: str = "1.5"
    """`docker --cpus` cgroup cap."""

    memory: str = "1g"
    """`docker --memory` cgroup cap."""

    pids_limit: int = 256
    """`docker --pids-limit` cgroup cap."""


@asset(
    name="replay_backfill",
    deps=[dims],
    description=(
        "One recent→old backfill chunk: download old aoestats matches' replays from the age "
        "archive and UPGRADE them in place to source='replay' (build orders / APM / unit comp / "
        "tech timings). Runs as a resource-capped Docker container; resumable purely from DB state."
    ),
    compute_kind="docker",
)
def replay_backfill(
    context: AssetExecutionContext,
    config: ReplayBackfillConfig,
    pipes_subprocess_client: PipesSubprocessClient,
    postgres: PostgresResource,
) -> MaterializeResult:
    command = [
        DOCKER_BIN,
        "run",
        "--rm",
        "--cpus",
        config.cpus,
        "--memory",
        config.memory,
        "--pids-limit",
        str(config.pids_limit),
        # Forward DATABASE_URL by NAME (value stays in the subprocess env, never in argv).
        "--env",
        "DATABASE_URL",
        config.image,
        "backfill",
        "--limit",
        str(config.limit),
        "--archive-floor",
        config.archive_floor,
        "--rate",
        str(config.rate),
        "--concurrency",
        str(config.concurrency),
        "--batch-size",
        str(config.batch_size),
    ]
    context.log.info(
        "launching dockerized backfill: limit=%s archive_floor=%s cpus=%s memory=%s",
        config.limit,
        config.archive_floor,
        config.cpus,
        config.memory,
    )
    return pipes_subprocess_client.run(
        command=command,
        context=context,
        env={"DATABASE_URL": postgres.database_url},
    ).get_materialize_result()


replay_backfill_job = define_asset_job(
    "replay_backfill_job",
    selection=[replay_backfill],
    description="Runs one resource-capped recent→old backfill chunk.",
)

# The age archive's reachable window, measured 2026-07-08: replays were still served back to
# ~2025-04 (≈15-18 months), 0% by 2024-12. 540 days (~18 months) errs GENEROUS on purpose — a floor
# that's too tight permanently skips reachable matches, whereas one that's too loose only attempts a
# few guaranteed-404 tail matches ONCE each (they're then recorded in `replay_backfill_misses` and
# never retried). Recomputed each tick so it tracks the sliding retention window with no manual edits.
_ARCHIVE_RETENTION_DAYS = 540


@schedule(
    job=replay_backfill_job,
    # Hourly at :17 — one bounded `--limit` chunk per hour, a gentle perpetual reach-back, never a
    # firehose. The DB state (source flip + `replay_backfill_misses`) makes each run self-advancing.
    cron_schedule="17 * * * *",
    # Auto-start when the daemon first loads this code location (no manual UI/CLI toggle needed) —
    # the whole point of standing the daemon up is to have this running.
    default_status=DefaultScheduleStatus.RUNNING,
    description="Hourly recent→old backfill chunk with a rolling archive floor.",
)
def replay_backfill_schedule(context: ScheduleEvaluationContext) -> RunRequest:
    floor = (
        context.scheduled_execution_time.date() - timedelta(days=_ARCHIVE_RETENTION_DAYS)
    ).isoformat()
    return RunRequest(
        run_config={"ops": {"replay_backfill": {"config": {"archive_floor": floor}}}},
    )
