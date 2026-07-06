//! Integration test: `import_pair` against a REAL PostgreSQL (via `testcontainers`), proving the
//! correctness claims the brief asks for — slug-resolved `map_id`/`civ_id`, `game_id` parsed to
//! bigint, `source='aoestats'`, correct `ladder`, `elo = new_rating`, an unknown-slug row
//! skipped+counted (not inserted, not crashing), and idempotent re-import.
//!
//! The `m_`/`p_` parquet fixtures are synthetic, written in-test via `parquet::arrow::ArrowWriter`
//! (library-first — no hand-rolled parquet, matching the reader side).
//!
//! `#[ignore]`-marked so plain `cargo test` stays Docker-free. Run explicitly with:
//! `cargo test -p aoestats -- --ignored`

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use aoestats::{import_pair, ImportStats};
use arrow::array::{
    BooleanArray, DurationNanosecondArray, Float64Array, Int64Array, RecordBatch, StringArray,
    TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use chrono::{TimeZone, Utc};
use migration::{Migrator, MigratorTrait};
use parquet::arrow::ArrowWriter;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tokio_postgres::NoTls;

/// The SAME digest-pinned Postgres image the deploy stack runs (`deploy/docker-compose.yml`) —
/// mirrors `ingest`'s own integration test constant exactly.
const POSTGRES_TAG: &str =
    "18.4@sha256:4aabea78cf39b90e834caf3af7d602a18565f6fe2508705c8d01aa63245c2e20";

/// Start the pinned Postgres container, build the full schema via the SAME `Migrator` the live DB
/// runs, seed a couple of `maps`/`civs` dim rows the fixtures below reference, and return a ready
/// client. The container must stay alive for as long as the client is used, hence the tuple
/// return.
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

    seed_dimensions(&client).await;

    (container, client)
}

/// `arabia` (map_id=1) and `blackforest` (map_id=2) — deliberately squashed, no underscore,
/// matching the LIVE dims' real convention (`pipeline_core::slug::slug`) confirmed by querying the
/// production DB during this task. `britons`/`franks` civs. `moonbase`/`atlanteans` are
/// deliberately left unseeded so the unknown-slug test below can reference them.
async fn seed_dimensions(client: &tokio_postgres::Client) {
    client
        .batch_execute(
            r#"
            INSERT INTO maps (map_id, name, slug, family) VALUES
                (1, 'Arabia', 'arabia', 'open'::map_family),
                (2, 'Black Forest', 'blackforest', 'closed'::map_family);
            INSERT INTO civs (civ_id, slug) VALUES
                (1, 'britons'),
                (2, 'franks');
            "#,
        )
        .await
        .expect("failed to seed maps/civs dimension rows");
}

async fn row_count(client: &tokio_postgres::Client, table: &str) -> i64 {
    let sql = format!("SELECT count(*) FROM {table}");
    client
        .query_one(&sql, &[])
        .await
        .unwrap_or_else(|err| panic!("row count query on {table} failed: {err}"))
        .get(0)
}

