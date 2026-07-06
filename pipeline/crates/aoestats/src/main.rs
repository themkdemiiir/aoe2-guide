//! Thin CLI over the `aoestats` library: `import` loads either one explicit `m_*`/`p_*` pair
//! (`--matches`/`--players`) or every pair discovered in a directory (`--dir`); `import-ages`
//! (Task M4b) loads one `p_*.parquet` file's `replay_summary_raw` per-age summaries into
//! `match_ages` — independently, since that table has no FK to `matches`.
//!
//! Like `ingest`'s and `migration`'s binaries, `DATABASE_URL` is read from the environment via
//! `pipeline_core::cli::database_url` and never placed in a clap arg, help string, or log line.
//!
//! `--dir` exists for the eventual M6 full-archive run, but THIS task's own live smoke
//! deliberately uses `--matches`/`--players` on a single sample pair — see
//! `.superpowers/sdd/task-M4a-aoestats-report.md`.

use std::path::{Path, PathBuf};

use anyhow::Context;
use aoestats::{import_ages, import_pair, AgesImportStats, ImportStats};
use clap::{Parser, Subcommand};
use pipeline_core::cli::{database_url, init_tracing, log_error_and_code};
use pipeline_core::redact_secret;
use tokio_postgres::NoTls;

/// Default log filter when `RUST_LOG` is unset.
const DEFAULT_LOG_FILTER: &str = "info";

#[derive(Parser)]
#[command(
    name = "aoestats",
    version,
    about = "Import the aoestats parquet archive (matches/players) into the AOE2 guide's PostgreSQL pipeline store (source=aoestats)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Import one `m_*.parquet`/`p_*.parquet` pair, or every pair found in a directory.
    Import {
        /// Directory containing `m_*.parquet` files with matching `p_<same-suffix>.parquet`
        /// partners. Mutually exclusive with `--matches`/`--players`.
        #[arg(long, conflicts_with_all = ["matches_file", "players_file"])]
        dir: Option<PathBuf>,
        /// Path to one `m_*.parquet` file. Requires `--players`.
        #[arg(long = "matches", requires = "players_file", value_name = "PATH")]
        matches_file: Option<PathBuf>,
        /// Path to the matching `p_*.parquet` file. Requires `--matches`.
        #[arg(long = "players", requires = "matches_file", value_name = "PATH")]
        players_file: Option<PathBuf>,
    },
    /// Import one `p_*.parquet` file's `replay_summary_raw` per-age summaries into `match_ages`
    /// (Task M4b). Independent of `import` — `match_ages` has no FK to `matches`, so this can run
    /// before, after, or without the matching `import` call. Requires `python3` on `PATH` (see
    /// `pipeline/py/aoestats_summaries.py`).
    ImportAges {
        /// Path to a `p_*.parquet` file.
        #[arg(long = "players", value_name = "PATH")]
        players_file: PathBuf,
    },
}

#[tokio::main]
async fn main() {
    init_tracing(DEFAULT_LOG_FILTER);

    // Parse args before touching the environment, so `--help`/`--version` never depend on
    // `DATABASE_URL`.
    let cli = Cli::parse();

    let secret = match database_url() {
        Ok(secret) => secret,
        Err(err) => {
            tracing::error!(error = %err, "failed to read DATABASE_URL");
            std::process::exit(1);
        }
    };

    if let Err(err) = run(cli, secret.expose()).await {
        std::process::exit(log_error_and_code(&err, &secret));
    }
}

async fn run(cli: Cli, database_url: &str) -> anyhow::Result<()> {
    let (mut client, connection) = tokio_postgres::connect(database_url, NoTls)
        .await
        .context("failed to connect to the database")?;

    let database_url_for_log = database_url.to_owned();
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            let message = redact_secret(&format!("{err:#}"), &database_url_for_log);
            tracing::error!(error = %message, "database connection closed with an error");
        }
    });

    match cli.command {
        Command::Import {
            dir,
            matches_file,
            players_file,
        } => run_import(&mut client, dir, matches_file, players_file).await,
        Command::ImportAges { players_file } => run_import_ages(&mut client, &players_file).await,
    }
}

