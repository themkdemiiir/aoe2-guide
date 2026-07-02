//! Fast in-process AoE2 replay-processing pipeline.
//!
//! A Rust port of `scripts/data-pipeline/replays/*.py` that parses replays with
//! the aoe2rec library directly (no subprocess, no JSON roundtrip) and downloads
//! + parses in parallel with rayon.
//!
//! Usage:
//!   replay-rs seed    <ids.csv|ids.txt> [--db <manifest.sqlite>] [--ladder L] [--played-at TS]
//!   replay-rs run     [--db <manifest.sqlite>] [--out <dir>] [--threads N] [--limit M]
//!   replay-rs bench   <dir of .aoe2record> [--threads N] [--repeat N]
//!   replay-rs analyze <file.aoe2record>|--match-id N [--you NAME] [--json]
//!
//! Defaults: --db ./manifest.sqlite  --out ./shards  --threads 12

mod api;
mod extract;
mod manifest;
mod pipeline;
mod report;
mod seed;
mod store;

use anyhow::{bail, Result};
use std::path::Path;

const DEFAULT_DB: &str = "manifest.sqlite";
const DEFAULT_OUT: &str = "shards";
const DEFAULT_THREADS: usize = 12;
const DEFAULT_LOOKAHEAD: usize = 400;

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (subcmd, rest) = match args.split_first() {
        Some((c, r)) => (c.as_str(), r),
        None => {
            print_usage();
            bail!("no subcommand given");
        }
    };

    match subcmd {
        "seed" => cmd_seed(rest),
        "run" => cmd_run(rest),
        "bench" => cmd_bench(rest),
        "analyze" => cmd_analyze(rest),
        "-h" | "--help" | "help" => {
            print_usage();
            Ok(())
        }
        other => {
            print_usage();
            bail!("unknown subcommand: {other}");
        }
    }
}

/// Parse-only throughput benchmark: extract every `.aoe2record` in a directory,
/// `--repeat N` times, in parallel across `--threads` cores. Isolates the
/// in-process parse+extract cost from network/IO so we can report replays/sec.
fn cmd_bench(args: &[String]) -> Result<()> {
    use rayon::prelude::*;
    use std::time::Instant;

    let mut dir: Option<String> = None;
    let mut threads = DEFAULT_THREADS;
    let mut repeat = 1usize;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--threads" => threads = take_value(args, &mut i, "--threads")?.parse()?,
            "--repeat" => repeat = take_value(args, &mut i, "--repeat")?.parse()?,
            v if !v.starts_with("--") => dir = Some(v.to_string()),
            other => bail!("bench: unknown flag {other}"),
        }
        i += 1;
    }
    let dir = dir.ok_or_else(|| anyhow::anyhow!("bench: missing <dir of .aoe2record> argument"))?;

    // Load all replay bytes up front so the timed loop is pure CPU.
    let mut blobs: Vec<(i64, bytes::Bytes)> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("aoe2record") {
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            blobs.push((id, bytes::Bytes::from(std::fs::read(&path)?)));
        }
    }
    if blobs.is_empty() {
        bail!("bench: no .aoe2record files in {dir}");
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()?;

    let total = blobs.len() * repeat;
    let t0 = Instant::now();
    let events: usize = pool.install(|| {
        (0..repeat)
            .into_par_iter()
            .flat_map(|_| blobs.par_iter())
            .map(|(id, bytes)| match extract::extract(*id, bytes.clone()) {
                Ok(d) => d.events.len(),
                Err(_) => 0,
            })
            .sum()
    });
    let secs = t0.elapsed().as_secs_f64();
    println!(
        "bench: {total} parses in {secs:.3}s = {:.1} replays/s, {:.2}M events/s ({threads} threads, {} unique replays x{repeat})",
        total as f64 / secs,
        events as f64 / secs / 1e6,
        blobs.len()
    );
    Ok(())
}

fn cmd_seed(args: &[String]) -> Result<()> {
    let mut db = DEFAULT_DB.to_string();
    let mut path: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                db = take_value(args, &mut i, "--db")?;
            }
            // --ladder / --played-at are accepted for CLI parity; per-row values
            // from the seed file take precedence. (Reserved for future use.)
            "--ladder" => {
                let _ = take_value(args, &mut i, "--ladder")?;
            }
            "--played-at" => {
                let _ = take_value(args, &mut i, "--played-at")?;
            }
            v if !v.starts_with("--") => path = Some(v.to_string()),
            other => bail!("seed: unknown flag {other}"),
        }
        i += 1;
    }
    let path = path.ok_or_else(|| anyhow::anyhow!("seed: missing <ids file> argument"))?;
    let (parsed, inserted) = seed::seed(&db, Path::new(&path))?;
    println!(
        "seeded {inserted} new match_ids into {db} ({parsed} parsed from {path}, \
         duplicates ignored)"
    );
    Ok(())
}

