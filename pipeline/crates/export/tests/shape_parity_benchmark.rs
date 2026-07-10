//! Task M5b's parity gate for the `benchmark.json` exporter — same two-tier structure as
//! `tests/shape_parity.rs` (see that file's doc).

use export::model::BenchmarkDoc;

const COMMITTED_BENCHMARK: &str =
    include_str!("../../../../reference-data/benchmark.json");

#[test]
fn committed_benchmark_matches_doc_shape() {
    let doc: BenchmarkDoc = serde_json::from_str(COMMITTED_BENCHMARK)
        .expect("committed benchmark.json no longer matches export::model::BenchmarkDoc's shape");
    assert!(
        !doc.civs.is_empty(),
        "sanity: the committed file has civ entries"
    );
}

#[test]
fn self_diff_is_empty() {
    let value: serde_json::Value = serde_json::from_str(COMMITTED_BENCHMARK).unwrap();
    assert_eq!(
        export::shape::shape_diff(&value, &value),
        Vec::<String>::new()
    );
}

/// Runs the real streaming query+assembly path against the LIVE dev database and shape-diffs the
/// produced `benchmark.json` against the committed file. Run explicitly:
/// `cargo test -p export -- --ignored`.
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

    let ageup = export::query::fetch_benchmark_ageup(&client).await.unwrap();
    let vils = export::query::fetch_benchmark_vils(&client).await.unwrap();
    let doc = export::build_benchmark(&ageup, &vils);
    let produced = serde_json::to_value(&doc).expect("serialize produced doc");
    let committed: serde_json::Value = serde_json::from_str(COMMITTED_BENCHMARK).unwrap();

    let diff = export::shape::shape_diff(&committed, &produced);
    assert_eq!(diff, Vec::<String>::new(), "shape diff: {diff:#?}");

    let _: BenchmarkDoc =
        serde_json::from_value(produced).expect("produced JSON matches BenchmarkDoc");
}
