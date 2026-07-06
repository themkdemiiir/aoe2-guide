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
            let secret = match database_url() {
                Ok(secret) => secret,
                Err(err) => {
                    tracing::error!(error = %err, "failed to read DATABASE_URL");
                    std::process::exit(1);
                }
            };
            if let Err(err) = run_civ_meta_export(&out, secret.expose()).await {
                std::process::exit(log_error_and_code(&err, &secret));
            }
        }
    }
}

async fn run_civ_meta_export(out: &Path, database_url: &str) -> anyhow::Result<()> {
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
    let path = out.join("civ-meta.json");
    let json = serde_json::to_string_pretty(&doc).context("failed to serialize civ-meta.json")?;
    fs::write(&path, format!("{json}\n"))
        .with_context(|| format!("failed to write {}", path.display()))?;

    tracing::info!(
        civs = civ_count,
        populated_1v1,
        populated_team,
        patches = doc.patches.len(),
        path = %path.display(),
        "civ-meta export complete"
    );
    Ok(())
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