fn cmd_run(args: &[String]) -> Result<()> {
    let mut db = DEFAULT_DB.to_string();
    let mut out = DEFAULT_OUT.to_string();
    let mut threads = DEFAULT_THREADS;
    let mut limit: Option<usize> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => db = take_value(args, &mut i, "--db")?,
            "--out" => out = take_value(args, &mut i, "--out")?,
            "--threads" => threads = take_value(args, &mut i, "--threads")?.parse()?,
            "--limit" => limit = Some(take_value(args, &mut i, "--limit")?.parse()?),
            other => bail!("run: unknown flag {other}"),
        }
        i += 1;
    }
    pipeline::run(pipeline::RunConfig {
        db,
        out,
        threads,
        limit,
        batch_lookahead: DEFAULT_LOOKAHEAD,
    })
}

/// Where the replay bytes come from (CLI-side concern; the lib only sees &Savegame).
enum Input { File(std::path::PathBuf), MatchId(i64) }

/// `analyze <file.aoe2record | --match-id N> [--you NAME] [--json]` — post-game coaching report.
fn cmd_analyze(args: &[String]) -> Result<()> {
    use replay_rs::analyze::{self, YouSel};
    let mut input: Option<Input> = None;
    let mut you = YouSel::Auto;
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--match-id" => input = Some(Input::MatchId(take_value(args, &mut i, "--match-id")?.parse()?)),
            "--you" => you = YouSel::Name(take_value(args, &mut i, "--you")?),
            "--json" => json = true,
            v if !v.starts_with("--") => input = Some(Input::File(std::path::PathBuf::from(v))),
            other => bail!("analyze: unknown flag {other}"),
        }
        i += 1;
    }
    let input = input.ok_or_else(|| anyhow::anyhow!("analyze: need <file.aoe2record> or --match-id N"))?;
    let game = load_game(&input)?;
    let report = analyze::analyze(&game, &you)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report::render(&report));
    }
    Ok(())
}

/// File read or getReplayFiles->download — the IO the lib deliberately doesn't do.
fn load_game(input: &Input) -> Result<aoe2rec::Savegame> {
    use anyhow::anyhow;
    match input {
        Input::File(p) => {
            aoe2rec::Savegame::from_file(p).map_err(|e| anyhow!("parse {}: {e}", p.display()))
        }
        Input::MatchId(id) => {
            let client = api::build_client()?;
            let per = api::get_replay_files(&client, &[*id])?;
            let files = per.get(id).ok_or_else(|| {
                anyhow!("match {id}: expired or not found (replays age out after ~weeks)")
            })?;
            let best = api::best_file(files).ok_or_else(|| {
                anyhow!("match {id}: no uploaded replay (all players' files missing)")
            })?;
            let url = best.url.clone().ok_or_else(|| anyhow!("match {id}: replay has no url"))?;
            let bytes = api::download_replay(&client, &url)?;
            aoe2rec::Savegame::from_bytes(bytes).map_err(|e| anyhow!("parse match {id}: {e}"))
        }
    }
}

/// Consume the value following a `--flag`, advancing the index.
fn take_value(args: &[String], i: &mut usize, flag: &str) -> Result<String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("{flag} requires a value"))
}

fn print_usage() {
    eprintln!(
        "replay-rs — fast in-process AoE2 replay pipeline\n\
         \n\
         USAGE:\n  \
           replay-rs seed <ids.csv|ids.txt> [--db <manifest.sqlite>]\n  \
           replay-rs run [--db <manifest.sqlite>] [--out <dir>] [--threads N] [--limit M]\n  \
           replay-rs bench <dir of .aoe2record> [--threads N] [--repeat N]\n  \
           replay-rs analyze <file.aoe2record>|--match-id N [--you NAME] [--json]\n\
         \n\
         DEFAULTS: --db {DEFAULT_DB}  --out {DEFAULT_OUT}  --threads {DEFAULT_THREADS}"
    );
}
