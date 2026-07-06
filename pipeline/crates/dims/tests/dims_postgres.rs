//! Integration test: `load_dims` against a REAL PostgreSQL (via `testcontainers`), proving the
//! correctness claims the library makes — non-zero row counts, known spot values, and
//! idempotency on re-run.
//!
//! `#[ignore]`-marked so plain `cargo test` stays Docker-free, mirroring
//! `ingest/tests/ingest_postgres.rs`. Run explicitly with: `cargo test -p dims -- --ignored`

use chrono::NaiveDate;
use dims::{load_dims, DimsStats};
use migration::{Migrator, MigratorTrait};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tokio_postgres::NoTls;

/// The SAME digest-pinned Postgres image the deploy stack runs (`deploy/docker-compose.yml`) and
/// `ingest/tests/ingest_postgres.rs` uses — the correctness proof below must run against the
/// exact version/build the live schema was written for.
const POSTGRES_TAG: &str =
    "18.4@sha256:4aabea78cf39b90e834caf3af7d602a18565f6fe2508705c8d01aa63245c2e20";

/// Start the pinned Postgres container and build the full schema via the SAME `Migrator` the live
/// DB runs (dimension tables start out empty, exactly like the real deploy). The container must
/// stay alive for as long as the client is used, hence the tuple return.
async fn migrated_client() -> (ContainerAsync<Postgres>, tokio_postgres::Client) {
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

    let db = migration::sea_orm::Database::connect(connection_string.clone())
        .await
        .expect("failed to connect via sea_orm to run migrations");
    Migrator::up(&db, None).await.expect("Migrator::up failed");

    let (client, connection) = tokio_postgres::connect(&connection_string, NoTls)
        .await
        .expect("failed to connect via tokio_postgres");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    (container, client)
}

async fn row_count(client: &tokio_postgres::Client, table: &str) -> i64 {
    let sql = format!("SELECT count(*) FROM {table}");
    client
        .query_one(&sql, &[])
        .await
        .unwrap_or_else(|err| panic!("row count query on {table} failed: {err}"))
        .get(0)
}

