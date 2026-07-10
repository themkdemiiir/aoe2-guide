//! Typed row structs + the SQL that reads them back from the `pipeline/dbt` views. Every query
//! here targets a dbt-managed view (never raw `matches`/`match_players` — the aggregation lives in
//! `pipeline/dbt/models/*.sql`, this crate only reads the result), and uses `client.query` (never
//! `copy_out` streaming): civ-meta's whole result set is at most a few hundred rows per view (one
//! per civ x ladder x elo_bucket/map/patch/opening), nothing like the ~60M-row `match_ages` that
//! `pipeline/docs/rust-playbook.md`'s "export" section reserves streaming for.

use futures_util::TryStreamExt;
use tokio_postgres::{Client, Row};

use crate::error::{ExportError, Result};

/// The closed `ladder` vocabulary every `civ_meta`/`civ_meta_by_*` row is expected to carry —
/// mirrors `civ_meta.rs::LADDERS`. Validated here, at the row-mapping boundary, rather than left
/// to `civ_meta.rs::build_doc`'s grouping `HashMap`, which would otherwise silently DROP a
/// drifted ladder value (it only ever reads the two keys it expects) or silently ADD an
/// unexpected one as a new top-level bucket.
const KNOWN_LADDERS: [&str; 2] = ["1v1", "team"];

/// The closed `elo_bucket` vocabulary a `civ_meta` row's `elo_bucket` column is expected to carry:
/// the nine display buckets ([`pipeline_core::elo::ELO_BUCKETS`]) plus the view's own `"all"`
/// rollup-row sentinel (see [`CivMetaRow`]'s doc).
fn is_known_elo_bucket(value: &str) -> bool {
    value == "all" || pipeline_core::elo::ELO_BUCKETS.contains(&value)
}

/// Fails loud (`ExportError::UnexpectedValue`) on a `ladder` value outside [`KNOWN_LADDERS`] —
/// see that constant's doc for why this must happen here, not in `civ_meta.rs`'s grouping.
fn validate_ladder(value: String) -> Result<String> {
    if KNOWN_LADDERS.contains(&value.as_str()) {
        Ok(value)
    } else {
        Err(ExportError::UnexpectedValue {
            field: "ladder",
            value,
        })
    }
}

/// Fails loud (`ExportError::UnexpectedValue`) on an `elo_bucket` value outside the closed set —
/// see [`is_known_elo_bucket`]'s doc.
fn validate_elo_bucket(value: String) -> Result<String> {
    if is_known_elo_bucket(&value) {
        Ok(value)
    } else {
        Err(ExportError::UnexpectedValue {
            field: "elo_bucket",
            value,
        })
    }
}

/// Closed vocabulary for the benchmark views' `mode` column: the two real ladders plus the
/// GROUPING SETS `"all"` rollup grain. Mirrors `validate_ladder`/`validate_elo_bucket`.
fn is_known_mode(value: &str) -> bool {
    value == "all" || KNOWN_LADDERS.contains(&value)
}

/// Fails loud (`ExportError::UnexpectedValue`) on an `opening` value outside the closed
/// `opening_kind` vocabulary — reuses [`pipeline_core::OpeningKind`] itself (the SAME closed set
/// `civ_meta_openings.sql` casts onto — see that model's doc, final-review finding #1) as the
/// source of truth, rather than hand-duplicating its twelve values a second time here.
fn validate_opening(value: String) -> Result<String> {
    match pipeline_core::OpeningKind::try_from(value.as_str()) {
        Ok(_) => Ok(value),
        Err(_) => Err(ExportError::UnexpectedValue {
            field: "opening",
            value,
        }),
    }
}

/// Fails loud (`ExportError::UnexpectedValue`) on a `mode` value outside the closed set — see
/// [`is_known_mode`]'s doc.
fn validate_mode(value: String) -> Result<String> {
    if is_known_mode(&value) {
        Ok(value)
    } else {
        Err(ExportError::UnexpectedValue {
            field: "mode",
            value,
        })
    }
}

