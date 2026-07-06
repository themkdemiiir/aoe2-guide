"""Shared resources.

A single `PostgresResource` is the one place `DATABASE_URL` becomes a live connection (for the
data-quality `@asset_check`) or an env var handed to a Rust binary (every Pipes-wrapped asset in
`assets/`) — never hardcoded, never logged, always sourced from the environment (git-ignored
`pipeline/.env`). `database_url` is typed `str` but populated via `dagster.EnvVar("DATABASE_URL")`
in `definitions.py`, which defers the actual env-var read to run-launch time — so `dagster
definitions validate` (which never launches a run) loads clean even when `DATABASE_URL` isn't
sourced into the shell it runs in.
"""

from collections.abc import Iterator
from contextlib import contextmanager

import psycopg
from dagster import ConfigurableResource


class PostgresResource(ConfigurableResource):
    """The pipeline's one Postgres connection string, resolved from `DATABASE_URL`."""

    database_url: str

    @contextmanager
    def get_connection(self) -> Iterator[psycopg.Connection]:
        conn = psycopg.connect(self.database_url)
        try:
            yield conn
        finally:
            conn.close()