/// Writes a synthetic `m_*.parquet`-shaped file with exactly the columns `parquet_read::read_matches`
/// projects, mirroring the REAL file's arrow-embedded types verified against
/// `m_2022-08-28.parquet` — notably `duration` as `Duration(Nanosecond)`, not a plain `Int64` (see
/// `parquet_read`'s module doc).
///
/// Three rows: `2001`(arabia, `random_map`), `2002`(blackforest, `team_random_map`),
/// `2003`(**`moonbase` — deliberately unseeded**, to exercise the unknown-map-slug path).
fn write_matches_fixture(path: &Path) {
    let schema = Arc::new(Schema::new(vec![
        Field::new("map", DataType::Utf8, true),
        Field::new(
            "started_timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
        Field::new("duration", DataType::Duration(TimeUnit::Nanosecond), true),
        Field::new("game_id", DataType::Utf8, true),
        Field::new("num_players", DataType::Int64, true),
        Field::new("leaderboard", DataType::Utf8, true),
        Field::new("patch", DataType::Int64, true),
    ]));

    let ts1 = Utc
        .with_ymd_and_hms(2022, 8, 28, 3, 41, 40)
        .unwrap()
        .timestamp_micros();
    let ts2 = Utc
        .with_ymd_and_hms(2022, 8, 28, 4, 45, 0)
        .unwrap()
        .timestamp_micros();
    let ts3 = Utc
        .with_ymd_and_hms(2022, 8, 28, 5, 0, 0)
        .unwrap()
        .timestamp_micros();

    let map = StringArray::from(vec![Some("arabia"), Some("blackforest"), Some("moonbase")]);
    let started_timestamp = TimestampMicrosecondArray::from(vec![Some(ts1), Some(ts2), Some(ts3)]);
    // 1_800_000_000_000 ns = 1800 s = 1_800_000 ms; 2_400_000_000_000 ns = 2_400_000 ms.
    let duration = DurationNanosecondArray::from(vec![
        Some(1_800_000_000_000),
        Some(2_400_000_000_000),
        Some(1_000_000_000_000),
    ]);
    let game_id = StringArray::from(vec![Some("2001"), Some("2002"), Some("2003")]);
    let num_players = Int64Array::from(vec![Some(2), Some(4), Some(2)]);
    let leaderboard = StringArray::from(vec![
        Some("random_map"),
        Some("team_random_map"),
        Some("random_map"),
    ]);
    let patch = Int64Array::from(vec![Some(101), Some(101), Some(101)]);

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(map),
            Arc::new(started_timestamp),
            Arc::new(duration),
            Arc::new(game_id),
            Arc::new(num_players),
            Arc::new(leaderboard),
            Arc::new(patch),
        ],
    )
    .expect("build matches RecordBatch");

    let file = File::create(path).expect("create matches fixture file");
    let mut writer = ArrowWriter::try_new(file, schema, None).expect("build ArrowWriter");
    writer.write(&batch).expect("write matches batch");
    writer.close().expect("close matches ArrowWriter");
}

/// Writes a synthetic `p_*.parquet`-shaped file. Five rows spanning three matches:
/// - `2001`: `5001`(britons, elo 1500, winner) + `5002`(franks, elo 1490, loser) — both civs known.
/// - `2002`: `5003`(**`atlanteans` — deliberately unseeded**, unknown-civ path) +
///   `5004`(britons, elo 1550).
/// - `2003`: `5005`(britons) — civ IS known, but `2003`'s match itself is never inserted (unknown
///   map), proving the `JOIN matches` gate excludes it independently of the civ join.
/// - `2002`: a 6th row with civ `franks` (known) but a **NULL `profile_id`** — reproduces a real
///   row this task's live smoke hit in the actual `p_2022-08-28.parquet` archive file (`profile_id`
///   is part of `match_players`' PK, so without the `WHERE … profile_id IS NOT NULL` guard this ONE
///   row aborts the whole transaction — see `db`'s module doc).
fn write_players_fixture(path: &Path) {
    let schema = Arc::new(Schema::new(vec![
        Field::new("game_id", DataType::Utf8, true),
        Field::new("civ", DataType::Utf8, true),
        Field::new("profile_id", DataType::Float64, true),
        Field::new("winner", DataType::Boolean, true),
        Field::new("opening", DataType::Utf8, true),
        Field::new("feudal_age_uptime", DataType::Float64, true),
        Field::new("castle_age_uptime", DataType::Float64, true),
        Field::new("imperial_age_uptime", DataType::Float64, true),
        Field::new("new_rating", DataType::Int64, true),
    ]));

    let game_id = StringArray::from(vec![
        Some("2001"),
        Some("2001"),
        Some("2002"),
        Some("2002"),
        Some("2003"),
        Some("2002"),
    ]);
    let civ = StringArray::from(vec![
        Some("britons"),
        Some("franks"),
        Some("atlanteans"),
        Some("britons"),
        Some("britons"),
        Some("franks"),
    ]);
    let profile_id = Float64Array::from(vec![
        Some(5001.0),
        Some(5002.0),
        Some(5003.0),
        Some(5004.0),
        Some(5005.0),
        None,
    ]);
    let winner = BooleanArray::from(vec![
        Some(true),
        Some(false),
        Some(true),
        Some(false),
        Some(true),
        Some(false),
    ]);
    let opening = StringArray::from(vec![Some("scouts"), None, None, Some("maa"), None, None]);
    let feudal = Float64Array::from(vec![Some(300.5), None, None, None, None, None]);
    let castle = Float64Array::from(vec![Some(700.0), None, None, None, None, None]);
    let imperial = Float64Array::from(vec![None, None, None, None, None, None]);
    let new_rating = Int64Array::from(vec![
        Some(1500),
        Some(1490),
        Some(1600),
        Some(1550),
        Some(1700),
        Some(1234),
    ]);

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(game_id),
            Arc::new(civ),
            Arc::new(profile_id),
            Arc::new(winner),
            Arc::new(opening),
            Arc::new(feudal),
            Arc::new(castle),
            Arc::new(imperial),
            Arc::new(new_rating),
        ],
    )
    .expect("build players RecordBatch");

    let file = File::create(path).expect("create players fixture file");
    let mut writer = ArrowWriter::try_new(file, schema, None).expect("build ArrowWriter");
    writer.write(&batch).expect("write players batch");
    writer.close().expect("close players ArrowWriter");
}

