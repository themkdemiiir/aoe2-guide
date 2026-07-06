//! Regression test for the `m20260706_000011_create_age_kind_enum` rollback bug: an earlier
//! version of that migration recreated the dbt-owned `benchmark_vils` view in BOTH `up` and
//! `down`. SeaORM runs `down`s in reverse migration order, so a multi-step rollback (`refresh`/
//! `reset`/`down -n>=2`, all first-class subcommands in `main.rs`) ran migration 11's `down`
//! (which recreated `benchmark_vils`, depending on `match_ages`) and THEN migration 10's `down`
//! (`DROP TABLE match_ages`, no cascade) — Postgres refused with "view benchmark_vils depends on
//! table match_ages" and the whole rollback aborted mid-teardown. A single `down -n 1` never hit
//! it (`benchmark_vils` still existed to satisfy migration 11's own `down`), which is why this
//! needed a full round-trip through the ENTIRE down-chain to reproduce.
//!
//! The fix: neither `up` nor `down` recreates the view anymore (only `dbt run`, post-migration,
//! does) — see that migration's module doc. This test proves the down-chain no longer aborts by
//! running the real `Migrator` all the way up, then all the way down-then-up again
//! (`Migrator::refresh`), against a real Postgres.
//!
//! `#[ignore]`-marked so plain `cargo test` stays Docker-free, same convention as
//! `ingest`/`aoestats`'s testcontainers suites. Run explicitly with:
//! `cargo test -p migration -- --ignored`

use migration::sea_orm::{ConnectionTrait, DatabaseConnection};
use migration::{Migrator, MigratorTrait};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};

/// The SAME digest-pinned Postgres image the deploy stack runs (`deploy/docker-compose.yml`) and
/// the `ingest`/`aoestats` testcontainers suites use — this rollback proof must run against the
/// exact version/build the live schema (and the bug) was observed on.
const POSTGRES_TAG: &str =
    "18.4@sha256:4aabea78cf39b90e834caf3af7d602a18565f6fe2508705c8d01aa63245c2e20";

/// Start the pinned Postgres container and return a `sea_orm` connection to it. The container
/// must stay alive for as long as the connection is used, hence the tuple return.
async fn container_and_connection() -> (ContainerAsync<Postgres>, DatabaseConnection) {
    let container = Postgres::default()
        .with_tag(POSTGRES_TAG)
        .start()
        .await
        .expect("failed to start the postgres container");

    let host = container
        .get_host()
        .await
        .expect("failed to read the container host");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("failed to read the mapped postgres port");
    let connection_string = format!("postgres://postgres:postgres@{host}:{port}/postgres");

    let db = migration::sea_orm::Database::connect(connection_string)
        .await
        .expect("failed to connect via sea_orm");

    (container, db)
}

#[tokio::test]
#[ignore]
async fn migrator_refresh_does_not_abort_on_the_age_kind_migration() {
    let (_container, db) = container_and_connection().await;

    Migrator::up(&db, None)
        .await
        .expect("Migrator::up must succeed on a fresh database");

    // The fix's whole point: `up` no longer recreates the dbt-owned `benchmark_vils` view, so it
    // must NOT exist post-migrate (only `dbt run`, which never runs in this test, creates it).
    let select_view = db
        .execute_unprepared("SELECT 1 FROM benchmark_vils LIMIT 1")
        .await;
    assert!(
        select_view.is_err(),
        "benchmark_vils must not exist after Migrator::up — only dbt run creates it now"
    );

    // The actual regression proof: on the buggy code, this `refresh` (a full `down` of every
    // migration, reverse order, then a full `up`) aborted with "view benchmark_vils depends on
    // table match_ages" partway through the down-chain. It must now succeed end to end.
    Migrator::refresh(&db)
        .await
        .expect("Migrator::refresh must succeed — the down-chain must not abort on match_ages");
}