/// One row of the `civ_meta` view: a (civ, ladder, elo_bucket) grain, where `elo_bucket = "all"` is
/// the overall rollup row (see `pipeline/dbt/models/civ_meta.sql`'s doc).
#[derive(Debug, Clone)]
pub struct CivMetaRow {
    pub civ_slug: String,
    pub ladder: String,
    pub elo_bucket: String,
    pub games: i64,
    pub wins: i64,
    pub winrate: f64,
    /// Total appearances across every civ in this row's ladder — same value repeated on every row
    /// of that ladder; only the `elo_bucket = "all"` row's copy is actually used (for `playRate`).
    pub ladder_total_games: i64,
}

const SELECT_CIV_META: &str =
    "SELECT civ_slug, ladder, elo_bucket, games, wins, winrate, ladder_total_games FROM civ_meta";

pub async fn fetch_civ_meta(client: &Client) -> Result<Vec<CivMetaRow>> {
    let rows = client.query(SELECT_CIV_META, &[]).await?;
    rows.iter().map(row_to_civ_meta).collect()
}

fn row_to_civ_meta(row: &Row) -> Result<CivMetaRow> {
    Ok(CivMetaRow {
        civ_slug: row.try_get("civ_slug")?,
        ladder: validate_ladder(row.try_get("ladder")?)?,
        elo_bucket: validate_elo_bucket(row.try_get("elo_bucket")?)?,
        games: row.try_get("games")?,
        wins: row.try_get("wins")?,
        winrate: row.try_get("winrate")?,
        ladder_total_games: row.try_get("ladder_total_games")?,
    })
}

/// One row of the `civ_meta_by_map` view.
#[derive(Debug, Clone)]
pub struct ByMapRow {
    pub civ_slug: String,
    pub ladder: String,
    pub map_slug: String,
    pub games: i64,
    pub winrate: f64,
}

const SELECT_BY_MAP: &str =
    "SELECT civ_slug, ladder, map_slug, games, winrate FROM civ_meta_by_map";

pub async fn fetch_by_map(client: &Client) -> Result<Vec<ByMapRow>> {
    let rows = client.query(SELECT_BY_MAP, &[]).await?;
    rows.iter()
        .map(|row| {
            Ok(ByMapRow {
                civ_slug: row.try_get("civ_slug")?,
                ladder: validate_ladder(row.try_get("ladder")?)?,
                map_slug: row.try_get("map_slug")?,
                games: row.try_get("games")?,
                winrate: row.try_get("winrate")?,
            })
        })
        .collect()
}

/// One row of the `civ_meta_by_patch` view.
#[derive(Debug, Clone)]
pub struct ByPatchRow {
    pub civ_slug: String,
    pub ladder: String,
    pub build: String,
    pub games: i64,
    pub winrate: f64,
}

const SELECT_BY_PATCH: &str =
    "SELECT civ_slug, ladder, build, games, winrate FROM civ_meta_by_patch";

pub async fn fetch_by_patch(client: &Client) -> Result<Vec<ByPatchRow>> {
    let rows = client.query(SELECT_BY_PATCH, &[]).await?;
    rows.iter()
        .map(|row| {
            Ok(ByPatchRow {
                civ_slug: row.try_get("civ_slug")?,
                ladder: validate_ladder(row.try_get("ladder")?)?,
                build: row.try_get("build")?,
                games: row.try_get("games")?,
                winrate: row.try_get("winrate")?,
            })
        })
        .collect()
}

/// One row of the `civ_meta_openings` view, pre-filtered to the top 3 per (civ, ladder) —
/// `opening_rank` is assigned by the view's own `row_number() OVER (...)`. `opening` is the closed
/// `opening_kind` enum value (final-review finding #1 — see that view's doc), NOT the free-text
/// `match_players.opening` column.
#[derive(Debug, Clone)]
pub struct OpeningRow {
    pub civ_slug: String,
    pub ladder: String,
    pub opening: String,
    pub games: i64,
}

const SELECT_OPENINGS: &str =
    "SELECT civ_slug, ladder, opening, games FROM civ_meta_openings WHERE opening_rank <= 3";

