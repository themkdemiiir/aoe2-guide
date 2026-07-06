"""`pipeline/dbt`'s 12 models as Dagster assets, via `dagster-dbt`'s `@dbt_assets`.

Every model in `pipeline/dbt/models` selects straight from the raw `matches`/`match_players`/
`match_ages`/`maps`/`civs` tables — none of them `{{ ref(...) }}` another model, and the project
declares no `sources.yml` (see `pipeline/dbt/README.md`'s model table) — so every dbt-derived
asset is, in dbt's OWN graph, a root node with no upstream. `RawTableUpstreamDbtTranslator` adds
the real upstream edge dbt itself has no way to know about: the `dims`/`aoestats_import` assets
that actually populate those raw tables (plus every partition of the future `replay_backfill`).
This is a pure Python-side lineage annotation — it does not touch a single line of dbt SQL, so it
carries zero risk to the M5a/M5b exporters' shape-parity gates.

We deliberately do NOT call `DbtProject.prepare_if_dev()` here: that would shell out to `dbt
deps`/`dbt parse` at Definitions-IMPORT time (needing a resolvable `PGHOST` etc., and hitting the
dbt project's `.venv`), which would make even `dagster definitions validate` depend on a live
profile. `pipeline/dbt/README.md`'s own `dbt parse --profiles-dir .` step is the one source of
truth for `target/manifest.json` — run it once (see this project's README) before validating or
materializing.
"""

from collections.abc import Mapping
from typing import Any, Optional

import dagster as dg
from dagster_dbt import DagsterDbtTranslator, DbtCliResource, DbtProject, dbt_assets

from ..paths import DBT_PROJECT_DIR
from .backfill_assets import replay_backfill
from .ingest_assets import aoestats_import, dims

dbt_project = DbtProject(project_dir=DBT_PROJECT_DIR, profiles_dir=DBT_PROJECT_DIR)


class RawTableUpstreamDbtTranslator(DagsterDbtTranslator):
    """Merges `dims`/`aoestats_import`/`replay_backfill` onto every dbt node's deps — see module
    doc for why this can't instead be expressed as a dbt `source()`."""

    def get_asset_spec(
        self,
        manifest: Mapping[str, Any],
        unique_id: str,
        project: Optional[DbtProject],
    ) -> dg.AssetSpec:
        spec = super().get_asset_spec(manifest, unique_id, project)
        return spec.merge_attributes(
            deps=[
                dims.key,
                aoestats_import.key,
                # Unpartitioned dbt models depending on a partitioned upstream asset need an
                # explicit partition mapping — "depends on every partition materialized so far".
                dg.AssetDep(replay_backfill.key, partition_mapping=dg.AllPartitionMapping()),
            ]
        )


@dbt_assets(
    manifest=dbt_project.manifest_path,
    project=dbt_project,
    dagster_dbt_translator=RawTableUpstreamDbtTranslator(),
)
def civ_meta_dbt_assets(context: dg.AssetExecutionContext, dbt: DbtCliResource):
    # `build` (not `run`) so dbt's own `schema.yml` `data_tests` (not_null/accepted_values) surface
    # as native Dagster asset checks alongside our custom data-quality check in `checks/`.
    yield from dbt.cli(["build"], context=context).stream()
