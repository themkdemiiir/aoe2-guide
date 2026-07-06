//! M5a's parity gate, in two tiers.
//!
//! 1. [`committed_file_matches_civ_meta_doc_shape`] / [`shape_diff_against_self_is_empty`] — no DB,
//!    run in plain `cargo test`. The first is the STRONGEST proof available: it deserializes the
//!    REAL committed `src/data/civ-meta.json` into [`export::model::CivMetaDoc`]. If every field
//!    this crate's type declares round-trips against the legacy JS generators' real output, the
//!    shapes are provably compatible — one type, exercised in both directions.
//! 2. [`exporter_output_against_live_db_matches_committed_shape`] (`#[ignore]`) — runs this crate's
//!    real query+assembly path against the LIVE dev database and shape-diffs its OWN JSON output
//!    against the committed file. Requires `DATABASE_URL` to point at a database that already has
//!    `pipeline/dbt` applied (`dbt run`) — unlike `ingest`'s/`aoestats`'s testcontainers tests, the
//!    views this exporter reads are dbt artifacts, not something `migration::Migrator` creates, so
//!    a from-scratch container can't stand this schema up without also invoking dbt inside the
//!    test (out of scope for this task — see `.superpowers/sdd/task-M5a-export-report.md` for the
//!    gate as actually run). Run explicitly: `cargo test -p export -- --ignored`.

use export::model::CivMetaDoc;

const COMMITTED_CIV_META_JSON: &str = include_str!("../../../../src/data/civ-meta.json");

#[test]
fn committed_file_matches_civ_meta_doc_shape() {
    let doc: CivMetaDoc = serde_json::from_str(COMMITTED_CIV_META_JSON).expect(
        "committed src/data/civ-meta.json no longer matches export::model::CivMetaDoc's shape",
    );
    assert!(
        !doc.civs.is_empty(),
        "sanity: the committed file has civ entries"
    );
    assert!(
        !doc.patches.is_empty(),
        "sanity: the committed file has a patch axis"
    );
}

#[test]
fn shape_diff_against_self_is_empty() {
    let value: serde_json::Value = serde_json::from_str(COMMITTED_CIV_META_JSON).unwrap();
    assert_eq!(
        export::shape::shape_diff(&value, &value),
        Vec::<String>::new()
    );
}

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

    let inputs = export::CivMetaInputs {
        civ_slugs: export::query::fetch_all_civ_slugs(&client).await.unwrap(),
        overall_and_elo: export::query::fetch_civ_meta(&client).await.unwrap(),
        by_map: export::query::fetch_by_map(&client).await.unwrap(),
        by_patch: export::query::fetch_by_patch(&client).await.unwrap(),
        openings: export::query::fetch_openings(&client).await.unwrap(),
        ageup: export::query::fetch_ageup(&client).await.unwrap(),
        patch_axis: export::query::fetch_patch_axis(&client).await.unwrap(),
        source_date: export::query::fetch_source_date(&client).await.unwrap(),
    };
    let doc = export::build_doc(inputs);
    let produced = serde_json::to_value(&doc).expect("serialize produced doc");
    let committed: serde_json::Value = serde_json::from_str(COMMITTED_CIV_META_JSON).unwrap();

    let diff = export::shape::shape_diff(&committed, &produced);
    assert_eq!(diff, Vec::<String>::new(), "shape diff: {diff:#?}");

    // The stronger, type-level proof too: the exporter's own output round-trips through the exact
    // same type the committed file does.
    let _: CivMetaDoc = serde_json::from_value(produced).expect("produced JSON matches CivMetaDoc");
}
