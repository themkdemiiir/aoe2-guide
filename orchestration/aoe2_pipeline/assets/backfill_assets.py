"""The now->old replay backfill: ASSET STRUCTURE + time partitions only (per the M6 brief — the
actual node2 backfill run is a held operational step, not part of this task).

`pipeline/crates/pipeline` (bin name `pipeline`) is this repo's ALREADY-DESIGNED composition root
for the eventual discover -> download -> parse -> `to_batch` -> ingest run loop (it joins
`fetch`'s `DiscoverySeed` with `replay`'s `ParsedReplay` — see its own module doc). Today it is a
literal skeleton: no args, no network, no database, just a log line and exit 0 (see
`pipeline/crates/pipeline/src/main.rs`). That makes it a genuinely safe stand-in to wrap in Pipes
right now — materializing `replay_backfill` runs no live crawl and touches no DB, exactly as the
M6 brief requires ("a `--dry-run`/`--limit 0` or a skipped materialization is fine for
validation"). Wiring the real run loop (with real `--partition-date`/`--limit` flags this asset's
`extras` already documents the intent for) into that binary is a follow-on Rust milestone — NOT
part of this task, which is scoped to code + local validation only (no Rust crate changes, no
live crawl; see the M6 brief's constraints).

Do not expand this asset into an actual crawl trigger without that binary existing first, and
never materialize it against the live Relic API from this project's validation gates.
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
        "matches.source = 'replay'). STRUCTURAL ONLY today — see module doc: the Rust binary "
        "this Pipes-invokes is an inert skeleton. Do not materialize against the live Relic API "
        "from this task's gates."
    ),
    compute_kind="rust",
)
def replay_backfill(
    context: AssetExecutionContext,
    pipes_subprocess_client: PipesSubprocessClient,
    postgres: PostgresResource,
) -> MaterializeResult:
    return pipes_subprocess_client.run(
        command=[str(PIPELINE_BIN)],
        context=context,
        # The skeleton binary ignores these — they document the FUTURE real CLI's shape (the
        # run loop this asset will drive once `pipeline/crates/pipeline`'s main.rs is built out).
        extras={"partition_date": context.partition_key},
        env={"DATABASE_URL": postgres.database_url},
    ).get_materialize_result()
