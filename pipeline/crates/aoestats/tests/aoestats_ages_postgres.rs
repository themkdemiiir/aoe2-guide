//! Integration test: `import_ages` against a REAL PostgreSQL (via `testcontainers`) AND a REAL
//! `python3` subprocess — proving Task M4b's correctness claims: civ_id resolved, age lowercase
//! (round-tripped through `pipeline_core::Age`), `uptime_ms = uptime * 1000`, metrics correct, an
//! unknown-civ-slug row skipped+counted, a missing-identity row skipped+counted, and idempotent
//! re-import via delete-then-insert (`match_ages` has no primary key to `ON CONFLICT` against).
//!
//! The `p_*.parquet` fixture is synthetic (written via `parquet::arrow::ArrowWriter`, same
//! convention as `tests/aoestats_postgres.rs`), but its `replay_summary_raw` values are a REAL
//! Python-repr blob captured from the live aoestats archive (`p_2022-08-28.parquet`, `civ =
//! 'mayans'`, via `~/bin/duckdb -json` on `mkd@192.168.111.7` — see the task report) — not
//! hand-written, so this exercises the real `ast.literal_eval` shape end to end.
//!
//! `#[ignore]`-marked so plain `cargo test` stays Docker-free (and python3-optional). Run
//! explicitly with: `cargo test -p aoestats -- --ignored`

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use aoestats::{import_ages, AgesImportStats};
use arrow::array::{BooleanArray, Float64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use migration::{Migrator, MigratorTrait};
use parquet::arrow::ArrowWriter;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tokio_postgres::NoTls;

/// The SAME digest-pinned Postgres image the deploy stack runs, matching every other
/// testcontainers test in this workspace.
const POSTGRES_TAG: &str =
    "18.4@sha256:4aabea78cf39b90e834caf3af7d602a18565f6fe2508705c8d01aa63245c2e20";

/// A REAL `replay_summary_raw` value (verbatim, `civ = 'mayans'`) from `p_2022-08-28.parquet`:
/// dark age reached (5 villagers, 4 houses + 1 town center, 0 research, `uptime=0`); feudal/
/// castle/imperial each present in `age_stats` but NOT researched (`age_researched: False`,
/// `uptime: -1` — a real sentinel value the archive uses, ported through as-is per the "never
/// fabricate" rule, not special-cased). Confirmed parity: running BOTH
/// `scripts/data-pipeline/extract-replay-summaries.py` and `pipeline/py/aoestats_summaries.py` on
/// this exact string produces identical output on every field `match_ages` uses (see the task
/// report's Gate 2).
const MAYANS_REAL_BLOB: &str = "{'age_stats': {'dark': {'uptime': 0, 'research': [], 'unit_counts': {'villager': 5}, 'age_researched': True, 'building_counts': {'house': 4, 'town center': 1}}, 'castle': {'uptime': -1, 'research': [], 'unit_counts': {}, 'age_researched': False, 'building_counts': {}}, 'feudal': {'uptime': -1, 'research': [], 'unit_counts': {}, 'age_researched': False, 'building_counts': {}}, 'imperial': {'uptime': -1, 'research': [], 'unit_counts': {}, 'age_researched': False, 'building_counts': {}}}, 'opening_name': 'unknown'}";

/// Start the pinned Postgres container, build the full schema (including migration #10,
/// `match_ages`) via the SAME `Migrator` the live DB runs, seed one `civs` row, and return a ready
/// client.
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

    client
        .batch_execute("INSERT INTO civs (civ_id, slug) VALUES (1, 'mayans');")
        .await
        .expect("failed to seed the mayans civ row");
    // `atlanteans` is deliberately left unseeded so the unknown-civ-slug path is exercised below.

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

/// Five `p_*.parquet`-shaped rows, exactly the columns `read_player_age_sources` projects:
/// - `3001`(mayans, profile 9999, real blob) — the happy path: everything resolves.
/// - `3002`(**atlanteans — deliberately unseeded**, profile 9998, real blob) — unknown-civ path.
/// - `3003`(mayans, **profile_id NULL**, real blob) — missing-identity path.
/// - `3004`(mayans, profile 9997, `replay_summary_raw` **too short** — filtered before python).
/// - `3005`(mayans, profile 9996, `replay_summary_raw` **NULL** — filtered before python).
fn write_ages_fixture(path: &Path) {
    let schema = Arc::new(Schema::new(vec![
        Field::new("game_id", DataType::Utf8, true),
        Field::new("civ", DataType::Utf8, true),
        Field::new("profile_id", DataType::Float64, true),
        Field::new("winner", DataType::Boolean, true),
        Field::new("replay_summary_raw", DataType::Utf8, true),
    ]));

    let game_id = StringArray::from(vec![
        Some("3001"),
        Some("3002"),
        Some("3003"),
        Some("3004"),
        Some("3005"),
    ]);
    let civ = StringArray::from(vec![
        Some("mayans"),
        Some("atlanteans"),
        Some("mayans"),
        Some("mayans"),
        Some("mayans"),
    ]);
    let profile_id = Float64Array::from(vec![
        Some(9999.0),
        Some(9998.0),
        None,
        Some(9997.0),
        Some(9996.0),
    ]);
    let winner = BooleanArray::from(vec![
        Some(true),
        Some(false),
        Some(true),
        Some(true),
        Some(true),
    ]);
    let replay_summary_raw = StringArray::from(vec![
        Some(MAYANS_REAL_BLOB),
        Some(MAYANS_REAL_BLOB),
        Some(MAYANS_REAL_BLOB),
        Some("too short"),
        None,
    ]);

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(game_id),
            Arc::new(civ),
            Arc::new(profile_id),
            Arc::new(winner),
            Arc::new(replay_summary_raw),
        ],
    )
    .expect("build ages RecordBatch");

    let file = File::create(path).expect("create ages fixture file");
    let mut writer = ArrowWriter::try_new(file, schema, None).expect("build ArrowWriter");
    writer.write(&batch).expect("write ages batch");
    writer.close().expect("close ages ArrowWriter");
}

