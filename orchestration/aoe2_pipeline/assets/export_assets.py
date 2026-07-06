"""Rust `export` bin subcommands, wrapped as external processes via Dagster Pipes — one asset per
subcommand (each writes a small group of JSON files in one pass; see
`pipeline/crates/export/src/main.rs`), downstream of the matching dbt models per
`pipeline/dbt/README.md`'s model->file table.

Like `ingest_assets.py`'s wraps, `export` doesn't speak the Dagster Pipes wire protocol — see that
module's doc for why `get_materialize_result()` still works.
"""

from dagster import (
    AssetExecutionContext,
    AssetKey,
    Config,
    MaterializeResult,
    PipesSubprocessClient,
    asset,
)

from ..paths import EXPORT_BIN
from ..resources import PostgresResource

CIV_META_DBT_DEPS = [
    AssetKey("civ_meta"),
    AssetKey("civ_meta_by_map"),
    AssetKey("civ_meta_by_patch"),
    AssetKey("civ_meta_openings"),
    AssetKey("civ_meta_ageup"),
    AssetKey("patch_axis"),
]
MATCHUPS_DBT_DEPS = [
    AssetKey("matchups_1v1"),
    AssetKey("matchups_1v1_by_map"),
    AssetKey("matchups_1v1_by_elo"),
    AssetKey("matchups_team"),
]
BENCHMARK_DBT_DEPS = [
    AssetKey("benchmark_ageup"),
    AssetKey("benchmark_vils"),
]


class ExportOutConfig(Config):
    out_dir: str
    """Directory to write the exported JSON into. Required, no default — NEVER `src/data` (the
    M5a/M5b/M6 briefs all keep the committed file untouched); the operator names the real target
    explicitly every time."""


@asset(
    name="export_civ_meta",
    deps=CIV_META_DBT_DEPS,
    description=(
        "Writes civ-meta.json from the civ_meta*/patch_axis dbt views (Rust `export civ-meta "
        "--out <dir>`)."
    ),
    compute_kind="rust",
)
def export_civ_meta(
    context: AssetExecutionContext,
    config: ExportOutConfig,
    pipes_subprocess_client: PipesSubprocessClient,
    postgres: PostgresResource,
) -> MaterializeResult:
    return pipes_subprocess_client.run(
        command=[str(EXPORT_BIN), "civ-meta", "--out", config.out_dir],
        context=context,
        env={"DATABASE_URL": postgres.database_url},
    ).get_materialize_result()


@asset(
    name="export_matchups",
    deps=MATCHUPS_DBT_DEPS,
    description=(
        "Writes civ-matchups.json / civ-matchups-by-map.json / civ-matchups-by-elo.json / "
        "civ-matchups-team.json from the matchups_* dbt views in one pass (Rust `export "
        "matchups --out <dir>`)."
    ),
    compute_kind="rust",
)
def export_matchups(
    context: AssetExecutionContext,
    config: ExportOutConfig,
    pipes_subprocess_client: PipesSubprocessClient,
    postgres: PostgresResource,
) -> MaterializeResult:
    return pipes_subprocess_client.run(
        command=[str(EXPORT_BIN), "matchups", "--out", config.out_dir],
        context=context,
        env={"DATABASE_URL": postgres.database_url},
    ).get_materialize_result()


@asset(
    name="export_benchmark",
    deps=BENCHMARK_DBT_DEPS,
    description=(
        "Writes benchmark.json from the benchmark_ageup/benchmark_vils dbt views (Rust `export "
        "benchmark --out <dir>`)."
    ),
    compute_kind="rust",
)
def export_benchmark(
    context: AssetExecutionContext,
    config: ExportOutConfig,
    pipes_subprocess_client: PipesSubprocessClient,
    postgres: PostgresResource,
) -> MaterializeResult:
    return pipes_subprocess_client.run(
        command=[str(EXPORT_BIN), "benchmark", "--out", config.out_dir],
        context=context,
        env={"DATABASE_URL": postgres.database_url},
    ).get_materialize_result()
