//! Integration test: `ingest_batch` against a REAL PostgreSQL (via `testcontainers`), proving
//! the correctness claims the library makes — row counts, idempotency on re-ingest, the
//! `elo_bucket` GENERATED column, fail-loud FK rollback, and (5b) full-row read-backs per table
//! so a same-typed adjacent-column swap can no longer pass silently.
//!
//! `#[ignore]`-marked so plain `cargo test` stays Docker-free. Run explicitly with:
//! `cargo test -p ingest -- --ignored`

use chrono::{DateTime, TimeZone, Utc};
use ingest::{
    ingest_batch, IngestStats, Ladder, MatchSource, NewMatch, NewMatchPlayer, NewMatchPlayerTech,
    NewMatchPlayerUnit, NewReplayAge, NewReplayEvent, ReplayBatch,
};
use migration::{Migrator, MigratorTrait};
use pipeline_core::{Age, GameCivId, GameUnitId, MatchId, OpeningKind, ProfileId, TechId};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tokio_postgres::NoTls;

/// The SAME digest-pinned Postgres image the deploy stack runs
/// (`deploy/docker-compose.yml`) — the correctness proof below must run against the exact
/// version/build the live schema was written for.
const POSTGRES_TAG: &str =
    "18.4@sha256:4aabea78cf39b90e834caf3af7d602a18565f6fe2508705c8d01aa63245c2e20";

/// Start the pinned Postgres container, build the full schema via the SAME `Migrator` the live
/// DB runs, seed the minimal `maps`/`civs` dimension rows the fixtures below reference (their
/// FKs need them), and return a ready client. The container must stay alive for as long as the
/// client is used, hence the tuple return.
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

    // Build the schema through sea_orm, the same way the `migration` binary does.
    let db = migration::sea_orm::Database::connect(connection_string.clone())
        .await
        .expect("failed to connect via sea_orm to run migrations");
    Migrator::up(&db, None).await.expect("Migrator::up failed");

    let (client, connection) = tokio_postgres::connect(&connection_string, NoTls)
        .await
        .expect("failed to connect via tokio_postgres");
    tokio::spawn(async move {
        // A background-task connection error would otherwise surface only as an opaque
        // "connection closed" on the next query; this test doesn't need more than that.
        let _ = connection.await;
    });

    seed_dimensions(&client).await;

    (container, client)
}

