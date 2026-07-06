"""DATABASE_URL -> discrete libpq env vars.

Mirrors `pipeline/dbt/env_from_database_url.sh` in Python (same `urllib.parse` approach, same
reasoning restated there): dbt-postgres's `type: postgres` profile has no raw-DSN mode, but this
repo's one source of truth for the connection is the single `DATABASE_URL` in the git-ignored
`pipeline/.env` — the same variable every Rust pipeline binary reads. Rather than require the
operator to *also* `source pipeline/dbt/env_from_database_url.sh` before starting Dagster,
`definitions.py` calls `derive_pg_env` once at import time (guarded — see its own comment) so
`DbtCliResource`'s subprocess `dbt` invocations have PGHOST/PGPORT/PGUSER/PGPASSWORD/PGDATABASE
already in the environment.

Never logs `database_url` or the derived password.
"""

from urllib.parse import urlparse


def derive_pg_env(database_url: str) -> dict[str, str]:
    """Fail loud (no defaults) if `database_url` is missing a host or user — a partially-formed
    connection string should never silently resolve to an empty/placeholder PGHOST."""
    parsed = urlparse(database_url)
    if not parsed.hostname or not parsed.username:
        raise ValueError("DATABASE_URL is missing a host or user — cannot derive PG* env vars")
    return {
        "PGHOST": parsed.hostname,
        "PGPORT": str(parsed.port or 5432),
        "PGUSER": parsed.username,
        "PGPASSWORD": parsed.password or "",
        "PGDATABASE": (parsed.path or "/").lstrip("/"),
    }
