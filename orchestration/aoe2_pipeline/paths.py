"""Filesystem pointers this project needs, all resolved relative to this file (so the module
works from any cwd, e.g. `dagster dev` launched from the repo root or from `orchestration/`):

- The `pipeline/dbt` project + the `dbt` executable inside ITS OWN venv (`pipeline/dbt/.venv`,
  set up by `pipeline/dbt/README.md`'s `uv venv` + `uv pip install dbt-core dbt-postgres` steps).
  This project's own venv deliberately does NOT install `dbt-postgres` a second time — see
  `resources.py`'s `DbtCliResource(dbt_executable=...)` wiring in `definitions.py`.
- The Rust release binaries Pipes invokes (`cargo build --release` from `pipeline/`; see
  README.md's build step — gate 4 of the M6 brief).
"""

from pathlib import Path

# orchestration/aoe2_pipeline/paths.py -> parents[0]=aoe2_pipeline, [1]=orchestration, [2]=repo root
REPO_ROOT = Path(__file__).resolve().parents[2]
PIPELINE_DIR = REPO_ROOT / "pipeline"

DBT_PROJECT_DIR = PIPELINE_DIR / "dbt"
DBT_EXECUTABLE = DBT_PROJECT_DIR / ".venv" / "bin" / "dbt"

RUST_RELEASE_DIR = PIPELINE_DIR / "target" / "release"
EXPORT_BIN = RUST_RELEASE_DIR / "export"
AOESTATS_BIN = RUST_RELEASE_DIR / "aoestats"
DIMS_BIN = RUST_RELEASE_DIR / "dims"
# The composition-root binary — TODAY an inert skeleton (no network/DB; see its own module doc at
# `pipeline/crates/pipeline/src/main.rs`). `assets/backfill_assets.py` invokes it as a structural
# stand-in for the not-yet-built M6 discover->download->parse->ingest run loop.
PIPELINE_BIN = RUST_RELEASE_DIR / "pipeline"
