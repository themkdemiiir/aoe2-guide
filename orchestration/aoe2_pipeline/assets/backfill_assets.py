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
    RunRequest,
    ScheduleEvaluationContext,
    asset,
    define_asset_job,
    schedule,
)
from dagster_docker import PipesDockerClient

from ..resources import PostgresResource
from .ingest_assets import dims


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
    pipes_docker_client: PipesDockerClient,
    postgres: PostgresResource,
) -> MaterializeResult:
    context.log.info(
        "launching dockerized backfill: limit=%s archive_floor=%s cpus=%s memory=%s",
        config.limit,
        config.archive_floor,
        config.cpus,
        config.memory,
    )
    # PipesDockerClient (not PipesSubprocessClient + `docker run`): it injects the Pipes context via
    # env AND reads the run's Pipes messages out of the container's LOG STREAM — the only channel
    # that works from a container, since a host-side message file isn't visible inside it. So the
    # Rust binary's `pipeline::Pipes` stdout messages (BackfillSummary metadata + logs) surface in
    # the Dagster UI. `env=` forwards DATABASE_URL into the container (value never on any argv);
    # `container_kwargs` carries the same cgroup caps the old `docker run --cpus/--memory/--pids`
    # flags did, plus `auto_remove` (the old `--rm`) — the container stays stateless.
    return pipes_docker_client.run(
        context=context,
        image=config.image,
        command=[
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
        ],
        env={"DATABASE_URL": postgres.database_url},
        container_kwargs={
            # NO `auto_remove`: PipesDockerClient streams the container's logs (where the Pipes
            # report_asset_materialization/closed messages ride) and only THEN `container.wait()`s —
            # auto-removing on exit races that and drops the final messages (empty UI metadata). The
            # exited container is cleaned up below instead.
            "nano_cpus": int(float(config.cpus) * 1_000_000_000),
            "mem_limit": config.memory,
            "pids_limit": config.pids_limit,
        },
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

# Per-run throughput, calibrated 2026-07-08 against the live archive. The archive's effective ceiling
# is ~20-30 matches RESOLVED/min: at rate=60/concurrency=6 it drops ~half the requests as connection
# errors (not 429s — no polite throttle, just failures) with NO net throughput gain, so pushing
# harder is pure waste. rate=40/concurrency=4 stays fast but clean; limit=1200 fills a ~50-min run
# (near-continuous, no overlap with the hourly tick). Recent-first ordering is inherent to the
# backfill's keyset walk, so this prioritizes the newest still-reachable matches. Bump `_LIMIT` only
# alongside a shorter cron / run-concurrency guard, or runs will overlap.
_LIMIT = 1200
_RATE = 40
_CONCURRENCY = 4


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
        run_config={
            "ops": {
                "replay_backfill": {
                    "config": {
                        "archive_floor": floor,
                        "limit": _LIMIT,
                        "rate": _RATE,
                        "concurrency": _CONCURRENCY,
                    }
                }
            }
        },
    )