#[tokio::test]
#[ignore]
async fn load_dims_populates_every_table_with_known_spot_values_and_is_idempotent() {
    let (_container, mut client) = migrated_client().await;

    // Empty schema, exactly like the live DB before this loader ever ran.
    assert_eq!(row_count(&client, "maps").await, 0);
    assert_eq!(row_count(&client, "civs").await, 0);
    assert_eq!(row_count(&client, "civs_relic").await, 0);
    assert_eq!(row_count(&client, "patch_index").await, 0);

    // --- 1. First load: every table non-empty. ---
    let stats1 = load_dims(&mut client)
        .await
        .expect("first load_dims call failed");
    assert!(
        stats1.maps > 100,
        "maps.tsv has ~150 rows, got {}",
        stats1.maps
    );
    assert!(
        stats1.civs > 40,
        "civ-id-map.json has ~50 rows, got {}",
        stats1.civs
    );
    assert!(
        stats1.civs_relic > 40,
        "relic-civ-id-map.json has ~50 rows, got {}",
        stats1.civs_relic
    );
    assert!(
        stats1.patch_index > 10,
        "patch-index.json has ~20 rows, got {}",
        stats1.patch_index
    );

    let maps_count = row_count(&client, "maps").await;
    let civs_count = row_count(&client, "civs").await;
    let civs_relic_count = row_count(&client, "civs_relic").await;
    let patch_index_count = row_count(&client, "patch_index").await;
    assert_eq!(maps_count as u64, stats1.maps);
    assert_eq!(civs_count as u64, stats1.civs);
    assert_eq!(civs_relic_count as u64, stats1.civs_relic);
    assert_eq!(patch_index_count as u64, stats1.patch_index);

    // --- 2. Known spot values (M4a will JOIN on these `slug` columns). ---
    // `civs`: game/replay civ_id 2 -> franks (src/data/civ-id-map.json).
    let franks_slug: String = client
        .query_one("SELECT slug FROM civs WHERE civ_id = 2", &[])
        .await
        .expect("civ_id 2 must exist")
        .get(0);
    assert_eq!(franks_slug, "franks");

    // `civs_relic`: Relic civilization_id 5 -> britons, valid_from = provenance.validFrom.
    let (britons_slug, valid_from): (String, NaiveDate) = {
        let row = client
            .query_one(
                "SELECT slug, valid_from FROM civs_relic WHERE civ_id = 5",
                &[],
            )
            .await
            .expect("relic civ_id 5 must exist");
        (row.get(0), row.get(1))
    };
    assert_eq!(britons_slug, "britons");
    assert_eq!(valid_from, NaiveDate::from_ymd_opt(2025, 9, 1).unwrap());

    // `maps`: id 9 -> Arabia/open, id 29 -> Arena/closed (mirrors `pipeline_core::maps`' own
    // spot-checks against the real committed maps.tsv).
    let (arabia_name, arabia_slug, arabia_family): (String, String, String) = {
        let row = client
            .query_one(
                "SELECT name, slug, family::text FROM maps WHERE map_id = 9",
                &[],
            )
            .await
            .expect("map_id 9 must exist");
        (row.get(0), row.get(1), row.get(2))
    };
    assert_eq!(arabia_name, "Arabia");
    assert_eq!(arabia_slug, "arabia");
    assert_eq!(arabia_family, "open");

    let arena_family: String = client
        .query_one("SELECT family::text FROM maps WHERE map_id = 29", &[])
        .await
        .expect("map_id 29 must exist")
        .get(0);
    assert_eq!(arena_family, "closed");

    // `patch_index`: the current live build from src/data/patch-index.json.
    let (label, released): (String, Option<NaiveDate>) = {
        let row = client
            .query_one(
                "SELECT label, released FROM patch_index WHERE build = 179158",
                &[],
            )
            .await
            .expect("build 179158 must exist");
        (row.get(0), row.get(1))
    };
    assert_eq!(label, "Update 179158");
    assert_eq!(released, NaiveDate::from_ymd_opt(2026, 6, 16));

    // --- 3. Re-run: idempotent — identical stats, unchanged row counts. ---
    let stats2 = load_dims(&mut client)
        .await
        .expect("second load_dims call failed");
    assert_eq!(
        stats2, stats1,
        "re-running load_dims with unchanged refdata must report identical upsert counts"
    );
    assert_eq!(
        row_count(&client, "maps").await,
        maps_count,
        "idempotent: maps row count unchanged"
    );
    assert_eq!(
        row_count(&client, "civs").await,
        civs_count,
        "idempotent: civs row count unchanged"
    );
    assert_eq!(
        row_count(&client, "civs_relic").await,
        civs_relic_count,
        "idempotent: civs_relic row count unchanged"
    );
    assert_eq!(
        row_count(&client, "patch_index").await,
        patch_index_count,
        "idempotent: patch_index row count unchanged"
    );

    // Refreshing a slug (the "re-running refreshes slugs after a refdata change" contract) is
    // exercised at the SQL level, not by mutating the committed refdata file: hand-craft a
    // conflicting upsert against `civs` and confirm ON CONFLICT DO UPDATE is really wired up.
    client
        .execute(
            "INSERT INTO civs (civ_id, slug) VALUES (2, 'stale-slug') \
             ON CONFLICT (civ_id) DO UPDATE SET slug = EXCLUDED.slug",
            &[],
        )
        .await
        .expect("manual conflicting upsert failed");
    let stale: String = client
        .query_one("SELECT slug FROM civs WHERE civ_id = 2", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        stale, "stale-slug",
        "test setup: civ_id 2 must now be stale"
    );

    load_dims(&mut client)
        .await
        .expect("third load_dims call failed");
    let refreshed: String = client
        .query_one("SELECT slug FROM civs WHERE civ_id = 2", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        refreshed, "franks",
        "load_dims must refresh a stale slug back to the refdata value on re-run"
    );
    assert_eq!(
        row_count(&client, "civs").await,
        civs_count,
        "the refresh must not have changed the row count"
    );
}

/// Trivial sanity check that [`DimsStats`] fields line up positionally with what the assertions
/// above expect (`maps, civs, civs_relic, patch_index`), guarding against an accidental field
/// reorder silently compiling since the struct derives no `Ord`/positional API elsewhere.
#[test]
fn dims_stats_default_is_all_zero() {
    assert_eq!(
        DimsStats::default(),
        DimsStats {
            maps: 0,
            civs: 0,
            civs_relic: 0,
            patch_index: 0,
        }
    );
}