#[tokio::test]
#[ignore]
async fn import_ages_resolves_civs_converts_uptime_skips_unknowns_and_is_idempotent() {
    let (_container, mut client) = migrated_client().await;

    let dir = tempfile::tempdir().expect("create temp dir for fixture");
    let players_path = dir.path().join("p_test_ages.parquet");
    write_ages_fixture(&players_path);

    // --- 1. First import: exact stats. ---
    let stats = import_ages(&mut client, &players_path)
        .await
        .expect("first import_ages call failed");
    assert_eq!(
        stats,
        AgesImportStats {
            // Only 3001/3002/3003 pass the `length(replay_summary_raw) > 50` filter; 3004 (too
            // short) and 3005 (NULL) never reach the python subprocess.
            source_rows: 3,
            // The real blob yields exactly 4 age-rows (dark/feudal/castle/imperial) per source
            // row: 3 * 4 = 12.
            staged_rows: 12,
            ages_deleted: 0,
            // Only 3001's 4 age-rows have BOTH a known civ AND a non-NULL profile_id.
            ages_inserted: 4,
            unknown_civ_slugs: vec![(Some("atlanteans".to_owned()), 4)],
            rows_missing_identity: 4,
        },
        "first import must report exactly what it staged/resolved/excluded"
    );

    assert_eq!(row_count(&client, "match_ages").await, 4);

    // --- 2. Full-row read-backs for match_id 3001, all four ages. ---
    assert_dark_age_row(&client).await;
    assert_unresearched_age_row(&client, "feudal").await;
    assert_unresearched_age_row(&client, "castle").await;
    assert_unresearched_age_row(&client, "imperial").await;

    // `3002` (unknown civ) and `3003` (missing profile_id) must have NO rows at all.
    let excluded: i64 = client
        .query_one(
            "SELECT count(*) FROM match_ages WHERE match_id IN (3002, 3003)",
            &[],
        )
        .await
        .expect("query excluded match_ids")
        .get(0);
    assert_eq!(
        excluded, 0,
        "an unknown civ slug and a NULL profile_id must both be excluded, never fabricated"
    );

    // --- 3. Re-import the IDENTICAL file: idempotency via delete-then-insert. ---
    let stats2 = import_ages(&mut client, &players_path)
        .await
        .expect("second (re-)import_ages call failed");
    assert_eq!(
        stats2.ages_deleted, 4,
        "re-import must delete the 4 previously-inserted rows for match_id 3001 (the only \
         match_id in this batch that ever had rows) before re-inserting them"
    );
    assert_eq!(
        stats2.ages_inserted, 4,
        "re-import must re-insert the same 4 rows"
    );
    assert_eq!(
        row_count(&client, "match_ages").await,
        4,
        "idempotency: match_ages count unchanged after re-import (delete-then-insert, not \
         append)"
    );
}

