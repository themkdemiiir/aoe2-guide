//! `pipeline crawl` — the CLI over [`pipeline::crawl`] (the M6 discover -> download -> parse ->
//! ingest run loop). Like `ingest`/`migration`'s binaries, `DATABASE_URL` is read from the
//! environment (never a clap arg/help string/log line) — see `pipeline_core::cli`'s doc for why
//! (a past leak caused a real password rotation).
//!
//! `--dry-run` skips reading `DATABASE_URL` (and connecting to Postgres) entirely — it needs
//! neither to validate CLI/manifest wiring. See [`pipeline::CrawlConfig`]'s doc for exactly which
//! operations `--dry-run` and an omitted `--profile-id` each skip.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, Subcommand};
use fetch::{FetchClient, SqliteManifest};
use pipeline::{CrawlConfig, FetchSource, PgSink};
use pipeline_core::cli::{database_url, init_tracing, log_error_and_code};
use pipeline_core::{redact_secret, ProfileId};
use tokio_postgres::NoTls;
use tokio_util::sync::CancellationToken;

/// Default log filter when `RUST_LOG` is unset.
const DEFAULT_LOG_FILTER: &str = "info";

/// Mirrors `fetch::config::MAX_CONCURRENCY` (private to that crate) — the same sane default for
/// how many connections the Relic API fast path tolerates at once, reused here as this loop's own
/// per-match worker concurrency bound (see `pipeline::crawl`'s "Two independent bounds" doc).
const DEFAULT_CONCURRENCY: usize = 4;
/// Mirrors `fetch::config::REPLAYFILES_PER_MIN` (private to that crate).
const DEFAULT_RATE_PER_MIN: u32 = 100;
/// Mirrors the old `replay-rs` CLI's own default manifest filename.
const DEFAULT_MANIFEST_PATH: &str = "manifest.sqlite";

#[derive(Parser)]
#[command(
    name = "pipeline",
    version,
    about = "AOE2 guide data pipeline: discover -> download -> parse -> ingest replay crawl"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Discover a profile's recent ranked replays and download + parse + ingest the eligible ones.
    Crawl(CrawlArgs),
}

#[derive(clap::Args)]
struct CrawlArgs {
    /// Relic profile id to discover recent matches for. Omit to skip discovery (no network call
    /// for it) and only attempt matches already sitting in the manifest.
    #[arg(long)]
    profile_id: Option<i64>,

    /// Max matches to attempt this run (`take_ready`'s own LIMIT).
    #[arg(long, default_value_t = 50)]
    limit: usize,

    /// Max concurrent in-flight match-processing tasks — also used as the underlying Relic HTTP
    /// client's own concurrency bound.
    #[arg(long, default_value_t = DEFAULT_CONCURRENCY)]
    concurrency: usize,

    /// Steady request rate (requests/min) the underlying HTTP client throttles to (GCRA).
    #[arg(long, default_value_t = DEFAULT_RATE_PER_MIN)]
    rate: u32,

    /// Path to the resumable SQLite manifest (created if absent).
    #[arg(long, default_value = DEFAULT_MANIFEST_PATH)]
    manifest: PathBuf,

    /// Discover + plan only: never download, parse, or ingest — and never reads DATABASE_URL or
    /// connects to Postgres. Safe for wiring validation (e.g. Dagster's inert partition check).
    #[arg(long)]
    dry_run: bool,
}

#[tokio::main]
async fn main() {
    init_tracing(DEFAULT_LOG_FILTER);

    // Parse args before touching the environment at all, so `--help`/`--version` are handled
    // entirely by clap (and exit immediately) without ever depending on `DATABASE_URL` — same
    // discipline as `ingest`/`migration`'s binaries.
    let cli = Cli::parse();
    let Command::Crawl(args) = cli.command;

    if args.dry_run {
        if let Err(err) = run_dry_run(args).await {
            tracing::error!(error = %format!("{err:#}"), "pipeline command failed");
            std::process::exit(1);
        }
        return;
    }

    let secret = match database_url() {
        Ok(secret) => secret,
        Err(err) => {
            tracing::error!(error = %err, "failed to read DATABASE_URL");
            std::process::exit(1);
        }
    };

    if let Err(err) = run_live(args, secret.expose()).await {
        std::process::exit(log_error_and_code(&err, &secret));
    }
}

fn spawn_ctrl_c_handler(cancel: CancellationToken) {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            tracing::info!("ctrl-c received — finishing in-flight work, then exiting");
            cancel.cancel();
        }
    });
}

fn open_manifest(path: &std::path::Path) -> anyhow::Result<SqliteManifest> {
    let path = path
        .to_str()
        .context("--manifest path must be valid UTF-8")?;
    SqliteManifest::open(path).context("failed to open the resumable manifest")
}

fn build_source(args: &CrawlArgs) -> anyhow::Result<Arc<FetchSource>> {
    let client = FetchClient::with_limits(args.rate, args.concurrency.max(1))
        .context("failed to build the Relic fetch client")?;
    Ok(Arc::new(FetchSource::new(client)))
}

fn crawl_config(args: &CrawlArgs, dry_run: bool) -> CrawlConfig {
    CrawlConfig {
        profile_id: args.profile_id.map(ProfileId),
        limit: args.limit,
        concurrency: args.concurrency.max(1),
        dry_run,
    }
}

async fn run_dry_run(args: CrawlArgs) -> anyhow::Result<()> {
    let manifest = open_manifest(&args.manifest)?;
    let source = build_source(&args)?;
    let config = crawl_config(&args, true);

    let cancel = CancellationToken::new();
    spawn_ctrl_c_handler(cancel.clone());

    let sink: Option<&mut PgSink<'_>> = None;
    let (_manifest, summary) = pipeline::crawl(source, manifest, sink, &config, &cancel)
        .await
        .context("crawl failed")?;

    tracing::info!(?summary, "dry-run complete");
    Ok(())
}

async fn run_live(args: CrawlArgs, database_url: &str) -> anyhow::Result<()> {
    let manifest = open_manifest(&args.manifest)?;
    let source = build_source(&args)?;
    let config = crawl_config(&args, false);

    let (mut client, connection) = tokio_postgres::connect(database_url, NoTls)
        .await
        .context("failed to connect to the database")?;

    // Drive the connection on its own task, as tokio_postgres requires; log (redacted) if it ever
    // fails instead of silently dropping the error — same pattern as `ingest`'s binary.
    let database_url_for_log = database_url.to_owned();
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            let message = redact_secret(&format!("{err:#}"), &database_url_for_log);
            tracing::error!(error = %message, "database connection closed with an error");
        }
    });

    let cancel = CancellationToken::new();
    spawn_ctrl_c_handler(cancel.clone());

    let mut sink = PgSink::new(&mut client);
    let (_manifest, summary) = pipeline::crawl(source, manifest, Some(&mut sink), &config, &cancel)
        .await
        .context("crawl failed")?;

    tracing::info!(
        seeded = summary.seeded,
        planned = summary.planned,
        skipped_no_seed = summary.skipped_no_seed,
        cancelled_before_start = summary.cancelled_before_start,
        attempted = summary.attempted,
        succeeded = summary.succeeded,
        failed = summary.failed,
        "crawl complete"
    );
    Ok(())
}