/// Seed the two `maps` rows and two `civs` rows the fixtures below reference. `map_id = 999` and
/// `civ_id = 999` are deliberately left unseeded so the fail-loud test can reference them.
async fn seed_dimensions(client: &tokio_postgres::Client) {
    client
        .batch_execute(
            r#"
            INSERT INTO maps (map_id, name, slug, family) VALUES
                (1, 'Arabia', 'arabia', 'open'::map_family),
                (2, 'Arena', 'arena', 'closed'::map_family);
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

/// A minimal but genuine two-match batch: two matches, three players (one with a known elo,
/// one with a NULL elo, one on a second map/civ), three events, four age-up rows (including one
/// `Age::Dark` row to prove the aoestats-only age round-trips end-to-end), three
/// `match_player_units` rows (Phase B, task-enrichB: two distinct units for one player on match
/// 1001, one unit for the player on match 1002 — proving the per-(match,player,unit) shape and
/// the `new_match_ids` join gate identically to `replay_ages`), and two `match_player_techs` rows
/// (Phase D, task-enrichD: one watched-tech CLICK time for the player on match 1001, one for the
/// player on match 1002 — proving the per-(match,player,tech) shape and the SAME join gate).
fn sample_batch() -> ReplayBatch {
    let played_at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();

    ReplayBatch {
        matches: vec![
            NewMatch {
                match_id: MatchId(1001),
                source: MatchSource::Replay,
                ladder: Ladder::OneVOne,
                map_id: 1,
                build: Some(101),
                patch: Some("1.0".to_owned()),
                played_at,
                duration_ms: Some(1_800_000),
                n_players: 2,
            },
            NewMatch {
                match_id: MatchId(1002),
                source: MatchSource::Replay,
                ladder: Ladder::OneVOne,
                map_id: 2,
                build: Some(101),
                patch: Some("1.0".to_owned()),
                played_at,
                duration_ms: Some(1_500_000),
                n_players: 2,
            },
        ],
        players: vec![
            NewMatchPlayer {
                match_id: MatchId(1001),
                profile_id: ProfileId(5001),
                civ_id: GameCivId(1),
                elo: Some(1400), // -> elo_bucket '1400-1649'
                won: Some(true),
                opening: Some("scouts".to_owned()),
                opening_kind: Some(OpeningKind::Scouts),
                feudal_t: Some(320.5),
                castle_t: Some(780.0),
                imperial_t: None,
                // Phase C (task-enrichC): a replay-derived player always has Some(..) apm.
                apm: Some(74.2),
            },
            NewMatchPlayer {
                match_id: MatchId(1001),
                profile_id: ProfileId(5002),
                civ_id: GameCivId(2),
                elo: None, // -> elo_bucket NULL
                won: Some(false),
                opening: None,
                opening_kind: None,
                feudal_t: None,
                castle_t: None,
                imperial_t: None,
                // Deliberately None here (unlike the other two replay players below) to prove
                // an aoestats-shaped row (no APM data) round-trips as a genuine NULL, not a
                // fabricated 0.0 — see `assert_match_player_5002_apm_is_null`.
                apm: None,
            },
            NewMatchPlayer {
                match_id: MatchId(1002),
                profile_id: ProfileId(5003),
                civ_id: GameCivId(1),
                elo: Some(2000),
                won: Some(true),
                opening: Some("man_at_arms".to_owned()),
                opening_kind: Some(OpeningKind::ManAtArms),
                feudal_t: Some(300.0),
                castle_t: None,
                imperial_t: None,
                apm: Some(0.0), // a replay player who issued zero commands -> honest Some(0.0)
            },
        ],
        events: vec![
            NewReplayEvent {
                match_id: MatchId(1001),
                profile_id: Some(ProfileId(5001)),
                player_number: 1,
                t_ms: 5_000,
                kind: "train".to_owned(),
                // Deliberately distinct from `amount` below (both are `Option<i64>`) so a
                // same-typed adjacent-column swap between `target_id`/`amount` is caught by the
                // full-row read-back, not just a row count.
                target_id: Some(83),
                amount: Some(1),
                detail: None,
            },
            NewReplayEvent {
                match_id: MatchId(1001),
                profile_id: Some(ProfileId(5002)),
                player_number: 2,
                t_ms: 6_000,
                kind: "research".to_owned(),
                target_id: Some(101),
                amount: None,
                detail: Some("loom".to_owned()),
            },
            NewReplayEvent {
                match_id: MatchId(1002),
                profile_id: Some(ProfileId(5003)),
                player_number: 1,
                t_ms: 4_000,
                kind: "train".to_owned(),
                target_id: Some(83),
                amount: Some(3),
                detail: None,
            },
        ],
        ages: vec![
            NewReplayAge {
                match_id: MatchId(1001),
                profile_id: ProfileId(5001),
                civ_id: GameCivId(1),
                won: Some(true),
                age: Age::Feudal,
                uptime_ms: 320_500,
                villagers: Some(22),
                military: Some(0),
                n_buildings: Some(6),
                n_research: Some(2),
            },
            NewReplayAge {
                match_id: MatchId(1001),
                profile_id: ProfileId(5002),
                civ_id: GameCivId(2),
                won: Some(false),
                age: Age::Feudal,
                uptime_ms: 340_200,
                villagers: Some(20),
                military: None,
                n_buildings: None,
                n_research: None,
            },
            NewReplayAge {
                match_id: MatchId(1002),
                profile_id: ProfileId(5003),
                civ_id: GameCivId(1),
                won: Some(true),
                age: Age::Feudal,
                uptime_ms: 300_000,
                villagers: Some(21),
                military: Some(0),
                n_buildings: Some(5),
                n_research: Some(1),
            },
            // aoestats-only age: the replay extractor never emits `dark` (`config::AGES` is
            // `["feudal", "castle", "imperial"]`), but the aoestats summariser does. This proves
            // `Age::Dark` round-trips through COPY into the TEXT `age` column unchanged.
            NewReplayAge {
                match_id: MatchId(1002),
                profile_id: ProfileId(5003),
                civ_id: GameCivId(1),
                won: Some(true),
                age: Age::Dark,
                uptime_ms: 0,
                villagers: Some(3),
                military: Some(0),
                n_buildings: Some(2),
                n_research: Some(0),
            },
        ],
        player_units: vec![
            NewMatchPlayerUnit {
                match_id: MatchId(1001),
                profile_id: ProfileId(5001),
                unit_id: GameUnitId(83),
                trained: 5,
            },
            NewMatchPlayerUnit {
                match_id: MatchId(1001),
                profile_id: ProfileId(5001),
                unit_id: GameUnitId(4),
                trained: 12,
            },
            NewMatchPlayerUnit {
                match_id: MatchId(1002),
                profile_id: ProfileId(5003),
                unit_id: GameUnitId(83),
                trained: 3,
            },
        ],
        player_techs: vec![
            NewMatchPlayerTech {
                match_id: MatchId(1001),
                profile_id: ProfileId(5001),
                tech_id: TechId(22), // Loom
                t_ms: 12_000,
            },
            NewMatchPlayerTech {
                match_id: MatchId(1002),
                profile_id: ProfileId(5003),
                tech_id: TechId(213), // Wheelbarrow
                t_ms: 45_000,
            },
        ],
    }
}

#[tokio::test]
#[ignore]
async fn ingest_batch_inserts_rows_is_idempotent_and_generates_elo_bucket() {
    let (_container, mut client) = migrated_client().await;
    let batch = sample_batch();

    // --- 1. First ingest: exact row counts in all four tables. ---
    let stats = ingest_batch(&mut client, &batch)
        .await
        .expect("first ingest_batch call failed");
    assert_eq!(
        stats,
        IngestStats {
            matches_inserted: 2,
            matches_skipped: 0,
            players: 3,
            events: 3,
            ages: 4,
            units: 3,
            techs: 2,
        },
        "first ingest must report exactly what it wrote"
    );

    assert_eq!(row_count(&client, "matches").await, 2);
    assert_eq!(row_count(&client, "match_players").await, 3);
    assert_eq!(row_count(&client, "replay_events").await, 3);
    assert_eq!(row_count(&client, "replay_ages").await, 4);
    assert_eq!(row_count(&client, "match_player_units").await, 3);
    assert_eq!(row_count(&client, "match_player_techs").await, 2);

    // --- 2. Full-row read-backs: every column of a known row must equal the DTO value, not
    //        just the row count. This is the check a same-typed adjacent-column swap (e.g.
    //        replay_events' `target_id`/`amount`, both `Option<i64>`) would otherwise slip
    //        through undetected. ---
    assert_match_1001_row(&client).await;
    assert_match_player_5001_row(&client).await;
    assert_match_player_5003_opening_kind(&client).await;
    assert_match_player_apm_null_vs_zero(&client).await;
    assert_replay_event_row(&client).await;
    assert_replay_ages_rows(&client).await;
    assert_match_player_units_rows(&client).await;
    assert_match_player_techs_rows(&client).await;

    // --- 3. elo_bucket is GENERATED — assert the DB-computed value, not anything we wrote. ---
    let bucket_1400: Option<String> = client
        .query_one(
            "SELECT elo_bucket FROM match_players WHERE match_id = 1001 AND profile_id = 5001",
            &[],
        )
        .await
        .expect("query elo_bucket for elo=1400 failed")
        .get(0);
    assert_eq!(
        bucket_1400.as_deref(),
        Some("1400-1649"),
        "elo=1400 must generate bucket '1400-1649'"
    );

    let bucket_null: Option<String> = client
        .query_one(
            "SELECT elo_bucket FROM match_players WHERE match_id = 1001 AND profile_id = 5002",
            &[],
        )
        .await
        .expect("query elo_bucket for elo=NULL failed")
        .get(0);
    assert_eq!(
        bucket_null, None,
        "elo=NULL must generate a NULL bucket, never a fabricated band"
    );

    // --- 4. Re-ingest the IDENTICAL batch: idempotency. ---
    let stats2 = ingest_batch(&mut client, &batch)
        .await
        .expect("second (re-)ingest_batch call failed");
    assert_eq!(
        stats2,
        IngestStats {
            matches_inserted: 0,
            matches_skipped: 2,
            players: 0,
            events: 0,
            ages: 0,
            units: 0,
            techs: 0,
        },
        "re-ingesting an already-seen batch must insert zero rows anywhere"
    );

    assert_eq!(
        row_count(&client, "matches").await,
        2,
        "idempotency: counts unchanged on re-ingest (matches)"
    );
    assert_eq!(
        row_count(&client, "match_players").await,
        3,
        "idempotency: counts unchanged on re-ingest (match_players)"
    );
    assert_eq!(
        row_count(&client, "replay_events").await,
        3,
        "idempotency: counts unchanged on re-ingest (replay_events)"
    );
    assert_eq!(
        row_count(&client, "replay_ages").await,
        4,
        "idempotency: counts unchanged on re-ingest (replay_ages)"
    );
    assert_eq!(
        row_count(&client, "match_player_units").await,
        3,
        "idempotency: counts unchanged on re-ingest (match_player_units)"
    );
    assert_eq!(
        row_count(&client, "match_player_techs").await,
        2,
        "idempotency: counts unchanged on re-ingest (match_player_techs)"
    );
}

/// Full-row read-back for `matches` (match_id = 1001), against `sample_batch().matches[0]`.
async fn assert_match_1001_row(client: &tokio_postgres::Client) {
    let row = client
        .query_one(
            "SELECT source::text, ladder::text, map_id, build, patch, played_at, duration_ms, n_players \
             FROM matches WHERE match_id = 1001",
            &[],
        )
        .await
        .expect("full-row read-back query on matches failed");

    assert_eq!(row.get::<_, String>(0), "replay", "matches.source");
    assert_eq!(row.get::<_, String>(1), "1v1", "matches.ladder");
    assert_eq!(row.get::<_, i32>(2), 1, "matches.map_id");
    assert_eq!(row.get::<_, Option<i32>>(3), Some(101), "matches.build");
    assert_eq!(
        row.get::<_, Option<String>>(4),
        Some("1.0".to_owned()),
        "matches.patch"
    );
    assert_eq!(
        row.get::<_, DateTime<Utc>>(5),
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        "matches.played_at"
    );
    assert_eq!(
        row.get::<_, Option<i32>>(6),
        Some(1_800_000),
        "matches.duration_ms"
    );
    assert_eq!(row.get::<_, i16>(7), 2, "matches.n_players");
}

/// Full-row read-back for `match_players` (match_id = 1001, profile_id = 5001), against
/// `sample_batch().players[0]` — including the f32 `feudal_t`/`castle_t`/`imperial_t` columns
/// (the 5b f64->f32 fix: these are `real`/float4 in Postgres) and (Phase C, task-enrichC) `apm`,
/// same `real`/`Option<f32>` treatment. Also proves `opening_kind` (final-review finding #1)
/// round-trips through `ingest_batch`'s `TEXT`-staged/`::opening_kind`-cast INSERT — see
/// [`assert_match_player_5003_opening_kind`] for a second, distinct label.
async fn assert_match_player_5001_row(client: &tokio_postgres::Client) {
    let row = client
        .query_one(
            "SELECT civ_id, elo, won, opening, opening_kind::text, feudal_t, castle_t, \
             imperial_t, apm \
             FROM match_players WHERE match_id = 1001 AND profile_id = 5001",
            &[],
        )
        .await
        .expect("full-row read-back query on match_players failed");

    assert_eq!(row.get::<_, i32>(0), 1, "match_players.civ_id");
    assert_eq!(row.get::<_, Option<i32>>(1), Some(1400), "match_players.elo");
    assert_eq!(row.get::<_, Option<bool>>(2), Some(true), "match_players.won");
    assert_eq!(
        row.get::<_, Option<String>>(3),
        Some("scouts".to_owned()),
        "match_players.opening"
    );
    assert_eq!(
        row.get::<_, Option<String>>(4),
        Some("scouts".to_owned()),
        "match_players.opening_kind"
    );
    assert_eq!(
        row.get::<_, Option<f32>>(5),
        Some(320.5_f32),
        "match_players.feudal_t"
    );
    assert_eq!(
        row.get::<_, Option<f32>>(6),
        Some(780.0_f32),
        "match_players.castle_t"
    );
    assert_eq!(
        row.get::<_, Option<f32>>(7),
        None,
        "match_players.imperial_t"
    );
    assert_eq!(
        row.get::<_, Option<f32>>(8),
        Some(74.2_f32),
        "match_players.apm"
    );
}

/// A SECOND, distinct `opening_kind` label (match 1002, profile 5003 — `sample_batch().players[2]`)
/// on a DIFFERENT enum value than [`assert_match_player_5001_row`]'s `scouts` — proves
/// `ingest_batch`'s `opening_kind::opening_kind` cast isn't a one-value coincidence.
async fn assert_match_player_5003_opening_kind(client: &tokio_postgres::Client) {
    let row = client
        .query_one(
            "SELECT opening, opening_kind::text \
             FROM match_players WHERE match_id = 1002 AND profile_id = 5003",
            &[],
        )
        .await
        .expect("full-row read-back query on match_players (5003) failed");
    assert_eq!(
        row.get::<_, Option<String>>(0),
        Some("man_at_arms".to_owned()),
        "match_players.opening"
    );
    assert_eq!(
        row.get::<_, Option<String>>(1),
        Some("man_at_arms".to_owned()),
        "match_players.opening_kind"
    );
}

/// Phase C (task-enrichC): `apm` round-trips its two other honest states too — a genuine `NULL`
/// (match 1001/profile 5002, standing in for the aoestats path, which never supplies APM data at
/// all) MUST stay `NULL`, never come back as a fabricated `0.0`; and a replay player who issued
/// zero commands (match 1002/profile 5003) MUST come back as a real `0.0`, never `NULL` — the
/// two states are NOT interchangeable, so this asserts them side by side against the same column.
async fn assert_match_player_apm_null_vs_zero(client: &tokio_postgres::Client) {
    let null_row = client
        .query_one(
            "SELECT apm FROM match_players WHERE match_id = 1001 AND profile_id = 5002",
            &[],
        )
        .await
        .expect("full-row read-back query on match_players (apm NULL) failed");
    assert_eq!(
        null_row.get::<_, Option<f32>>(0),
        None,
        "match_players.apm (profile 5002) must be a genuine NULL, never fabricated as 0.0"
    );

    let zero_row = client
        .query_one(
            "SELECT apm FROM match_players WHERE match_id = 1002 AND profile_id = 5003",
            &[],
        )
        .await
        .expect("full-row read-back query on match_players (apm 0.0) failed");
    assert_eq!(
        zero_row.get::<_, Option<f32>>(0),
        Some(0.0_f32),
        "match_players.apm (profile 5003) must be a real 0.0, never NULL"
    );
}

/// Full-row read-back for `replay_events` (match_id = 1001, player_number = 1), against
/// `sample_batch().events[0]` — `target_id` (83) and `amount` (1) are deliberately distinct
/// values of the same `Option<i64>` type so a column swap between them cannot pass silently.
async fn assert_replay_event_row(client: &tokio_postgres::Client) {
    let row = client
        .query_one(
            "SELECT profile_id, t_ms, kind, target_id, amount, detail \
             FROM replay_events WHERE match_id = 1001 AND player_number = 1",
            &[],
        )
        .await
        .expect("full-row read-back query on replay_events failed");

    assert_eq!(
        row.get::<_, Option<i64>>(0),
        Some(5001),
        "replay_events.profile_id"
    );
    assert_eq!(row.get::<_, i32>(1), 5_000, "replay_events.t_ms");
    assert_eq!(row.get::<_, String>(2), "train", "replay_events.kind");
    assert_eq!(
        row.get::<_, Option<i64>>(3),
        Some(83),
        "replay_events.target_id"
    );
    assert_eq!(row.get::<_, Option<i64>>(4), Some(1), "replay_events.amount");
    assert_eq!(row.get::<_, Option<String>>(5), None, "replay_events.detail");
}

/// Full-row read-back for `replay_ages` (match_id = 1002, profile_id = 5003): both the ordinary
/// `feudal` row and the `Age::Dark` row, proving the aoestats-only age round-trips through COPY
/// into the `age_kind` enum column exactly as `"dark"` (`age::text` in the SELECT so the
/// read-back can compare against a plain `String`, same as the enum-column reads elsewhere).
async fn assert_replay_ages_rows(client: &tokio_postgres::Client) {
    let feudal_row = client
        .query_one(
            "SELECT civ_id, won, age::text, uptime_ms, villagers, military, n_buildings, \
             n_research FROM replay_ages WHERE match_id = 1002 AND profile_id = 5003 AND age = \
             'feudal'",
            &[],
        )
        .await
        .expect("full-row read-back query on replay_ages (feudal) failed");

    assert_eq!(feudal_row.get::<_, i32>(0), 1, "replay_ages.civ_id (feudal)");
    assert_eq!(
        feudal_row.get::<_, Option<bool>>(1),
        Some(true),
        "replay_ages.won (feudal)"
    );
    assert_eq!(feudal_row.get::<_, String>(2), "feudal", "replay_ages.age (feudal)");
    assert_eq!(
        feudal_row.get::<_, i32>(3),
        300_000,
        "replay_ages.uptime_ms (feudal)"
    );
    assert_eq!(
        feudal_row.get::<_, Option<i32>>(4),
        Some(21),
        "replay_ages.villagers (feudal)"
    );
    assert_eq!(
        feudal_row.get::<_, Option<i32>>(5),
        Some(0),
        "replay_ages.military (feudal)"
    );
    assert_eq!(
        feudal_row.get::<_, Option<i32>>(6),
        Some(5),
        "replay_ages.n_buildings (feudal)"
    );
    assert_eq!(
        feudal_row.get::<_, Option<i32>>(7),
        Some(1),
        "replay_ages.n_research (feudal)"
    );

    let dark_row = client
        .query_one(
            "SELECT civ_id, won, age::text, uptime_ms, villagers, military, n_buildings, \
             n_research FROM replay_ages WHERE match_id = 1002 AND profile_id = 5003 AND age = \
             'dark'",
            &[],
        )
        .await
        .expect("full-row read-back query on replay_ages (dark) failed — Age::Dark must round-trip");

    assert_eq!(dark_row.get::<_, i32>(0), 1, "replay_ages.civ_id (dark)");
    assert_eq!(
        dark_row.get::<_, Option<bool>>(1),
        Some(true),
        "replay_ages.won (dark)"
    );
    assert_eq!(dark_row.get::<_, String>(2), "dark", "replay_ages.age (dark)");
    assert_eq!(dark_row.get::<_, i32>(3), 0, "replay_ages.uptime_ms (dark)");
    assert_eq!(
        dark_row.get::<_, Option<i32>>(4),
        Some(3),
        "replay_ages.villagers (dark)"
    );
    assert_eq!(
        dark_row.get::<_, Option<i32>>(5),
        Some(0),
        "replay_ages.military (dark)"
    );
    assert_eq!(
        dark_row.get::<_, Option<i32>>(6),
        Some(2),
        "replay_ages.n_buildings (dark)"
    );
    assert_eq!(
        dark_row.get::<_, Option<i32>>(7),
        Some(0),
        "replay_ages.n_research (dark)"
    );
}

/// Full-row read-back for `match_player_units` (Phase B, task-enrichB): both of match 1001/
/// profile 5001's two distinct-unit rows (proving the per-(match,player,unit) shape — `trained`
/// is a genuine per-row value, not a shared/aggregated one) and match 1002/profile 5003's row
/// (proving the `new_match_ids` join scopes correctly across matches).
async fn assert_match_player_units_rows(client: &tokio_postgres::Client) {
    let unit_83 = client
        .query_one(
            "SELECT trained FROM match_player_units \
             WHERE match_id = 1001 AND profile_id = 5001 AND unit_id = 83",
            &[],
        )
        .await
        .expect("full-row read-back query on match_player_units (unit 83) failed");
    assert_eq!(
        unit_83.get::<_, i32>(0),
        5,
        "match_player_units.trained (match 1001, profile 5001, unit 83)"
    );

    let unit_4 = client
        .query_one(
            "SELECT trained FROM match_player_units \
             WHERE match_id = 1001 AND profile_id = 5001 AND unit_id = 4",
            &[],
        )
        .await
        .expect("full-row read-back query on match_player_units (unit 4) failed");
    assert_eq!(
        unit_4.get::<_, i32>(0),
        12,
        "match_player_units.trained (match 1001, profile 5001, unit 4)"
    );

    let match_1002_row = client
        .query_one(
            "SELECT trained FROM match_player_units \
             WHERE match_id = 1002 AND profile_id = 5003 AND unit_id = 83",
            &[],
        )
        .await
        .expect("full-row read-back query on match_player_units (match 1002) failed");
    assert_eq!(
        match_1002_row.get::<_, i32>(0),
        3,
        "match_player_units.trained (match 1002, profile 5003, unit 83)"
    );
}

/// Full-row read-back for `match_player_techs` (Phase D, task-enrichD): match 1001/profile
/// 5001's Loom row and match 1002/profile 5003's Wheelbarrow row, proving the per-(match,player,
/// tech) shape (`t_ms` is a genuine per-row CLICK value, not a shared/aggregated one) and that the
/// `new_match_ids` join scopes correctly across matches, same as `match_player_units` above.
async fn assert_match_player_techs_rows(client: &tokio_postgres::Client) {
    let loom = client
        .query_one(
            "SELECT t_ms FROM match_player_techs \
             WHERE match_id = 1001 AND profile_id = 5001 AND tech_id = 22",
            &[],
        )
        .await
        .expect("full-row read-back query on match_player_techs (tech 22, Loom) failed");
    assert_eq!(
        loom.get::<_, i32>(0),
        12_000,
        "match_player_techs.t_ms (match 1001, profile 5001, tech 22/Loom)"
    );

    let wheelbarrow = client
        .query_one(
            "SELECT t_ms FROM match_player_techs \
             WHERE match_id = 1002 AND profile_id = 5003 AND tech_id = 213",
            &[],
        )
        .await
        .expect("full-row read-back query on match_player_techs (tech 213, Wheelbarrow) failed");
    assert_eq!(
        wheelbarrow.get::<_, i32>(0),
        45_000,
        "match_player_techs.t_ms (match 1002, profile 5003, tech 213/Wheelbarrow)"
    );
}

#[tokio::test]
#[ignore]
async fn ingest_batch_fails_loud_and_rolls_back_on_fk_violation() {
    let (_container, mut client) = migrated_client().await;
    let played_at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();

    let before_matches = row_count(&client, "matches").await;
    let before_players = row_count(&client, "match_players").await;

    // `map_id = 999` was deliberately left out of the `maps` seed above -> the matches insert
    // itself must violate the FK and the whole transaction must roll back.
    let bad_map_batch = ReplayBatch {
        matches: vec![NewMatch {
            match_id: MatchId(9001),
            source: MatchSource::Replay,
            ladder: Ladder::OneVOne,
            map_id: 999,
            build: None,
            patch: None,
            played_at,
            duration_ms: None,
            n_players: 2,
        }],
        ..Default::default()
    };
    ingest_batch(&mut client, &bad_map_batch)
        .await
        .expect_err("a NewMatch with an unknown map_id must fail loud, not silently skip");
    assert_eq!(
        row_count(&client, "matches").await,
        before_matches,
        "rolled back: an FK violation on matches.map_id must leave `matches` unchanged"
    );

    // A valid match_id/map_id, but `civ_id = 999` was deliberately left out of the `civs` seed
    // -> the match_players insert violates its FK, and the WHOLE transaction — including the
    // just-inserted parent match, which by itself would have succeeded — must roll back too.
    let bad_civ_batch = ReplayBatch {
        matches: vec![NewMatch {
            match_id: MatchId(9002),
            source: MatchSource::Replay,
            ladder: Ladder::OneVOne,
            map_id: 1,
            build: None,
            patch: None,
            played_at,
            duration_ms: None,
            n_players: 2,
        }],
        players: vec![NewMatchPlayer {
            match_id: MatchId(9002),
            profile_id: ProfileId(6001),
            civ_id: GameCivId(999),
            elo: None,
            won: None,
            opening: None,
            opening_kind: None,
            feudal_t: None,
            castle_t: None,
            imperial_t: None,
            apm: None,
        }],
        ..Default::default()
    };
    ingest_batch(&mut client, &bad_civ_batch)
        .await
        .expect_err("a NewMatchPlayer with an unknown civ_id must fail loud, not silently skip");
    assert_eq!(
        row_count(&client, "matches").await,
        before_matches,
        "rolled back: the parent match must not persist when a child FK violation aborts the transaction"
    );
    assert_eq!(
        row_count(&client, "match_players").await,
        before_players,
        "rolled back: an FK violation on match_players.civ_id must leave `match_players` unchanged"
    );
}