/// `match_id=3001`, `age='dark'`: civ_id resolved (mayans -> 1), `uptime_ms = 0 * 1000 = 0`,
/// villagers=5 (from `unit_counts.villager`), military=0 (no non-villager units), n_buildings=5
/// (4 houses + 1 town center), n_research=0 (empty `research` list).
async fn assert_dark_age_row(client: &tokio_postgres::Client) {
    let row = client
        .query_one(
            "SELECT profile_id, civ_id, won, age::text, uptime_ms, villagers, military, \
             n_buildings, n_research FROM match_ages WHERE match_id = 3001 AND age = 'dark'",
            &[],
        )
        .await
        .expect("full-row read-back query on match_ages (3001, dark) failed");

    assert_eq!(row.get::<_, i64>(0), 9999, "match_ages.profile_id");
    assert_eq!(row.get::<_, i32>(1), 1, "match_ages.civ_id (mayans)");
    assert_eq!(row.get::<_, Option<bool>>(2), Some(true), "match_ages.won");
    assert_eq!(row.get::<_, String>(3), "dark", "match_ages.age is lowercase");
    assert_eq!(
        row.get::<_, i32>(4),
        0,
        "match_ages.uptime_ms (uptime=0 seconds -> 0 ms)"
    );
    assert_eq!(row.get::<_, Option<i32>>(5), Some(5), "match_ages.villagers");
    assert_eq!(row.get::<_, Option<i32>>(6), Some(0), "match_ages.military");
    assert_eq!(
        row.get::<_, Option<i32>>(7),
        Some(5),
        "match_ages.n_buildings (4 house + 1 town center)"
    );
    assert_eq!(
        row.get::<_, Option<i32>>(8),
        Some(0),
        "match_ages.n_research"
    );
}

/// `match_id=3001`, the three ages the real blob marks `age_researched: False` with the archive's
/// `uptime: -1` sentinel: `uptime_ms = -1 * 1000 = -1000` (ported through as-is, not
/// special-cased — see [`MAYANS_REAL_BLOB`]'s doc), all metrics zero (empty `unit_counts`/
/// `building_counts`/`research`).
async fn assert_unresearched_age_row(client: &tokio_postgres::Client, age: &str) {
    let row = client
        .query_one(
            "SELECT civ_id, uptime_ms, villagers, military, n_buildings, n_research \
             FROM match_ages WHERE match_id = 3001 AND age::text = $1",
            &[&age],
        )
        .await
        .unwrap_or_else(|err| panic!("full-row read-back query on match_ages (3001, {age}) failed: {err}"));

    assert_eq!(row.get::<_, i32>(0), 1, "match_ages.civ_id (mayans)");
    assert_eq!(
        row.get::<_, i32>(1),
        -1000,
        "match_ages.uptime_ms for {age} (uptime=-1 seconds -> -1000 ms)"
    );
    assert_eq!(row.get::<_, Option<i32>>(2), Some(0), "match_ages.villagers for {age}");
    assert_eq!(row.get::<_, Option<i32>>(3), Some(0), "match_ages.military for {age}");
    assert_eq!(row.get::<_, Option<i32>>(4), Some(0), "match_ages.n_buildings for {age}");
    assert_eq!(row.get::<_, Option<i32>>(5), Some(0), "match_ages.n_research for {age}");
}
