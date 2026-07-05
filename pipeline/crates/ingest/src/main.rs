//! Thin CLI over the `ingest` library: reads a [`ingest::ReplayBatch`] as JSON from `--from
//! <path>` (or stdin) and bulk-loads it via [`ingest::ingest_batch`].
//!
//! Like the `migration` binary, `DATABASE_URL` is read from the environment (via
//! `pipeline_core::cli::database_url`) and never placed in a clap arg, help string, or log line —
//! connect errors are redacted before they reach a log line (that leak caused a real password
//! rotation once).

use std::io::Read as _;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use ingest::{ingest_batch, ReplayBatch};
use pipeline_core::cli::{database_url, init_tracing, log_error_and_code};
use pipeline_core::redact_secret;
use tokio_postgres::NoTls;

/// Default log filter when `RUST_LOG` is unset.
const DEFAULT_LOG_FILTER: &str = "info";

#[derive(Parser)]
#[command(
    name = "ingest",
    version,
    about = "Bulk-load a JSON ReplayBatch into the AOE2 guide's PostgreSQL pipeline store"
)]
struct Cli {
    /// Path to a JSON-encoded ReplayBatch. Reads stdin when omitted.
    #[arg(long = "from")]
    from: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    init_tracing(DEFAULT_LOG_FILTER);

    // Parse args before touching the environment, so `--help`/`--version` never depend on
    // `DATABASE_URL` (and so it can never surface in help output).
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
    let batch = read_batch(cli.from.as_deref())?;

    let (mut client, connection) = tokio_postgres::connect(database_url, NoTls)
        .await
        .context("failed to connect to the database")?;

    // Drive the connection on its own task, as tokio_postgres requires; log (redacted) if it
    // ever fails instead of silently dropping the error.
    let database_url_for_log = database_url.to_owned();
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            let message = redact_secret(&format!("{err:#}"), &database_url_for_log);
            tracing::error!(error = %message, "database connection closed with an error");
        }
    });

    let stats = ingest_batch(&mut client, &batch)
        .await
        .context("ingest_batch failed")?;

    tracing::info!(
        matches_inserted = stats.matches_inserted,
        matches_skipped = stats.matches_skipped,
        players = stats.players,
        events = stats.events,
        ages = stats.ages,
        "ingest complete"
    );
    Ok(())
}

/// Read the JSON [`ReplayBatch`] from `--from <path>`, or stdin when omitted.
fn read_batch(from: Option<&std::path::Path>) -> anyhow::Result<ReplayBatch> {
    let raw = match from {
        Some(path) => std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?,
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("failed to read ReplayBatch JSON from stdin")?;
            buf
        }
    };

    serde_json::from_str(&raw).context("failed to parse ReplayBatch JSON")
}