#[tokio::test]
#[ignore]
async fn import_pair_resolves_slugs_skips_unknowns_and_is_idempotent() {
    let (_container, mut client) = migrated_client().await;

    let dir = tempfile::tempdir().expect("create temp dir for fixtures");
    let matches_path = dir.path().join("m_2022-08-28.parquet");
    let players_path = dir.path().join("p_2022-08-28.parquet");
    write_matches_fixture(&matches_path);
    write_players_fixture(&players_path);

    // --- 1. First import: exact stats. ---
    let stats = import_pair(&mut client, &matches_path, &players_path)
        .await
        .expect("first import_pair call failed");
    assert_eq!(
        stats,
        ImportStats {
            matches_inserted: 2,
            players_inserted: 3,
            unknown_map_slugs: vec![(Some("moonbase".to_owned()), 1)],
            unknown_civ_slugs: vec![(Some("atlanteans".to_owned()), 1)],
            matches_missing_game_id: 0,
            players_missing_identity: 1,
        },
        "first import must report exactly what it wrote, which slugs it couldn't resolve, and \
         which rows it excluded for missing their own identity"
    );

    assert_eq!(row_count(&client, "matches").await, 2);
    assert_eq!(row_count(&client, "match_players").await, 3);

    // --- 2. Full-row read-backs. ---
    assert_match_2001_row(&client).await;
    assert_match_2002_row(&client).await;
    assert_player_5001_row(&client).await;

    // `2003` (unknown map) must not exist at all.
    let match_2003: i64 = client
        .query_one("SELECT count(*) FROM matches WHERE match_id = 2003", &[])
        .await
        .expect("query match 2003")
        .get(0);
    assert_eq!(
        match_2003, 0,
        "an unmatched map slug must not insert its match"
    );

    // `5003` (unknown civ, on an otherwise-valid match) and `5005` (known civ, but its match
    // `2003` never landed) must both be absent.
    let orphaned_players: i64 = client
        .query_one(
            "SELECT count(*) FROM match_players WHERE profile_id IN (5003, 5005)",
            &[],
        )
        .await
        .expect("query orphaned players")
        .get(0);
    assert_eq!(
        orphaned_players, 0,
        "an unknown civ slug AND a player of a never-inserted match must both be excluded"
    );

    // The NULL-`profile_id` row (civ `franks`, match `2002`, `elo` 1234) must be excluded too —
    // `elo = 1234` is a value unique to that row, so its absence proves the guard worked (a NULL
    // primary-key column can't be searched for directly).
    let match_2002_player_count: i64 = client
        .query_one(
            "SELECT count(*) FROM match_players WHERE match_id = 2002",
            &[],
        )
        .await
        .expect("query match_players for match 2002")
        .get(0);
    assert_eq!(
        match_2002_player_count, 1,
        "match 2002 must have exactly its one valid player (5004) — the unknown-civ row (5003) \
         and the NULL-profile_id row must both be excluded"
    );

    // --- 3. Re-import the IDENTICAL pair: idempotency. ---
    let stats2 = import_pair(&mut client, &matches_path, &players_path)
        .await
        .expect("second (re-)import_pair call failed");
    assert_eq!(
        stats2.matches_inserted, 0,
        "re-importing an already-seen pair must insert zero new matches"
    );
    assert_eq!(
        stats2.players_inserted, 0,
        "re-importing an already-seen pair must insert zero new match_players"
    );

    assert_eq!(
        row_count(&client, "matches").await,
        2,
        "idempotency: matches count unchanged on re-import"
    );
    assert_eq!(
        row_count(&client, "match_players").await,
        3,
        "idempotency: match_players count unchanged on re-import"
    );
}

