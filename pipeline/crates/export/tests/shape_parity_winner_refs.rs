//! Parity gate for the `winner-refs.json` exporter — same two-tier structure as
//! `tests/shape_parity_winner_comps.rs`/`tests/shape_parity_civ_cube.rs` (see those files' docs).

use export::model::WinnerRefsDoc;

const COMMITTED_WINNER_REFS: &str = include_str!("../../../../public/winner-refs.json");

#[test]
fn committed_winner_refs_matches_doc_shape() {
    let doc: WinnerRefsDoc = serde_json::from_str(COMMITTED_WINNER_REFS).expect(
        "committed public/winner-refs.json no longer matches export::model::WinnerRefsDoc's shape",
    );
    assert!(!doc.openings.is_empty(), "sanity: the committed file has opening civ entries");
    assert!(
        !doc.eco_tech_by_castle.is_empty(),
        "sanity: the committed file has ecoTechByCastle bucket entries"
    );
    assert!(
        !doc.meds_by_bucket.is_empty(),
        "sanity: the committed file has medsByBucket bucket entries"
    );
    assert!(!doc.meds_by_map.is_empty(), "sanity: the committed file has medsByMap map entries");

    let some_opening_cell = doc
        .openings
        .values()
        .flat_map(|by_elo| by_elo.values())
        .next()
        .expect("sanity: at least one (civ, elo_bucket) openings cell");
    assert!(!some_opening_cell.list.is_empty(), "sanity: a real cell has a non-empty opening list");
}

#[test]
fn self_diff_is_empty() {
    let value: serde_json::Value = serde_json::from_str(COMMITTED_WINNER_REFS).unwrap();
    assert_eq!(export::shape::shape_diff(&value, &value), Vec::<String>::new());
}

/// Runs the real query+assembly path against the LIVE dev database (with `pipeline/dbt` already
/// applied) and shape-diffs the produced `winner-refs.json` against the committed file. Run
/// explicitly: `cargo test -p export -- --ignored`.
///
/// NOTE (documented, not a bug to "fix" here — see `export::winner_refs`'s module doc for the full
/// reasoning): `ecoTechByCastle`/`medsByBucket`/`medsByMap` are sourced from REPLAY-ONLY tables
/// (`match_player_techs`/`replay_events`), a materially smaller corpus than the committed file's
/// old full-archive `match_ages.parquet`-derived extract — a freshly produced document's cells for
/// those three fields are expected to differ in COVERAGE (and possibly be sparser) from the
/// committed file's. `openings`, by contrast, is full-corpus and should stay broadly comparable.
/// `shape_diff`'s dynamic-map handling (`openings`/`ecoTechByCastle`/`medsByBucket`/`medsByMap` are
/// all in `shape::DYNAMIC_MAP_FIELDS`) already treats non-overlapping keys as expected, not a
/// mismatch — this only fails on a genuine STRUCTURAL drift (a field renamed/retyped/removed).
#[tokio::test]
#[ignore]
async fn exporter_output_against_live_db_matches_committed_shape() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set (and pipeline/dbt already `dbt run` against it)");
    let (client, connection) = tokio_postgres::connect(&database_url, tokio_postgres::NoTls)
        .await
        .expect("failed to connect to the database");
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let openings = export::query::fetch_winner_openings(&client).await.unwrap();
    let ecotech = export::query::fetch_winner_ecotech(&client).await.unwrap();
    let meds = export::query::fetch_winner_meds(&client).await.unwrap();
    let doc = export::build_winner_refs(&openings, &ecotech, &meds);
    let produced = serde_json::to_value(&doc).expect("serialize produced doc");
    let committed: serde_json::Value = serde_json::from_str(COMMITTED_WINNER_REFS).unwrap();

    let diff = export::shape::shape_diff(&committed, &produced);
    assert_eq!(diff, Vec::<String>::new(), "shape diff: {diff:#?}");

    let _: WinnerRefsDoc =
        serde_json::from_value(produced).expect("produced JSON matches WinnerRefsDoc");
}