pub async fn fetch_openings(client: &Client) -> Result<Vec<OpeningRow>> {
    let rows = client.query(SELECT_OPENINGS, &[]).await?;
    rows.iter()
        .map(|row| {
            Ok(OpeningRow {
                civ_slug: row.try_get("civ_slug")?,
                ladder: validate_ladder(row.try_get("ladder")?)?,
                opening: validate_opening(row.try_get("opening")?)?,
                games: row.try_get("games")?,
            })
        })
        .collect()
}

/// One row of the `civ_meta_ageup` view — median seconds-to-age-up. The view's own `WHERE
/// mp.feudal_t IS NOT NULL` guard means a civ/ladder with NO sample at all simply has no row here
/// (never a fabricated zero) — but `castle_median`/`imperial_median` are each `Option<f64>`, NOT
/// bare `f64`: `percentile_cont` ignores nulls WITHIN its own column, so a civ/ladder that reaches
/// feudal but never once reaches imperial in the sample has a real row with `imperial_median =
/// NULL` (a genuinely thin-sample outcome, not a bug — see `civ_meta.rs::build_ladder_stats`'s
/// "all or nothing" handling).
#[derive(Debug, Clone)]
pub struct AgeUpRow {
    pub civ_slug: String,
    pub ladder: String,
    pub feudal_median: Option<f64>,
    pub castle_median: Option<f64>,
    pub imperial_median: Option<f64>,
}

const SELECT_AGEUP: &str =
    "SELECT civ_slug, ladder, feudal_median, castle_median, imperial_median FROM civ_meta_ageup";

pub async fn fetch_ageup(client: &Client) -> Result<Vec<AgeUpRow>> {
    let rows = client.query(SELECT_AGEUP, &[]).await?;
    rows.iter()
        .map(|row| {
            Ok(AgeUpRow {
                civ_slug: row.try_get("civ_slug")?,
                ladder: validate_ladder(row.try_get("ladder")?)?,
                feudal_median: row.try_get("feudal_median")?,
                castle_median: row.try_get("castle_median")?,
                imperial_median: row.try_get("imperial_median")?,
            })
        })
        .collect()
}

/// One row of the `patch_axis` view: global (not per-civ), 1v1-ladder-only match counts by build —
/// feeds the top-level `patches` list.
#[derive(Debug, Clone)]
pub struct PatchAxisRow {
    pub build: String,
    pub matches: i64,
}

const SELECT_PATCH_AXIS: &str = "SELECT build, matches FROM patch_axis";

pub async fn fetch_patch_axis(client: &Client) -> Result<Vec<PatchAxisRow>> {
    let rows = client.query(SELECT_PATCH_AXIS, &[]).await?;
    rows.iter()
        .map(|row| {
            Ok(PatchAxisRow {
                build: row.try_get("build")?,
                matches: row.try_get("matches")?,
            })
        })
        .collect()
}

/// All civ slugs known to the dims (excludes `civ_id = 0`, the `"random"` sentinel — see the
/// `civs` migration's doc) — drives the `civs` map's key SET so a civ with zero appearances in the
/// sample still gets a `{"1v1": null, "team": null}` entry, matching the committed file's own
/// "every guide civ has an entry" shape (see `aggregate-rich.mjs`, which iterates the full civ set
/// rather than only civs found in the data).
const SELECT_ALL_CIV_SLUGS: &str = "SELECT slug FROM civs WHERE civ_id <> 0 ORDER BY slug";

pub async fn fetch_all_civ_slugs(client: &Client) -> Result<Vec<String>> {
    let rows = client.query(SELECT_ALL_CIV_SLUGS, &[]).await?;
    rows.iter().map(|row| Ok(row.try_get("slug")?)).collect()
}

/// The corpus's newest `played_at` month (`YYYY-MM`), for the document's top-level `sourceDate` —
/// mirrors `aggregate-rich.mjs`'s hand-set `SOURCE_DATE` constant, computed instead of hardcoded
/// since this crate has no frozen-archive month to hardcode. `None` on an empty `matches` table
/// (never fabricated).
const SELECT_SOURCE_DATE: &str = "SELECT to_char(max(played_at), 'YYYY-MM') FROM matches";

