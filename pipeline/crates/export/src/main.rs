//! CLI over the `export` library: `civ-meta` runs the M5a exporter against the live DB;
//! `shape-diff` is a standalone structural-diff tool over two already-written JSON files (used by
//! this task's parity gate, and generally useful for any future exporter's own shape check).
//!
//! Like `ingest`'s/`aoestats`'s binaries, `DATABASE_URL` is read from the environment via
//! `pipeline_core::cli::database_url` and never placed in a clap arg, help string, or log line —
//! and `shape-diff` never touches it at all (it only ever reads two local files).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand};
use export::{build_doc, CivMetaInputs};
use pipeline_core::cli::{database_url, init_tracing, log_error_and_code};
use pipeline_core::redact_secret;
use serde::Serialize;
use tokio_postgres::NoTls;

/// Default log filter when `RUST_LOG` is unset.
const DEFAULT_LOG_FILTER: &str = "info";

#[derive(Parser)]
#[command(
    name = "export",
    version,
    about = "Export the AOE2 guide's src/data/*.json shapes from the PostgreSQL pipeline store"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Export `civ-meta.json` from the `pipeline/dbt` `civ_meta*`/`patch_axis` views.
    CivMeta {
        /// Directory to write `civ-meta.json` into (created if missing). NEVER `src/data` for
        /// this task — the M5a brief keeps the committed file untouched.
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
    },
    /// Export all four `civ-matchups*.json` files from the `pipeline/dbt` `matchups_*` views, in
    /// one pass — mirrors `refresh-matchups-current.mjs`'s own single-pass shape (task M5b).
    Matchups {
        /// Directory to write `civ-matchups.json`/`civ-matchups-by-map.json`/
        /// `civ-matchups-by-elo.json`/`civ-matchups-team.json` into. NEVER `src/data`.
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
    },
    /// Export `benchmark.json` from the `pipeline/dbt` `benchmark_ageup`/`benchmark_vils` views
    /// (task M5b) — streams both views via `query_raw` rather than buffering (see
    /// `export::query`'s doc).
    Benchmark {
        /// Directory to write `benchmark.json` into. NEVER `reference-data`.
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
    },
    /// Print the structural (key-set + type-family) diff between two JSON files; exits 1 if the
    /// diff is non-empty. Values are ignored — see `export::shape`'s doc.
    ShapeDiff {
        #[arg(value_name = "FILE_A")]
        a: PathBuf,
        #[arg(value_name = "FILE_B")]
        b: PathBuf,
    },
}

#[tokio::main]
async fn main() {
    init_tracing(DEFAULT_LOG_FILTER);

    // Parse args before touching the environment, so `--help`/`--version` never depend on
    // `DATABASE_URL` (and `shape-diff` never needs it at all).
    let cli = Cli::parse();

    match cli.command {
        Command::ShapeDiff { a, b } => std::process::exit(run_shape_diff(&a, &b)),
        Command::CivMeta { out } => {
            let secret = database_url_or_exit();
            if let Err(err) = run_civ_meta_export(&out, secret.expose()).await {
                std::process::exit(log_error_and_code(&err, &secret));
            }
        }
        Command::Matchups { out } => {
            let secret = database_url_or_exit();
            if let Err(err) = run_matchups_export(&out, secret.expose()).await {
                std::process::exit(log_error_and_code(&err, &secret));
            }
        }
        Command::Benchmark { out } => {
            let secret = database_url_or_exit();
            if let Err(err) = run_benchmark_export(&out, secret.expose()).await {
                std::process::exit(log_error_and_code(&err, &secret));
            }
        }
    }
}

/// Reads `DATABASE_URL` or exits(1) — the identical "every DB-backed subcommand needs this first"
/// step `civ-meta`/`matchups`/`benchmark` all share (`shape-diff` never calls this; it touches no
/// database at all — see the module doc).
fn database_url_or_exit() -> pipeline_core::Secret {
    match database_url() {
        Ok(secret) => secret,
        Err(err) => {
            tracing::error!(error = %err, "failed to read DATABASE_URL");
            std::process::exit(1);
        }
    }
}

/// Connects to `database_url`, spawning the connection's driver task exactly like
/// `run_civ_meta_export` did before this helper was extracted (redacts the connection error
/// before logging, same as every other pipeline binary's `main.rs`).
///
/// Also disables parallel-worker planning (`max_parallel_workers_per_gather = 0`) for this
/// session only: the `civ_meta`/`matchups`/`benchmark` exports run GROUPING-SETS aggregations
/// over the full 30M+-match table, and PostgreSQL's parallel workers allocate their DSA
/// (dynamic shared memory) out of `/dev/shm`, which the aoe2-stack Postgres container sizes far
/// below what those workers request — the query fails with "could not resize shared memory
/// segment ... No space left on device" (see the M6 export-under-Dagster incident). Forcing a
/// single-worker (serial) plan avoids that allocation entirely; it costs some wall-clock time on
/// these already-slow analytical queries but makes `export` work under any container without an
/// infra change. `SET` is session-scoped — it only affects this connection, not the server, and
/// not `ingest`/`migration`/`aoestats`, which have their own separate connections.
async fn connect(database_url: &str) -> anyhow::Result<tokio_postgres::Client> {
    let (client, connection) = tokio_postgres::connect(database_url, NoTls)
        .await
        .context("failed to connect to the database")?;
    let database_url_for_log = database_url.to_owned();
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            let message = redact_secret(&format!("{err:#}"), &database_url_for_log);
            tracing::error!(error = %message, "database connection closed with an error");
        }
    });
    client
        .batch_execute("SET max_parallel_workers_per_gather = 0")
        .await
        .context("failed to disable parallel workers on the export connection")?;
    Ok(client)
}

