"""The FRESHNESS crawl — captures the LATEST ranked games, orchestrated by Dagster as a
resource-capped Docker container.

Complements the backfill (which reaches BACKWARD over old aoestats matches via the age archive):
this reaches FORWARD, seeding from the top of the ranked ladder (`getLeaderBoard2`) → each profile's
recent matches (`getRecentMatchHistory`) → the FAST-path replay (`getReplayFiles`) → parse → ingest
as a rich `source='replay'` row. It's the only path that captures games newer than aoestats' end
(2026-02); the backfill can't reach them.

Unlike the backfill (stateless), the crawl keeps a small SQLite MANIFEST for resumability — so it
skips matches it already crawled instead of re-downloading them every run. That manifest is the one
piece of state, kept on a HOST VOLUME (`--manifest /state/...`, bind-mounted) so it survives the
`--rm`-style ephemeral container across runs. Everything else mirrors the backfill asset:
PipesDockerClient (Pipes metadata from the container log stream), cgroup resource caps via
`container_kwargs`, DATABASE_URL forwarded by name.
"""

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


class LatestCrawlConfig(Config):
    """One freshness-crawl run's knobs. Bounded defaults tuned for the fast path (~100/min, far
    gentler-to-hit than the archive)."""

    leaderboard_id: int = 3
    """Relic leaderboard: 3 = 1v1 RM, 4 = team RM."""

    profiles: int = 200
    """How many top ladder profiles to seed from (each contributes its recent matches, deduped)."""

    limit: int = 500
    """Max matches to download+ingest this run — ample headroom over the games the top-N players
    play in one schedule interval; the rest are already-crawled (manifest-skipped)."""

    rate: int = 100
    """getReplayFiles/getRecentMatchHistory requests/min (the fast path's documented ceiling)."""

    concurrency: int = 4
    """In-flight per-match workers."""

    image: str = "aoe2-pipeline:latest"
    manifest_dir: str = "/home/mkd/crawl-state"
    """HOST dir bind-mounted to `/state` for the resumable SQLite manifest (must exist on the host)."""

    cpus: str = "1.5"
    memory: str = "1g"
    pids_limit: int = 256


@asset(
    name="latest_crawl",
    deps=[dims],
    description=(
        "One freshness-crawl chunk: top ranked-ladder profiles → their recent matches → fast-path "
        "replay → ingest as source='replay'. Captures the LATEST games (newer than aoestats). Runs "
        "as a resource-capped Docker container; resumable via a host-volume SQLite manifest."
    ),
    compute_kind="docker",
)
def latest_crawl(
    context: AssetExecutionContext,
    config: LatestCrawlConfig,
    pipes_docker_client: PipesDockerClient,
    postgres: PostgresResource,
) -> MaterializeResult:
    context.log.info(
        "launching freshness crawl: leaderboard=%s profiles=%s limit=%s",
        config.leaderboard_id,
        config.profiles,
        config.limit,
    )
    manifest = f"/state/crawl-{config.leaderboard_id}.sqlite"
    return pipes_docker_client.run(
        context=context,
        image=config.image,
        command=[
            "crawl",
            "--leaderboard",
            str(config.leaderboard_id),
            "--profiles",
            str(config.profiles),
            "--limit",
            str(config.limit),
            "--rate",
            str(config.rate),
            "--concurrency",
            str(config.concurrency),
            "--manifest",
            manifest,
            # No raw-replay archiving for the freshness crawl — the parsed rows are in Postgres; the
            # raw corpus is a separate concern.
            "--no-raw",
        ],
        env={"DATABASE_URL": postgres.database_url},
        container_kwargs={
            # Same rationale as the backfill: no auto_remove (races the Pipes log reader); resource
            # caps via cgroups; plus the manifest host volume (the crawl's only persistent state).
            "nano_cpus": int(float(config.cpus) * 1_000_000_000),
            "mem_limit": config.memory,
            "pids_limit": config.pids_limit,
            "volumes": {config.manifest_dir: {"bind": "/state", "mode": "rw"}},
            # Run as the HOST user (uid 1000) that owns `manifest_dir`, not the image's `app` user
            # (10001) — otherwise the container can't open/create the SQLite manifest on the mount.
            "user": "1000:1000",
        },
    ).get_materialize_result()


latest_crawl_job = define_asset_job(
    "latest_crawl_job",
    selection=[latest_crawl],
    description="Runs one resource-capped freshness-crawl chunk.",
)


# Both ranked ladders run every 30 min, OFFSET 15 min from each other (1v1 at :00/:30, team at
# :15/:45), so the two crawls never run concurrently — each container has its own FetchClient rate
# limit, and staggering keeps their combined load under Relic's fast-path ceiling. They also write
# SEPARATE manifests (`crawl-3.sqlite` / `crawl-4.sqlite` — the asset keys the path on
# `leaderboard_id`), so there's no SQLite contention even if a run ever overruns its slot.


@schedule(
    job=latest_crawl_job,
    cron_schedule="*/30 * * * *",
    default_status=DefaultScheduleStatus.RUNNING,
    description="Every-30-min freshness crawl of the 1v1 ranked ladder.",
)
def latest_crawl_schedule(context: ScheduleEvaluationContext) -> RunRequest:
    return RunRequest(
        run_config={"ops": {"latest_crawl": {"config": {"leaderboard_id": 3}}}},
    )


@schedule(
    job=latest_crawl_job,
    cron_schedule="15,45 * * * *",
    default_status=DefaultScheduleStatus.RUNNING,
    description="Every-30-min freshness crawl of the TEAM ranked ladder (offset 15 min from 1v1).",
)
def latest_crawl_team_schedule(context: ScheduleEvaluationContext) -> RunRequest:
    return RunRequest(
        run_config={"ops": {"latest_crawl": {"config": {"leaderboard_id": 4}}}},
    )