pub async fn fetch_source_date(client: &Client) -> Result<Option<String>> {
    let row = client.query_one(SELECT_SOURCE_DATE, &[]).await?;
    Ok(row.try_get(0)?)
}

// --- matchups (task M5b) ----------------------------------------------------------------------
//
// Same `client.query` (buffered) posture as the civ-meta views above: `matchups_1v1_by_elo`, the
// largest of the four, is at most (civs x civs x elo_buckets) rows — a few thousand, nothing like
// `match_ages`' scale.

/// One row of `matchups_1v1`/`matchups_team`: a directed (civ, opp) pair — see those dbt models'
/// docs for the self-join that produces both directions per match.
#[derive(Debug, Clone)]
pub struct MatchupRow {
    pub civ_slug: String,
    pub opp_slug: String,
    pub games: i64,
    pub winrate: f64,
}

const SELECT_MATCHUPS_1V1: &str = "SELECT civ_slug, opp_slug, games, winrate FROM matchups_1v1";

pub async fn fetch_matchups_1v1(client: &Client) -> Result<Vec<MatchupRow>> {
    let rows = client.query(SELECT_MATCHUPS_1V1, &[]).await?;
    rows.iter().map(row_to_matchup).collect()
}

const SELECT_MATCHUPS_TEAM: &str = "SELECT civ_slug, opp_slug, games, winrate FROM matchups_team";

pub async fn fetch_matchups_team(client: &Client) -> Result<Vec<MatchupRow>> {
    let rows = client.query(SELECT_MATCHUPS_TEAM, &[]).await?;
    rows.iter().map(row_to_matchup).collect()
}

fn row_to_matchup(row: &Row) -> Result<MatchupRow> {
    Ok(MatchupRow {
        civ_slug: row.try_get("civ_slug")?,
        opp_slug: row.try_get("opp_slug")?,
        games: row.try_get("games")?,
        winrate: row.try_get("winrate")?,
    })
}

/// One row of `matchups_1v1_by_map`.
#[derive(Debug, Clone)]
pub struct MatchupByMapRow {
    pub civ_slug: String,
    pub opp_slug: String,
    pub map_slug: String,
    pub games: i64,
    pub winrate: f64,
}

const SELECT_MATCHUPS_BY_MAP: &str =
    "SELECT civ_slug, opp_slug, map_slug, games, winrate FROM matchups_1v1_by_map";

pub async fn fetch_matchups_1v1_by_map(client: &Client) -> Result<Vec<MatchupByMapRow>> {
    let rows = client.query(SELECT_MATCHUPS_BY_MAP, &[]).await?;
    rows.iter()
        .map(|row| {
            Ok(MatchupByMapRow {
                civ_slug: row.try_get("civ_slug")?,
                opp_slug: row.try_get("opp_slug")?,
                map_slug: row.try_get("map_slug")?,
                games: row.try_get("games")?,
                winrate: row.try_get("winrate")?,
            })
        })
        .collect()
}

/// One row of `matchups_1v1_by_elo`: `elo_bucket = "all"` is the (non-null-elo-only) rollup row —
/// same convention as `civ_meta`'s own `elo_bucket = 'all'` row (see that view's doc).
#[derive(Debug, Clone)]
pub struct MatchupByEloRow {
    pub civ_slug: String,
    pub opp_slug: String,
    pub elo_bucket: String,
    pub games: i64,
    pub winrate: f64,
}

const SELECT_MATCHUPS_BY_ELO: &str =
    "SELECT civ_slug, opp_slug, elo_bucket, games, winrate FROM matchups_1v1_by_elo";

pub async fn fetch_matchups_1v1_by_elo(client: &Client) -> Result<Vec<MatchupByEloRow>> {
    let rows = client.query(SELECT_MATCHUPS_BY_ELO, &[]).await?;
    rows.iter()
        .map(|row| {
            Ok(MatchupByEloRow {
                civ_slug: row.try_get("civ_slug")?,
                opp_slug: row.try_get("opp_slug")?,
                elo_bucket: validate_elo_bucket(row.try_get("elo_bucket")?)?,
                games: row.try_get("games")?,
                winrate: row.try_get("winrate")?,
            })
        })
        .collect()
}