async fn run_matchups_export(out: &Path, database_url: &str) -> anyhow::Result<()> {
    let client = connect(database_url).await?;

    tracing::info!("querying matchups_* views");
    let overall = export::query::fetch_matchups_1v1(&client).await?;
    let by_map = export::query::fetch_matchups_1v1_by_map(&client).await?;
    let by_elo = export::query::fetch_matchups_1v1_by_elo(&client).await?;
    let team = export::query::fetch_matchups_team(&client).await?;

    fs::create_dir_all(out).with_context(|| format!("failed to create {}", out.display()))?;

    let civ_matchups = export::build_civ_matchups(&overall);
    let civ_matchups_by_map = export::build_civ_matchups_by_map(&by_map);
    let civ_matchups_by_elo = export::build_civ_matchups_by_elo(&by_elo);
    let civ_matchups_team = export::build_civ_matchups_team(&team);

    write_json_pretty(out, "civ-matchups.json", &civ_matchups)?;
    write_json_pretty(out, "civ-matchups-by-map.json", &civ_matchups_by_map)?;
    write_json_pretty(out, "civ-matchups-by-elo.json", &civ_matchups_by_elo)?;
    write_json_pretty(out, "civ-matchups-team.json", &civ_matchups_team)?;

    tracing::info!(
        civs_1v1 = civ_matchups.civs.len(),
        civs_by_map = civ_matchups_by_map.civs.len(),
        civs_by_elo = civ_matchups_by_elo.civs.len(),
        civs_team = civ_matchups_team.civs.len(),
        out = %out.display(),
        "matchups export complete"
    );
    Ok(())
}

async fn run_benchmark_export(out: &Path, database_url: &str) -> anyhow::Result<()> {
    let client = connect(database_url).await?;

    tracing::info!("streaming benchmark_ageup/benchmark_vils/benchmark_ecotech views");
    let ageup = export::query::fetch_benchmark_ageup(&client).await?;
    let vils = export::query::fetch_benchmark_vils(&client).await?;
    let eco = export::query::fetch_benchmark_ecotech(&client).await?;
    let ageup_rows = ageup.len();
    let vils_rows = vils.len();
    let eco_rows = eco.len();

    let doc = export::build_benchmark(&ageup, &vils);
    let eco_doc = export::build_eco_benchmark(&eco);

    fs::create_dir_all(out).with_context(|| format!("failed to create {}", out.display()))?;
    write_json_pretty(out, "benchmark.json", &doc)?;
    write_json_pretty(out, "benchmark-eco.json", &eco_doc)?;

    tracing::info!(
        ageup_rows,
        vils_rows,
        eco_rows,
        civs = doc.civs.len(),
        eco_techs = eco_doc.techs.len(),
        out = %out.display(),
        "benchmark export complete"
    );
    Ok(())
}

async fn run_civ_meta_export(out: &Path, database_url: &str) -> anyhow::Result<()> {
    let client = connect(database_url).await?;

    tracing::info!("querying civ_meta views");
    let inputs = CivMetaInputs {
        civ_slugs: export::query::fetch_all_civ_slugs(&client).await?,
        overall_and_elo: export::query::fetch_civ_meta(&client).await?,
        by_map: export::query::fetch_by_map(&client).await?,
        by_patch: export::query::fetch_by_patch(&client).await?,
        openings: export::query::fetch_openings(&client).await?,
        ageup: export::query::fetch_ageup(&client).await?,
        patch_axis: export::query::fetch_patch_axis(&client).await?,
        source_date: export::query::fetch_source_date(&client).await?,
    };
    let civ_count = inputs.civ_slugs.len();
    let doc = build_doc(inputs);
    let populated_1v1 = doc.civs.values().filter(|c| c.ladder_1v1.is_some()).count();
    let populated_team = doc.civs.values().filter(|c| c.team.is_some()).count();

    fs::create_dir_all(out).with_context(|| format!("failed to create {}", out.display()))?;
    write_json_pretty(out, "civ-meta.json", &doc)?;

    tracing::info!(
        civs = civ_count,
        populated_1v1,
        populated_team,
        patches = doc.patches.len(),
        out = %out.display(),
        "civ-meta export complete"
    );
    Ok(())
}

/// Serializes `value` as pretty JSON (trailing newline, matching every committed `src/data`/
/// `public` JSON file's own convention) and writes it to `out/name`. Shared by every exporter
/// subcommand — `out` is caller-created (`fs::create_dir_all`) before the first call.
fn write_json_pretty<T: Serialize>(out: &Path, name: &str, value: &T) -> anyhow::Result<()> {
    let path = out.join(name);
    let json = serde_json::to_string_pretty(value)
        .with_context(|| format!("failed to serialize {name}"))?;
    fs::write(&path, format!("{json}\n"))
        .with_context(|| format!("failed to write {}", path.display()))
}

fn run_shape_diff(a_path: &Path, b_path: &Path) -> i32 {
    let read = |path: &Path| -> anyhow::Result<serde_json::Value> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&text)
            .with_context(|| format!("failed to parse {} as JSON", path.display()))
    };
    let (a, b) = match (read(a_path), read(b_path)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(err), _) | (_, Err(err)) => {
            eprintln!("{err:#}");
            return 1;
        }
    };
    let diff = export::shape::shape_diff(&a, &b);
    if diff.is_empty() {
        println!("(no structural differences)");
        0
    } else {
        for line in &diff {
            println!("{line}");
        }
        1
    }
}
