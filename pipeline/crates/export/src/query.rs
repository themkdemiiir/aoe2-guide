//! Typed row structs + the SQL that reads them back from the `pipeline/dbt` views. Every query
//! here targets a dbt-managed view (never raw `matches`/`match_players` — the aggregation lives in
//! `pipeline/dbt/models/*.sql`, this crate only reads the result), and uses `client.query` (never
//! `copy_out` streaming): civ-meta's whole result set is at most a few hundred rows per view (one
//! per civ x ladder x elo_bucket/map/patch/opening), nothing like the ~60M-row `match_ages` that
//! `pipeline/docs/rust-playbook.md`'s "export" section reserves streaming for.

use futures_util::TryStreamExt;
use tokio_postgres::{Client, Row};

use crate::error::Result;

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
        ladder: row.try_get("ladder")?,
        elo_bucket: row.try_get("elo_bucket")?,
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
                ladder: row.try_get("ladder")?,
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
                ladder: row.try_get("ladder")?,
                build: row.try_get("build")?,
                games: row.try_get("games")?,
                winrate: row.try_get("winrate")?,
            })
        })
        .collect()
}

/// One row of the `civ_meta_openings` view, pre-filtered to the top 3 per (civ, ladder) —
/// `opening_rank` is assigned by the view's own `row_number() OVER (...)`.
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
                ladder: row.try_get("ladder")?,
                opening: row.try_get("opening")?,
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
                ladder: row.try_get("ladder")?,
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
                elo_bucket: row.try_get("elo_bucket")?,
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
            elo_bucket: row.try_get("elo_bucket")?,
            mode: row.try_get("mode")?,
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
            elo_bucket: row.try_get("elo_bucket")?,
            mode: row.try_get("mode")?,
            vils_median: row.try_get("vils_median")?,
        });
    }
    Ok(out)
}