// --- benchmark (task M5b) ----------------------------------------------------------------------
//
// Unlike every fetch above, these two stream via `query_raw`/`RowStream` (never
// `client.query`/`Vec<Row>`) per `pipeline/docs/rust-playbook.md`'s export rule and the M5b task
// brief: `benchmark_ageup`/`benchmark_vils` are GROUPING-SETS views over `match_players`/
// `match_ages`, and at full-corpus scale (M6+) their row counts grow with civ x map x elo_bucket x
// mode combinations — unbounded compared to civ-meta's few-hundred-row views. Streaming keeps this
// exporter's memory flat regardless of how large that grows.

/// One row of `benchmark_ageup`: `map_slug`/`elo_bucket`/`mode` are `"all"` on a rollup grain —
/// see that view's doc for the four GROUPING SETS grains.
#[derive(Debug, Clone)]
pub struct BenchmarkAgeupRow {
    pub civ_slug: String,
    pub map_slug: String,
    pub elo_bucket: String,
    pub mode: String,
    pub feudal_median: f64,
    pub castle_median: f64,
    pub imperial_median: f64,
}

const SELECT_BENCHMARK_AGEUP: &str = "SELECT civ_slug, map_slug, elo_bucket, mode, \
     feudal_median, castle_median, imperial_median FROM benchmark_ageup";

pub async fn fetch_benchmark_ageup(client: &Client) -> Result<Vec<BenchmarkAgeupRow>> {
    let stream = client
        .query_raw(SELECT_BENCHMARK_AGEUP, Vec::<i32>::new())
        .await?;
    tokio::pin!(stream);
    let mut out = Vec::new();
    while let Some(row) = stream.try_next().await? {
        out.push(BenchmarkAgeupRow {
            civ_slug: row.try_get("civ_slug")?,
            map_slug: row.try_get("map_slug")?,
            elo_bucket: validate_elo_bucket(row.try_get("elo_bucket")?)?,
            mode: validate_mode(row.try_get("mode")?)?,
            feudal_median: row.try_get("feudal_median")?,
            castle_median: row.try_get("castle_median")?,
            imperial_median: row.try_get("imperial_median")?,
        });
    }
    Ok(out)
}

/// One row of `benchmark_vils` — the WINNERS-only villagers-by-Castle overlay (see that view's
/// doc for the three GROUPING SETS grains, one fewer than `benchmark_ageup`'s four: there is no
/// bare-`civ_slug` rollup here).
#[derive(Debug, Clone)]
pub struct BenchmarkVilsRow {
    pub civ_slug: String,
    pub map_slug: String,
    pub elo_bucket: String,
    pub mode: String,
    pub vils_median: f64,
}

const SELECT_BENCHMARK_VILS: &str =
    "SELECT civ_slug, map_slug, elo_bucket, mode, vils_median FROM benchmark_vils";

pub async fn fetch_benchmark_vils(client: &Client) -> Result<Vec<BenchmarkVilsRow>> {
    let stream = client
        .query_raw(SELECT_BENCHMARK_VILS, Vec::<i32>::new())
        .await?;
    tokio::pin!(stream);
    let mut out = Vec::new();
    while let Some(row) = stream.try_next().await? {
        out.push(BenchmarkVilsRow {
            civ_slug: row.try_get("civ_slug")?,
            map_slug: row.try_get("map_slug")?,
            elo_bucket: validate_elo_bucket(row.try_get("elo_bucket")?)?,
            mode: validate_mode(row.try_get("mode")?)?,
            vils_median: row.try_get("vils_median")?,
        });
    }
    Ok(out)
}

