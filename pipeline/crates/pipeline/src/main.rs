//! `pipeline crawl` — the CLI over [`pipeline::crawl`] (the M6 discover -> download -> parse ->
//! ingest run loop). Like `ingest`/`migration`'s binaries, `DATABASE_URL` is read from the
//! environment (never a clap arg/help string/log line) — see `pipeline_core::cli`'s doc for why
//! (a past leak caused a real password rotation).
//!
//! `--dry-run` skips reading `DATABASE_URL` (and connecting to Postgres) entirely — it needs
//! neither to validate CLI/manifest wiring. See [`pipeline::CrawlConfig`]'s doc for exactly which
//! operations `--dry-run` and an omitted `--profile-id` each skip.
//!
//! `pipeline reparse` is a second, unrelated entry point over [`pipeline::reparse_dir`]: it reads
//! the raw `.aoe2record.zst` archive `crawl`'s `--raw-dir` writes and re-parses it, entirely
//! offline (no `DATABASE_URL`, no Postgres) — see that function's doc for its parse-and-count-only
//! scope.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use fetch::{FetchClient, SqliteManifest};
use pipeline::{CrawlConfig, FetchSource, ImportShardsConfig, PgSink};
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
/// Default root for the raw `.aoe2record.zst` archive (`pipeline::RawArchive`) — see its module
/// doc for the on-disk layout.
const DEFAULT_RAW_DIR: &str = "data-cache/replays/raw";

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
    /// Re-parse every archived raw `.aoe2record.zst` under `--raw-dir`, proving the raw corpus is
    /// re-usable by a (possibly improved) parser. Parse-and-count ONLY — never re-ingests into
    /// Postgres and never reads `DATABASE_URL` (see `pipeline::reparse_dir`'s doc for the scope).
    Reparse(ReparseArgs),
    /// Migrate the OLD parsed-replay shard corpus into the live Postgres pipeline as
    /// `source='replay'`, re-deriving the full enrichment. See `pipeline::import_shards`'s module
    /// doc for the full design.
    ImportShards(ImportShardsArgs),
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

    /// Root directory for the raw `.aoe2record.zst` archive — every successfully downloaded
    /// replay (even ones that fail to parse) is persisted here so a future parser can re-extract
    /// more. See `pipeline::RawArchive`'s module doc for the on-disk layout. Ignored when
    /// `--dry-run` or `--no-raw` is set.
    #[arg(long, default_value = DEFAULT_RAW_DIR)]
    raw_dir: PathBuf,

    /// Disable raw-replay archiving entirely (no disk writes). `--dry-run` already never writes
    /// raw regardless of this flag — see `CrawlConfig::raw_dir`'s doc.
    #[arg(long)]
    no_raw: bool,
}

#[derive(clap::Args)]
struct ReparseArgs {
    /// Root directory of the raw `.aoe2record.zst` archive to read back (matches `pipeline crawl
    /// --raw-dir`).
    #[arg(long, default_value = DEFAULT_RAW_DIR)]
    raw_dir: PathBuf,

    /// Stop after this many archived replays (0 = no limit).
    #[arg(long, default_value_t = 0)]
    limit: usize,
}

/// Default shard directory — matches the OLD `scripts/data-pipeline/replay-rs` extractor's
/// output location (see `pipeline::import_shards`'s module doc).
const DEFAULT_SHARDS_DIR: &str = "data-cache/replays/shards";
/// Default scratch dir for the decompressed shards + staging DuckDB + per-chunk temp + cursor.
const DEFAULT_WORK_DIR: &str = "data-cache/replays/shard-staging";

#[derive(clap::Args)]
struct ImportShardsArgs {
    /// Directory containing `{meta,players,events,ages}.ndjson.gz`.
    #[arg(long, default_value = DEFAULT_SHARDS_DIR)]
    shards_dir: PathBuf,

    /// Path to the read-only DuckDB snapshot carrying `games` (played_at/ladder/rating per
    /// (match_id, profile_id)) — the seed data the shards themselves don't carry.
    #[arg(long)]
    snapshot: PathBuf,

    /// Path to the `duckdb` CLI binary (not always on `PATH` — e.g. `~/bin/duckdb`).
    #[arg(long, default_value = "duckdb")]
    duckdb_bin: PathBuf,

    /// Scratch dir for the decompressed shards, staging DuckDB, per-chunk temp files, and the
    /// resume cursor. Needs ~110 GB free for the full corpus's decompressed `events`.
    #[arg(long, default_value = DEFAULT_WORK_DIR)]
    work_dir: PathBuf,

