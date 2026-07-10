//! Parity gate for the `civ-cube.json` + `civ-cube-dims.json` exporters — same two-tier structure
//! as `tests/shape_parity_winner_comps.rs` (see that file's doc): a no-DB type-level round-trip
//! PLUS a self-diff (both always run), and an `#[ignore]`d live-DB run.

use export::model::{CivCubeDimsDoc, CivCubeDoc};

const COMMITTED_CIV_CUBE: &str = include_str!("../../../../public/civ-cube.json");
const COMMITTED_CIV_CUBE_DIMS: &str = include_str!("../../../../src/data/civ-cube-dims.json");

#[test]
fn committed_civ_cube_matches_doc_shape() {
    let doc: CivCubeDoc = serde_json::from_str(COMMITTED_CIV_CUBE).expect(
        "committed public/civ-cube.json no longer matches export::model::CivCubeDoc's shape",
    );
    assert!(!doc.civs.is_empty(), "sanity: the committed file has civ entries");
    assert!(!doc.elos.is_empty(), "sanity: the committed file has elo entries");
    assert!(!doc.maps.is_empty(), "sanity: the committed file has map entries");
    assert!(!doc.months.is_empty(), "sanity: the committed file has a patch axis");
    assert!(!doc.rows.is_empty(), "sanity: the committed file has packed cube rows");
}

#[test]
fn committed_civ_cube_dims_matches_doc_shape() {
    let dims: CivCubeDimsDoc = serde_json::from_str(COMMITTED_CIV_CUBE_DIMS).expect(
        "committed src/data/civ-cube-dims.json no longer matches \
         export::model::CivCubeDimsDoc's shape",
    );
    assert!(!dims.elos.is_empty(), "sanity: the committed dims file has elo entries");
    assert!(!dims.maps.is_empty(), "sanity: the committed dims file has map entries");
    assert!(!dims.months.is_empty(), "sanity: the committed dims file has a patch axis");
}

#[test]
fn self_diff_is_empty_for_both_files() {
    let cube: serde_json::Value = serde_json::from_str(COMMITTED_CIV_CUBE).unwrap();
    assert_eq!(export::shape::shape_diff(&cube, &cube), Vec::<String>::new());

    let dims: serde_json::Value = serde_json::from_str(COMMITTED_CIV_CUBE_DIMS).unwrap();
    assert_eq!(export::shape::shape_diff(&dims, &dims), Vec::<String>::new());
}

/// Runs the real streaming query+assembly path against the LIVE dev database (with `pipeline/dbt`
/// already applied) and shape-diffs both produced documents against their committed files. Run
/// explicitly: `cargo test -p export -- --ignored`.
///
/// NOTE (documented, not a bug to "fix" here): `civ_cube.sql`'s doc lists several deliberate scope
/// changes from the old DuckDB/JS generator this replaces (no `__unknown__` map sentinel, no
/// guide-civs filter, a uniform >=20-games floor) — a freshly produced document's `civs`/`maps`
/// lists and cell counts are expected to differ in VALUE from the committed file's. `shape_diff`
/// only reports STRUCTURAL drift (a field renamed/retyped/removed), which is what this test
/// actually gates.
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

    let rows = export::query::fetch_civ_cube(&client).await.unwrap();
    let patch_axis = export::query::fetch_patch_axis(&client).await.unwrap();
    let patches = export::civ_meta::build_patches(&patch_axis);
    let (doc, dims) = export::build_civ_cube(&rows, &patches);

    let produced_cube = serde_json::to_value(&doc).expect("serialize produced civ-cube doc");
    let committed_cube: serde_json::Value = serde_json::from_str(COMMITTED_CIV_CUBE).unwrap();
    let cube_diff = export::shape::shape_diff(&committed_cube, &produced_cube);
    assert_eq!(cube_diff, Vec::<String>::new(), "civ-cube.json shape diff: {cube_diff:#?}");
    let _: CivCubeDoc =
        serde_json::from_value(produced_cube).expect("produced civ-cube JSON matches CivCubeDoc");

    let produced_dims = serde_json::to_value(&dims).expect("serialize produced civ-cube-dims doc");
    let committed_dims: serde_json::Value = serde_json::from_str(COMMITTED_CIV_CUBE_DIMS).unwrap();
    let dims_diff = export::shape::shape_diff(&committed_dims, &produced_dims);
    assert_eq!(
        dims_diff,
        Vec::<String>::new(),
        "civ-cube-dims.json shape diff: {dims_diff:#?}"
    );
    let _: CivCubeDimsDoc = serde_json::from_value(produced_dims)
        .expect("produced civ-cube-dims JSON matches CivCubeDimsDoc");
}