/// One row of `benchmark_ecotech` — the WINNERS-only eco-upgrade click-time percentiles. `map_slug`
/// / `elo_bucket` / `mode` are `"all"` on a rollup grain (see that view's four GROUPING SETS).
/// `tech_id` is one of the analyzer's `WATCHED_TECHS` ids; `p25_ms`/`p50_ms`/`p75_ms` are click-time
/// percentiles in milliseconds (the exporter converts to seconds). `map_slug` is un-validated (an
/// open vocabulary of live `maps.slug` values, exactly like `BenchmarkAgeupRow`).
#[derive(Debug, Clone)]
pub struct BenchmarkEcoRow {
    pub tech_id: i32,
    pub map_slug: String,
    pub elo_bucket: String,
    pub mode: String,
    pub p25_ms: f64,
    pub p50_ms: f64,
    pub p75_ms: f64,
    pub n: i64,
}

const SELECT_BENCHMARK_ECO: &str =
    "SELECT tech_id, map_slug, elo_bucket, mode, p25_ms, p50_ms, p75_ms, n FROM benchmark_ecotech";

pub async fn fetch_benchmark_ecotech(client: &Client) -> Result<Vec<BenchmarkEcoRow>> {
    let stream = client
        .query_raw(SELECT_BENCHMARK_ECO, Vec::<i32>::new())
        .await?;
    tokio::pin!(stream);
    let mut out = Vec::new();
    while let Some(row) = stream.try_next().await? {
        out.push(BenchmarkEcoRow {
            tech_id: row.try_get("tech_id")?,
            map_slug: row.try_get("map_slug")?,
            elo_bucket: validate_elo_bucket(row.try_get("elo_bucket")?)?,
            mode: validate_mode(row.try_get("mode")?)?,
            p25_ms: row.try_get("p25_ms")?,
            p50_ms: row.try_get("p50_ms")?,
            p75_ms: row.try_get("p75_ms")?,
            n: row.try_get("n")?,
        });
    }
    Ok(out)
}

// --- winner comps (Phase E) -------------------------------------------------------------------
//
// Buffered `client.query` (like `civ_meta`/`matchups` above), NOT the `query_raw` streaming
// `benchmark_ageup`/`benchmark_vils`/`benchmark_ecotech` need below: `winner_comps.sql`'s own
// `unit_rank <= 6` filter STRICTLY bounds this result at (civs) x (9 elo buckets) x 6 rows
// regardless of corpus growth — the same "few-hundred-row, not unbounded" shape `civ_meta`'s own
// module-doc reasoning describes, unlike the benchmark views' genuinely open-ended GROUPING-SETS
// grains.

/// One row of the `winner_comps` view: a `(civ_slug, elo_bucket, unit)` grain, already restricted
/// to the top 6 units by producer share per `(civ, elo_bucket)` cell via `unit_rank <= 6` (see
/// that view's `row_number()` doc). [`SELECT_WINNER_COMPS`] orders by `(civ_slug, elo_bucket,
/// unit_rank)`, so a plain per-row push in `crate::winner_comps::build_winner_comps` reproduces
/// the intended producer-share-descending `units` list order without a second sort.
#[derive(Debug, Clone)]
pub struct WinnerCompsRow {
    pub civ_slug: String,
    pub elo_bucket: String,
    pub unit: String,
    pub winners_n: i64,
    pub producers: i64,
    pub producer_pct: f64,
    pub med_count: f64,
}

const SELECT_WINNER_COMPS: &str = "SELECT civ_slug, elo_bucket, unit, winners_n, producers, \
     producer_pct, med_count FROM winner_comps WHERE unit_rank <= 6 \
     ORDER BY civ_slug, elo_bucket, unit_rank";

