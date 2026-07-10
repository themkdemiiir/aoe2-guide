//! Phase E's parity gate for the `winner-comps.json` exporter — same two-tier structure as
//! `tests/shape_parity.rs`/`tests/shape_parity_benchmark.rs` (see those files' docs).

use export::model::WinnerCompsDoc;

const COMMITTED_WINNER_COMPS: &str = include_str!("../../../../public/winner-comps.json");

#[test]
fn committed_winner_comps_matches_doc_shape() {
    let doc: WinnerCompsDoc = serde_json::from_str(COMMITTED_WINNER_COMPS).expect(
        "committed public/winner-comps.json no longer matches export::model::WinnerCompsDoc's shape",
    );
    assert!(
        !doc.civs.is_empty(),
        "sanity: the committed file has civ entries"
    );
    let some_cell = doc
        .civs
        .values()
        .flat_map(|by_elo| by_elo.values())
        .next()
        .expect("sanity: at least one (civ, elo_bucket) cell");
    assert!(
        !some_cell.units.is_empty(),
        "sanity: a real cell has a non-empty units list"
    );
}

#[test]
fn self_diff_is_empty() {
    let value: serde_json::Value = serde_json::from_str(COMMITTED_WINNER_COMPS).unwrap();
    assert_eq!(
        export::shape::shape_diff(&value, &value),
        Vec::<String>::new()
    );
}

/// Runs the real query+assembly path against the LIVE dev database (with `pipeline/dbt` already
/// applied) and shape-diffs the produced `winner-comps.json` against the committed file. Run
/// explicitly: `cargo test -p export -- --ignored`.
///
/// NOTE (documented, not a bug to "fix" here): this exporter's corpus (`match_player_units`, a
/// REPLAY-SOURCE-ONLY table — see `winner_comps.sql`'s doc) is materially smaller than the old
/// generator's full aoestats archive, so it is plausible for a `(civ, elo_bucket)` cell present in
/// the committed file to be ABSENT from a freshly produced doc (a thin replay-only sample not yet
/// clearing the >=100-winners threshold) — `shape_diff`'s dynamic-map handling (see `shape.rs`'s
/// doc) already treats non-overlapping `civs`/`byElo`-style keys as expected, not a mismatch, so
/// this only fails on a genuine STRUCTURAL drift (a field renamed/retyped/removed).
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

    let rows = export::query::fetch_winner_comps(&client).await.unwrap();
    let doc = export::build_winner_comps(&rows);
    let produced = serde_json::to_value(&doc).expect("serialize produced doc");
    let committed: serde_json::Value = serde_json::from_str(COMMITTED_WINNER_COMPS).unwrap();

    let diff = export::shape::shape_diff(&committed, &produced);
    assert_eq!(diff, Vec::<String>::new(), "shape diff: {diff:#?}");

    let _: WinnerCompsDoc =
        serde_json::from_value(produced).expect("produced JSON matches WinnerCompsDoc");
}
