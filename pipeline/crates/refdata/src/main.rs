//! Thin CLI over the `refdata` library: writes `unit-stats.json` AND `game-facts.json`, both
//! derived from the committed aoe2techtree source slices baked into this binary at compile time
//! (see `lib.rs`'s doc) — no other input needed, so there is no source-path flag to get wrong.
//!
//! Unlike every other pipeline binary, this one touches no database and needs no `DATABASE_URL` —
//! it's a pure local-file transform, so `main` is synchronous (no `#[tokio::main]`) and only pulls
//! in `pipeline_core::cli::init_tracing` from the shared bootstrap.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::Parser;
use pipeline_core::cli::init_tracing;
use serde::Serialize;

/// Default log filter when `RUST_LOG` is unset.
const DEFAULT_LOG_FILTER: &str = "info";

#[derive(Parser)]
#[command(
    name = "refdata",
    version,
    about = "Derive src/data/unit-stats.json + game-facts.json from the committed aoe2techtree reference-data slices"
)]
struct Cli {
    /// Directory to write `unit-stats.json` + `game-facts.json` into (created if missing). NEVER
    /// `src/data` for this task — the Astro site's committed files stay untouched until an explicit
    /// cutover.
    #[arg(long, value_name = "DIR")]
    out: PathBuf,
}

fn main() {
    init_tracing(DEFAULT_LOG_FILTER);
    let cli = Cli::parse();

    if let Err(err) = run(&cli.out) {
        tracing::error!(error = %format!("{err:#}"), "refdata generation failed");
        std::process::exit(1);
    }
}

fn run(out_dir: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("failed to create output directory {}", out_dir.display()))?;

    let unit_stats = refdata::unit_stats::build_from_committed_reference_data()
        .context("building unit-stats.json")?;
    write_json(&out_dir.join("unit-stats.json"), &unit_stats)?;

    let game_facts = refdata::game_facts::build_from_committed_reference_data()
        .context("building game-facts.json")?;
    write_json(&out_dir.join("game-facts.json"), &game_facts)?;

    tracing::info!(
        units = unit_stats.units.len(),
        facts = game_facts.units.len(),
        out = %out_dir.display(),
        "wrote unit-stats.json and game-facts.json"
    );
    Ok(())
}

/// Pretty-prints `doc` with a trailing newline, matching the JS generators'
/// `` `${JSON.stringify(output, null, 2)}\n` `` exactly (serde_json's default `PrettyFormatter`
/// also uses 2-space indentation).
fn write_json<T: Serialize>(path: &Path, doc: &T) -> anyhow::Result<()> {
    let mut json = serde_json::to_string_pretty(doc)
        .with_context(|| format!("failed to serialize {}", path.display()))?;
    json.push('\n');
    fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