pub async fn fetch_winner_comps(client: &Client) -> Result<Vec<WinnerCompsRow>> {
    let rows = client.query(SELECT_WINNER_COMPS, &[]).await?;
    rows.iter()
        .map(|row| {
            Ok(WinnerCompsRow {
                civ_slug: row.try_get("civ_slug")?,
                elo_bucket: validate_elo_bucket(row.try_get("elo_bucket")?)?,
                unit: row.try_get("unit")?,
                winners_n: row.try_get("winners_n")?,
                producers: row.try_get("producers")?,
                producer_pct: row.try_get("producer_pct")?,
                med_count: row.try_get("med_count")?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // `row_to_civ_meta`/`fetch_by_map`'s etc. own closures need a real `tokio_postgres::Row`
    // (no in-process constructor exists — it's wire-format-backed), so `validate_ladder`/
    // `validate_elo_bucket` are exercised directly here instead: they are the exact functions
    // those row mappers call, so this is a direct test of the wired behavior, not a proxy for it.

    #[test]
    fn validate_ladder_accepts_the_known_vocabulary() {
        assert_eq!(validate_ladder("1v1".to_string()).unwrap(), "1v1");
        assert_eq!(validate_ladder("team".to_string()).unwrap(), "team");
    }

    #[test]
    fn validate_ladder_fails_loud_on_a_drifted_value() {
        let err = validate_ladder("2v2".to_string()).unwrap_err();
        match err {
            ExportError::UnexpectedValue { field, value } => {
                assert_eq!(field, "ladder");
                assert_eq!(value, "2v2");
            }
            other => panic!("expected UnexpectedValue, got {other:?}"),
        }
    }

    #[test]
    fn validate_elo_bucket_accepts_every_known_bucket_plus_the_all_rollup() {
        assert_eq!(validate_elo_bucket("all".to_string()).unwrap(), "all");
        for bucket in pipeline_core::elo::ELO_BUCKETS {
            assert_eq!(
                validate_elo_bucket(bucket.to_string()).unwrap(),
                bucket,
                "every closed-set elo_bucket string must validate"
            );
        }
    }

    #[test]
    fn validate_elo_bucket_fails_loud_on_a_drifted_value() {
        // A hypothetical new/renamed bucket the dbt view might start emitting — must never
        // silently become a new `byElo` key (see `civ_meta.rs::build_doc`'s grouping).
        let err = validate_elo_bucket("3000+".to_string()).unwrap_err();
        match err {
            ExportError::UnexpectedValue { field, value } => {
                assert_eq!(field, "elo_bucket");
                assert_eq!(value, "3000+");
            }
            other => panic!("expected UnexpectedValue, got {other:?}"),
        }
    }

    #[test]
    fn validate_mode_accepts_the_known_vocabulary_plus_the_all_rollup() {
        assert_eq!(validate_mode("1v1".to_string()).unwrap(), "1v1");
        assert_eq!(validate_mode("team".to_string()).unwrap(), "team");
        assert_eq!(validate_mode("all".to_string()).unwrap(), "all");
    }

    #[test]
    fn validate_mode_fails_loud_on_a_drifted_value() {
        for drifted in ["2v2", "ffa"] {
            let err = validate_mode(drifted.to_string()).unwrap_err();
            match err {
                ExportError::UnexpectedValue { field, value } => {
                    assert_eq!(field, "mode");
                    assert_eq!(value, drifted);
                }
                other => panic!("expected UnexpectedValue, got {other:?}"),
            }
        }
    }

    #[test]
    fn validate_opening_accepts_every_opening_kind_value() {
        for opening in [
            "scouts",
            "archers",
            "skirms",
            "eagles",
            "man_at_arms",
            "spears",
            "fast_castle",
            "drush",
            "trash",
            "fires",
            "galleys",
            "towers",
        ] {
            assert_eq!(
                validate_opening(opening.to_string()).unwrap(),
                opening,
                "every closed opening_kind string must validate"
            );
        }
    }

    #[test]
    fn validate_opening_fails_loud_on_a_drifted_value() {
        // `unknown` (aoestats' own non-classification bucket) is deliberately NOT an
        // `opening_kind` member (see `pipeline_core::opening`'s module doc) — it must never
        // appear here since `civ_meta_openings.sql` filters `opening_kind IS NOT NULL`, but if it
        // ever did leak through, this must reject it exactly like any other drifted value.
        for drifted in ["unknown", "man-at-arms", "Scouts"] {
            let err = validate_opening(drifted.to_string()).unwrap_err();
            match err {
                ExportError::UnexpectedValue { field, value } => {
                    assert_eq!(field, "opening");
                    assert_eq!(value, drifted);
                }
                other => panic!("expected UnexpectedValue, got {other:?}"),
            }
        }
    }
}