/// Full-row read-back for `matches` (match_id = 2001): `source`/`ladder`/`map_id` slug-resolved
/// from `random_map`+`arabia`, plus the `duration`(ns)->`duration_ms` conversion.
async fn assert_match_2001_row(client: &tokio_postgres::Client) {
    let row = client
        .query_one(
            "SELECT source::text, ladder::text, map_id, build, patch, duration_ms, n_players \
             FROM matches WHERE match_id = 2001",
            &[],
        )
        .await
        .expect("full-row read-back query on matches (2001) failed");

    assert_eq!(row.get::<_, String>(0), "aoestats", "matches.source");
    assert_eq!(
        row.get::<_, String>(1),
        "1v1",
        "matches.ladder (random_map -> 1v1)"
    );
    assert_eq!(row.get::<_, i32>(2), 1, "matches.map_id (arabia)");
    assert_eq!(
        row.get::<_, Option<i32>>(3),
        Some(101),
        "matches.build (from patch)"
    );
    assert_eq!(
        row.get::<_, Option<String>>(4),
        None,
        "matches.patch (text col) has no aoestats source -> NULL, documented"
    );
    assert_eq!(
        row.get::<_, Option<i32>>(5),
        Some(1_800_000),
        "matches.duration_ms (1_800_000_000_000 ns -> 1_800_000 ms)"
    );
    assert_eq!(row.get::<_, i16>(6), 2, "matches.n_players");
}

/// Full-row read-back for `matches` (match_id = 2002): `team_random_map` -> `team`, `blackforest`
/// -> map_id 2.
async fn assert_match_2002_row(client: &tokio_postgres::Client) {
    let row = client
        .query_one(
            "SELECT ladder::text, map_id, duration_ms FROM matches WHERE match_id = 2002",
            &[],
        )
        .await
        .expect("full-row read-back query on matches (2002) failed");

    assert_eq!(
        row.get::<_, String>(0),
        "team",
        "matches.ladder (team_random_map -> team)"
    );
    assert_eq!(row.get::<_, i32>(1), 2, "matches.map_id (blackforest)");
    assert_eq!(
        row.get::<_, Option<i32>>(2),
        Some(2_400_000),
        "matches.duration_ms (2_400_000_000_000 ns -> 2_400_000 ms)"
    );
}

/// Full-row read-back for `match_players` (match_id = 2001, profile_id = 5001): civ_id
/// slug-resolved from `britons`, `elo = new_rating`.
async fn assert_player_5001_row(client: &tokio_postgres::Client) {
    let row = client
        .query_one(
            "SELECT civ_id, elo, won, opening, feudal_t, castle_t, imperial_t \
             FROM match_players WHERE match_id = 2001 AND profile_id = 5001",
            &[],
        )
        .await
        .expect("full-row read-back query on match_players (5001) failed");

    assert_eq!(row.get::<_, i32>(0), 1, "match_players.civ_id (britons)");
    assert_eq!(
        row.get::<_, Option<i32>>(1),
        Some(1500),
        "match_players.elo (= new_rating)"
    );
    assert_eq!(
        row.get::<_, Option<bool>>(2),
        Some(true),
        "match_players.won"
    );
    assert_eq!(
        row.get::<_, Option<String>>(3),
        Some("scouts".to_owned()),
        "match_players.opening"
    );
    assert_eq!(
        row.get::<_, Option<f32>>(4),
        Some(300.5_f32),
        "match_players.feudal_t"
    );
    assert_eq!(
        row.get::<_, Option<f32>>(5),
        Some(700.0_f32),
        "match_players.castle_t"
    );
    assert_eq!(
        row.get::<_, Option<f32>>(6),
        None,
        "match_players.imperial_t"
    );
}