    /// Stop after this many distinct `match_id`s (sorted). 0 = every readable match.
    #[arg(long, default_value_t = 200)]
    limit: usize,

    /// Distinct matches per DuckDB range-`COPY` round-trip (bounds per-chunk RAM).
    #[arg(long, default_value_t = 2000)]
    chunk_size: usize,

    /// Matches per `ingest_batch` transaction.
    #[arg(long, default_value_t = 40)]
    batch_size: usize,

    /// A batch taking longer than this many seconds is treated as a load-safety signal — pause
    /// `--pause-secs` before the next one.
    #[arg(long, default_value_t = 10)]
    slow_batch_secs: u64,

    /// How long to pause (seconds) after a slow batch — see `--slow-batch-secs`.
    #[arg(long, default_value_t = 5)]
    pause_secs: u64,

    /// Force a full staging rebuild (re-decompress + reload) even if the `.done` markers exist.
    #[arg(long)]
    rebuild_staging: bool,
}

#[tokio::main]
async fn main() {
    init_tracing(DEFAULT_LOG_FILTER);

    // Parse args before touching the environment at all, so `--help`/`--version` are handled
    // entirely by clap (and exit immediately) without ever depending on `DATABASE_URL` — same
    // discipline as `ingest`/`migration`'s binaries.
    let cli = Cli::parse();

    match cli.command {
        Command::Crawl(args) => run_crawl_command(args).await,
        // Synchronous and never touches `DATABASE_URL`/Postgres (see `pipeline::reparse_dir`'s
        // scope doc) — called directly rather than via `spawn_blocking`: this is a one-shot CLI
        // invocation with nothing else for the runtime to interleave it with.
        Command::Reparse(args) => {
            if let Err(err) = run_reparse(args) {
                tracing::error!(error = %format!("{err:#}"), "pipeline command failed");
                std::process::exit(1);
            }
        }
        Command::ImportShards(args) => run_import_shards_command(args).await,
    }
}

async fn run_crawl_command(args: CrawlArgs) {
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
        // `--dry-run` never archives raw replays regardless of `--raw-dir`/`--no-raw` — the
        // "Process" step that could save one never runs in a dry-run anyway (see
        // `CrawlConfig::raw_dir`'s doc), but this makes that explicit rather than incidental.
        raw_dir: if dry_run || args.no_raw {
            None
        } else {
            Some(args.raw_dir.clone())
        },
    }
}

/// `pipeline reparse` — see `pipeline::reparse_dir`'s doc for exactly what this does (and does
/// not) do.
fn run_reparse(args: ReparseArgs) -> anyhow::Result<()> {
    let limit = (args.limit > 0).then_some(args.limit);
    let summary = pipeline::reparse_dir(&args.raw_dir, limit).context("reparse failed")?;
    tracing::info!(
        found = summary.found,
        parsed_ok = summary.parsed_ok,
        parsed_err = summary.parsed_err,
        bytes_read = summary.bytes_read,
        "reparse complete — parse-and-count only, no re-ingest (see `pipeline::reparse_dir`'s doc)"
    );
    Ok(())
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
        raw_saved = summary.raw_saved,
        raw_already_present = summary.raw_already_present,
        raw_failed = summary.raw_failed,
        raw_bytes_written = summary.raw_bytes_written,
        "crawl complete"
    );
    Ok(())
}

async fn run_import_shards_command(args: ImportShardsArgs) {
    let secret = match database_url() {
        Ok(secret) => secret,
        Err(err) => {
            tracing::error!(error = %err, "failed to read DATABASE_URL");
            std::process::exit(1);
        }
    };

    if let Err(err) = run_import_shards(args, secret.expose()).await {
        std::process::exit(log_error_and_code(&err, &secret));
    }
}

async fn run_import_shards(args: ImportShardsArgs, database_url: &str) -> anyhow::Result<()> {
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

    let cfg = ImportShardsConfig {
        shards_dir: args.shards_dir,
        snapshot_path: args.snapshot,
        duckdb_bin: args.duckdb_bin,
        work_dir: args.work_dir,
        limit: args.limit,
        chunk_size: args.chunk_size.max(1),
        batch_size: args.batch_size.max(1),
        slow_batch: Duration::from_secs(args.slow_batch_secs),
        pause: Duration::from_secs(args.pause_secs),
        rebuild_staging: args.rebuild_staging,
    };

    let summary = pipeline::import_shards(&cfg, &mut client)
        .await
        .context("import_shards failed")?;

    tracing::info!(?summary, "import-shards complete");
    Ok(())
}
