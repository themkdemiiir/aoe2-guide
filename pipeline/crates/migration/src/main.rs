//! Own clap CLI + explicit `tracing` subscriber for the migration binary.
//!
//! Deliberately does NOT use `sea_orm_migration::cli::run_cli`: its built-in clap `Cli` exposes
//! `--database-url` / `-u`, backed by `env = "DATABASE_URL"` with no `hide`/`hide_env_values`, so
//! `--help` prints the live database URL (password included) — that leak already caused a real
//! password rotation once. Instead we read `DATABASE_URL` from the environment ourselves and
//! never put it in a clap arg, help string, or log line.

use anyhow::Context;
use clap::{Parser, Subcommand};
use migration::{Migrator, MigratorTrait};
use pipeline_core::redact_secret;
use sea_orm_migration::sea_orm::{ConnectOptions, Database};
use tracing_subscriber::EnvFilter;

/// Default log filter when `RUST_LOG` is unset: informational, with sqlx's own statement
/// logging (which sqlx routes through `tracing` under the `sqlx` target) quieted to warnings.
const DEFAULT_LOG_FILTER: &str = "info,sqlx=warn,sea_orm_migration=info";

#[derive(Parser)]
#[command(
    name = "migration",
    version,
    about = "Run SeaORM migrations for the AOE2 guide's PostgreSQL pipeline store"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Apply pending migrations (default: all)
    Up {
        /// Number of pending migrations to apply (default: all)
        #[arg(short = 'n', long = "num")]
        steps: Option<u32>,
    },
    /// Roll back applied migrations (default: all)
    Down {
        /// Number of applied migrations to roll back (default: all)
        #[arg(short = 'n', long = "num")]
        steps: Option<u32>,
    },
    /// Show the status of every migration (applied/pending)
    Status,
    /// Drop all tables and types, then reapply every migration
    Fresh,
    /// Roll back every migration, then reapply every migration
    Refresh,
    /// Roll back every applied migration
    Reset,
}

#[tokio::main]
async fn main() {
    init_tracing();

    // Parse args before touching the environment at all, so `--help`/`--version` are handled
    // entirely by clap (and exit immediately) without ever depending on `DATABASE_URL`.
    let cli = Cli::parse();

    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => {
            tracing::error!("DATABASE_URL is not set");
            std::process::exit(1);
        }
    };

    if let Err(err) = run(cli.command, database_url.clone()).await {
        let message = redact_secret(&format!("{err:#}"), &database_url);
        tracing::error!(error = %message, "migration command failed");
        std::process::exit(1);
    }
}

/// Initialize the global `tracing` subscriber: structured output to stderr, filtered by
/// `RUST_LOG` (falling back to [`DEFAULT_LOG_FILTER`] when unset/invalid).
fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER));

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .init();
}

async fn run(command: Command, database_url: String) -> anyhow::Result<()> {
    let connect_options = ConnectOptions::new(database_url)
        .sqlx_logging(true)
        .to_owned();

    let db = Database::connect(connect_options)
        .await
        .context("failed to connect to the database")?;

    match command {
        Command::Up { steps } => Migrator::up(&db, steps).await.context("up failed"),
        Command::Down { steps } => Migrator::down(&db, steps).await.context("down failed"),
        Command::Status => Migrator::status(&db).await.context("status failed"),
        Command::Fresh => Migrator::fresh(&db).await.context("fresh failed"),
        Command::Refresh => Migrator::refresh(&db).await.context("refresh failed"),
        Command::Reset => Migrator::reset(&db).await.context("reset failed"),
    }
}
