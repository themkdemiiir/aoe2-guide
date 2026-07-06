#!/usr/bin/env bash
# pipeline/dbt/env_from_database_url.sh
#
# dbt-postgres's connection profile (`profiles.yml`) needs discrete host/port/user/password/dbname
# fields — the `type: postgres` adapter has no raw-DSN mode — but this repo's single source of
# truth for the connection is one DATABASE_URL (git-ignored `pipeline/.env`, the same variable
# every Rust pipeline binary reads via `pipeline_core::cli::database_url`). Sourcing this script
# derives the standard libpq env vars (PGHOST/PGPORT/PGUSER/PGPASSWORD/PGDATABASE) FROM
# DATABASE_URL so `profiles.yml`'s `env_var(...)` calls have something to read, without ever
# maintaining the connection secret in two places.
#
# Usage (from `pipeline/dbt/`, with DATABASE_URL already in the environment — e.g.
# `set -a; source ../.env; set +a`):
#   source env_from_database_url.sh
#   dbt run --profiles-dir . --select civ_meta
#
# Never echoes DATABASE_URL or the derived PGPASSWORD.

if [[ -z "${DATABASE_URL:-}" ]]; then
  echo "env_from_database_url.sh: DATABASE_URL is not set" >&2
  return 1 2>/dev/null || exit 1
fi

# Parsed with python's urllib rather than bash/sed regex — robust to `:`/`@`/`/` inside a
# percent-encoded user or password, which a hand-rolled shell regex would mis-split on.
eval "$(python3 - <<'PYEOF'
import os
from urllib.parse import urlparse

u = urlparse(os.environ["DATABASE_URL"])
print(f'export PGHOST={u.hostname!r}')
print(f'export PGPORT={u.port or 5432!r}')
print(f'export PGUSER={u.username!r}')
print(f'export PGPASSWORD={u.password!r}')
print(f'export PGDATABASE={(u.path or "/").lstrip("/")!r}')
PYEOF
)"
