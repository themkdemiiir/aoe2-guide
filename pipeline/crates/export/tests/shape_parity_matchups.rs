//! Task M5b's parity gate for the four `civ-matchups*.json` exporters — same two-tier structure
//! as `tests/shape_parity.rs` (see that file's doc for the full rationale): a no-DB type-level
//! round-trip PLUS a self-diff (both always run), and an `#[ignore]`d live-DB run.

use export::model::{CivMatchupsByEloDoc, CivMatchupsByMapDoc, CivMatchupsDoc};

const COMMITTED_CIV_MATCHUPS: &str = include_str!("../../../../src/data/civ-matchups.json");
const COMMITTED_CIV_MATCHUPS_BY_MAP: &str =
    include_str!("../../../../src/data/civ-matchups-by-map.json");
const COMMITTED_CIV_MATCHUPS_BY_ELO: &str =
    include_str!("../../../../src/data/civ-matchups-by-elo.json");
const COMMITTED_CIV_MATCHUPS_TEAM: &str =
    include_str!("../../../../src/data/civ-matchups-team.json");

#[test]
fn committed_civ_matchups_matches_doc_shape() {
    let doc: CivMatchupsDoc = serde_json::from_str(COMMITTED_CIV_MATCHUPS).expect(
        "committed civ-matchups.json no longer matches export::model::CivMatchupsDoc's shape",
    );
    assert!(
        !doc.civs.is_empty(),
        "sanity: the committed file has civ entries"
    );
}

#[test]
fn committed_civ_matchups_team_matches_doc_shape() {
    // Same Rust type as civ-matchups.json — see `model.rs`'s `CivMatchupsDoc` doc.
    let doc: CivMatchupsDoc = serde_json::from_str(COMMITTED_CIV_MATCHUPS_TEAM).expect(
        "committed civ-matchups-team.json no longer matches export::model::CivMatchupsDoc's shape",
    );
    assert!(
        !doc.civs.is_empty(),
        "sanity: the committed file has civ entries"
    );
}

#[test]
fn committed_civ_matchups_by_map_matches_doc_shape() {
    let doc: CivMatchupsByMapDoc = serde_json::from_str(COMMITTED_CIV_MATCHUPS_BY_MAP).expect(
        "committed civ-matchups-by-map.json no longer matches export::model::CivMatchupsByMapDoc's shape",
    );
    assert!(
        !doc.civs.is_empty(),
        "sanity: the committed file has civ entries"
    );
}

#[test]
fn committed_civ_matchups_by_elo_matches_doc_shape() {
    let doc: CivMatchupsByEloDoc = serde_json::from_str(COMMITTED_CIV_MATCHUPS_BY_ELO).expect(
        "committed civ-matchups-by-elo.json no longer matches export::model::CivMatchupsByEloDoc's shape",
    );
    assert!(
        !doc.civs.is_empty(),
        "sanity: the committed file has civ entries"
    );
    assert_eq!(doc.elo_buckets[0], "all");
}

#[test]
fn self_diffs_are_empty() {
    for committed in [
        COMMITTED_CIV_MATCHUPS,
        COMMITTED_CIV_MATCHUPS_BY_MAP,
        COMMITTED_CIV_MATCHUPS_BY_ELO,
        COMMITTED_CIV_MATCHUPS_TEAM,
    ] {
        let value: serde_json::Value = serde_json::from_str(committed).unwrap();
        assert_eq!(
            export::shape::shape_diff(&value, &value),
            Vec::<String>::new()
        );
    }
}

/// Runs the real query+assembly path against the LIVE dev database and shape-diffs each
/// exporter's own JSON output against its committed file — see `tests/shape_parity.rs`'s doc for
/// why this needs `pipeline/dbt` already applied and can't stand itself up via testcontainers.
/// Run explicitly: `cargo test -p export -- --ignored`.
#[tokio::test]
#[ignore]
async fn exporters_output_against_live_db_matches_committed_shapes() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set (and pipeline/dbt already `dbt run` against it)");
    let (client, connection) = tokio_postgres::connect(&database_url, tokio_postgres::NoTls)
        .await
        .expect("failed to connect to the database");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let overall = export::query::fetch_matchups_1v1(&client).await.unwrap();
    let by_map = export::query::fetch_matchups_1v1_by_map(&client)
        .await
        .unwrap();
    let by_elo = export::query::fetch_matchups_1v1_by_elo(&client)
        .await
        .unwrap();
    let team = export::query::fetch_matchups_team(&client).await.unwrap();

    let cases: [(&str, serde_json::Value); 4] = [
        (
            COMMITTED_CIV_MATCHUPS,
            serde_json::to_value(export::build_civ_matchups(&overall)).unwrap(),
        ),
        (
            COMMITTED_CIV_MATCHUPS_BY_MAP,
            serde_json::to_value(export::build_civ_matchups_by_map(&by_map)).unwrap(),
        ),
        (
            COMMITTED_CIV_MATCHUPS_BY_ELO,
            serde_json::to_value(export::build_civ_matchups_by_elo(&by_elo)).unwrap(),
        ),
        (
            COMMITTED_CIV_MATCHUPS_TEAM,
            serde_json::to_value(export::build_civ_matchups_team(&team)).unwrap(),
        ),
    ];

    for (committed_str, produced) in cases {
        let committed: serde_json::Value = serde_json::from_str(committed_str).unwrap();
        let diff = export::shape::shape_diff(&committed, &produced);
        assert_eq!(diff, Vec::<String>::new(), "shape diff: {diff:#?}");
    }
}