async fn run_import(
    client: &mut tokio_postgres::Client,
    dir: Option<PathBuf>,
    matches_file: Option<PathBuf>,
    players_file: Option<PathBuf>,
) -> anyhow::Result<()> {
    let pairs = match (dir, matches_file, players_file) {
        (Some(dir), None, None) => discover_pairs(&dir)?,
        (None, Some(matches_file), Some(players_file)) => vec![(matches_file, players_file)],
        _ => anyhow::bail!("pass either --dir <DIR> or both --matches <PATH> --players <PATH>"),
    };
    if pairs.is_empty() {
        anyhow::bail!("no m_*.parquet/p_*.parquet pairs found");
    }

    let mut total = ImportStats::default();
    for (matches_path, players_path) in &pairs {
        tracing::info!(
            matches = %matches_path.display(),
            players = %players_path.display(),
            "importing aoestats pair"
        );
        let stats = import_pair(client, matches_path, players_path)
            .await
            .with_context(|| {
                format!(
                    "import_pair failed for {} / {}",
                    matches_path.display(),
                    players_path.display()
                )
            })?;
        total.matches_inserted += stats.matches_inserted;
        total.players_inserted += stats.players_inserted;
        total.unknown_map_slugs.extend(stats.unknown_map_slugs);
        total.unknown_civ_slugs.extend(stats.unknown_civ_slugs);
        total.matches_missing_game_id += stats.matches_missing_game_id;
        total.players_missing_identity += stats.players_missing_identity;
    }

    tracing::info!(
        pairs = pairs.len(),
        matches_inserted = total.matches_inserted,
        players_inserted = total.players_inserted,
        unknown_map_slugs = total.unknown_map_slugs.len(),
        unknown_civ_slugs = total.unknown_civ_slugs.len(),
        matches_missing_game_id = total.matches_missing_game_id,
        players_missing_identity = total.players_missing_identity,
        "aoestats import run complete"
    );
    Ok(())
}

async fn run_import_ages(
    client: &mut tokio_postgres::Client,
    players_path: &Path,
) -> anyhow::Result<()> {
    tracing::info!(players = %players_path.display(), "importing aoestats ages");
    let stats: AgesImportStats = import_ages(client, players_path)
        .await
        .with_context(|| format!("import_ages failed for {}", players_path.display()))?;

    tracing::info!(
        source_rows = stats.source_rows,
        staged_rows = stats.staged_rows,
        ages_deleted = stats.ages_deleted,
        ages_inserted = stats.ages_inserted,
        unknown_civ_slugs = stats.unknown_civ_slugs.len(),
        rows_missing_identity = stats.rows_missing_identity,
        "aoestats ages import run complete"
    );
    Ok(())
}

/// Scans `dir` for `m_*.parquet` files and pairs each with its same-suffix `p_*.parquet` partner
/// (`m_2022-08-28.parquet` <-> `p_2022-08-28.parquet`). A matches file with no players partner is
/// logged and skipped, not a hard error — a partial archive directory shouldn't block importing
/// every pair that IS complete.
fn discover_pairs(dir: &Path) -> anyhow::Result<Vec<(PathBuf, PathBuf)>> {
    let mut pairs = Vec::new();
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory {}", dir.display()))?;

    // Carry the already-extracted `file_name` alongside its `PathBuf` from the start, rather than
    // re-deriving it after sorting — avoids an `.expect()` on "this UTF-8 check already passed"
    // (no `unwrap`/`expect`/`panic` in this crate's paths, CLI included).
    let mut matches_files: Vec<(PathBuf, String)> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_owned();
            (name.starts_with("m_") && name.ends_with(".parquet")).then_some((path, name))
        })
        .collect();
    matches_files.sort();

    for (matches_path, file_name) in matches_files {
        let suffix = &file_name["m_".len()..];
        let players_path = dir.join(format!("p_{suffix}"));
        if players_path.is_file() {
            pairs.push((matches_path, players_path));
        } else {
            tracing::warn!(
                matches = %matches_path.display(),
                expected_players = %players_path.display(),
                "no matching p_*.parquet partner found — skipping this date"
            );
        }
    }
    Ok(pairs)
}
