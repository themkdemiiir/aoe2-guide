"""Rust ingest-side assets, wrapped as external processes via Dagster Pipes
(`PipesSubprocessClient`) — upstream of the whole dbt aggregation layer (see `dbt_assets.py`'s
`RawTableUpstreamDbtTranslator`, which wires these in as extra deps on every dbt model, since
those models read the raw tables these populate directly, with no dbt `source()` indirection).

Neither binary participates in the Dagster Pipes wire protocol (they're plain Rust CLIs — see
`pipeline/docs/rust-playbook.md`; no Rust `dagster-pipes` SDK exists) — `PipesSubprocessClient`
still works for a non-participating subprocess: `get_materialize_result()` synthesizes a
`MaterializeResult` from the exit code when the process reports nothing itself, and raises if the
process exits non-zero. What we lose vs. a fully Pipes-aware subprocess is structured
in-flight metadata (row counts, custom logs) — an acceptable trade for not touching the Rust
crates' code (out of scope for this task; see the M6 brief's constraints).
"""

from dagster import (
    AssetExecutionContext,
    Config,
    MaterializeResult,
    PipesSubprocessClient,
    asset,
)

from ..paths import AOESTATS_BIN, DIMS_BIN
from ..resources import PostgresResource


@asset(
    name="dims",
    description=(
        "Idempotently loads the maps/civs/civs_relic/patch_index reference dims (Rust `dims` "
        "bin) from the committed refdata. Every matches/match_players row FK-references these, "
        "so this must run before any ingest against a fresh schema. Safe to re-materialize any "
        "time — the loader is idempotent (see pipeline/README.md's `dims` section)."
    ),
    compute_kind="rust",
)
def dims(
    context: AssetExecutionContext,
    pipes_subprocess_client: PipesSubprocessClient,
    postgres: PostgresResource,
) -> MaterializeResult:
    return pipes_subprocess_client.run(
        command=[str(DIMS_BIN)],
        context=context,
        env={"DATABASE_URL": postgres.database_url},
    ).get_materialize_result()


class AoestatsImportConfig(Config):
    archive_dir: str
    """Directory holding `m_*.parquet`/`p_*.parquet` pairs (the aoestats historical archive —
    see `.superpowers/sdd/task-M4a-aoestats-brief.md`; conventionally `~/aoestats` on the host
    that runs the import). Required, no default: per the pipeline's fail-loud/no-defaults rule,
    an operator must say explicitly where the archive lives rather than this asset silently
    guessing a path that may not exist on a given machine."""


@asset(
    name="aoestats_import",
    deps=[dims],
    description=(
        "Idempotently loads the historical aoestats parquet archive's matches/players into "
        "Postgres as `matches.source = 'aoestats'` (Rust `aoestats import --dir <archive_dir>`). "
        "This is the CURRENT sole writer of `matches`/`match_players` in the live DB — see "
        "`checks/data_quality.py`."
    ),
    compute_kind="rust",
)
def aoestats_import(
    context: AssetExecutionContext,
    config: AoestatsImportConfig,
    pipes_subprocess_client: PipesSubprocessClient,
    postgres: PostgresResource,
) -> MaterializeResult:
    return pipes_subprocess_client.run(
        command=[str(AOESTATS_BIN), "import", "--dir", config.archive_dir],
        context=context,
        env={"DATABASE_URL": postgres.database_url},
    ).get_materialize_result()
