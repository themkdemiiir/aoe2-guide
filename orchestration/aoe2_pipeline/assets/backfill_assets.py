"""The now->old replay backfill: ASSET STRUCTURE + time partitions only — the actual node2
backfill run remains a held operational step, not part of this task.

`pipeline/crates/pipeline` (bin name `pipeline`) now HAS a real `crawl` subcommand (the M6
discover -> download -> parse -> `to_batch` -> ingest run loop — see
`pipeline/crates/pipeline/src/crawl.rs`'s module doc). This asset invokes it with `--dry-run
--limit 0` and NO `--profile-id`: per `CrawlConfig`'s doc, omitting `--profile-id` skips discovery
entirely (no Relic API call at all, not even a mocked one), and `--dry-run` additionally skips
download/parse/ingest — so materializing `replay_backfill` still touches neither the live API nor
Postgres, exactly as this asset's validation gates require.

That also means this call is validation-only: `crawl` discovers by PROFILE, not by date, so this
asset's per-day `DailyPartitionsDefinition` has no value to feed it yet. Wiring a REAL per-partition
backfill (a profile-id source, or a date-scoped discovery strategy over the age-archive fallback)
is a follow-on task — not this one.

Never materialize this asset against the live Relic API from this project's validation gates.
"""

from dagster import (
    AssetExecutionContext,
    DailyPartitionsDefinition,
    MaterializeResult,
    PipesSubprocessClient,
    asset,
)

from ..paths import PIPELINE_BIN
from ..resources import PostgresResource
from .ingest_assets import dims

# AOE2:DE released 2019-11-14 (public record, not a pipeline-derived stat) — the earliest date a
# ranked match this pipeline cares about could exist, so it anchors the backfill's oldest
# partition. `DailyPartitionsDefinition` only defines the calendar; node2's actual backfill launch
# picks the now->old traversal order (a backfill-launch concern, not a partition-definition one).
REPLAY_BACKFILL_START_DATE = "2019-11-14"

replay_backfill_partitions = DailyPartitionsDefinition(start_date=REPLAY_BACKFILL_START_DATE)


@asset(
    name="replay_backfill",
    partitions_def=replay_backfill_partitions,
    deps=[dims],
    description=(
        "One day's discover -> download -> parse -> ingest replay run (would write "
        "matches.source = 'replay'). VALIDATION ONLY today — see module doc: `crawl` discovers "
        "per-PROFILE, not per-date, so this asset's `--dry-run --limit 0` call (no --profile-id) "
        "proves the binary + Pipes wiring without a real per-partition backfill yet. Do not "
        "materialize against the live Relic API from this task's gates."
    ),
    compute_kind="rust",
)
def replay_backfill(
    context: AssetExecutionContext,
    pipes_subprocess_client: PipesSubprocessClient,
    postgres: PostgresResource,
) -> MaterializeResult:
    return pipes_subprocess_client.run(
        # No `--profile-id` + `--dry-run`: per `CrawlConfig`'s doc this skips discovery AND
        # download/parse/ingest, so this call never touches the live Relic API or Postgres (see
        # the module doc for why a real per-partition backfill needs a follow-on task first).
        command=[str(PIPELINE_BIN), "crawl", "--dry-run", "--limit", "0"],
        context=context,
        # `extras`/`env` document the FUTURE real invocation's shape (the run loop this asset will
        # drive with a real `--profile-id`/`--limit` once a per-partition profile/date source
        # exists) — the dry-run call above ignores both.
        extras={"partition_date": context.partition_key},
        env={"DATABASE_URL": postgres.database_url},
    ).get_materialize_result()
