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
//!   replay-rs analyze <file>|--match-id N|--latest [N|all] [--you NAME] [--profile-id P] [--json]
//!   replay-rs recent   --profile-id P [--limit N]
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
        "recent" => cmd_recent(rest),
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

/// --latest's optional value: bare flag = 1, a number = N, "all" = whatever
/// getRecentMatchHistory returns (~last 10).
enum Latest { N(usize), All }

fn parse_latest(v: Option<&str>) -> Result<Latest> {
    match v {
        None => Ok(Latest::N(1)),
        Some("all") => Ok(Latest::All),
        Some(n) => n.parse().map(Latest::N)
            .map_err(|_| anyhow::anyhow!("analyze: --latest takes a number or 'all', got {n}")),
    }
}

/// `analyze <file.aoe2record>|--match-id N|--latest [N|all] [--you NAME] [--profile-id P] [--json]`
/// — post-game coaching report.
fn cmd_analyze(args: &[String]) -> Result<()> {
    use replay_rs::analyze::{self, YouSel};
    let mut input: Option<Input> = None;
    let mut you = YouSel::Auto;
    let mut json = false;
    let mut latest: Option<Latest> = None;
    let mut profile: Option<i64> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--match-id" => input = Some(Input::MatchId(take_value(args, &mut i, "--match-id")?.parse()?)),
            "--you" => you = YouSel::Name(take_value(args, &mut i, "--you")?),
            "--json" => json = true,
            "--latest" => {
                // optional value: consume the next arg only if it isn't another flag
                let peek = args.get(i + 1).map(String::as_str).filter(|v| !v.starts_with("--"));
                if peek.is_some() { i += 1; }
                latest = Some(parse_latest(peek)?);
            }
            "--profile-id" => profile = Some(take_value(args, &mut i, "--profile-id")?.parse()?),
            v if !v.starts_with("--") => input = Some(Input::File(std::path::PathBuf::from(v))),
            other => bail!("analyze: unknown flag {other}"),
        }
        i += 1;
    }

    if let Some(latest) = latest {
        if input.is_some() {
            bail!("analyze: --latest conflicts with a file / --match-id input");
        }
        if !matches!(you, YouSel::Auto) {
            bail!("analyze: --you conflicts with --latest (identity comes from --profile-id)");
        }
        if let Latest::N(0) = latest {
            bail!("analyze: --latest 0 requests zero games — pass a positive count or 'all'");
        }
        let profile = resolve_profile(profile)?;
        let client = api::build_client()?;
        let recent = api::get_recent_matches(&client, profile)?;
        let take = match latest { Latest::All => recent.len(), Latest::N(n) => n.min(recent.len()) };
        if take == 0 {
            bail!("analyze: no recent ranked matches for profile {profile}");
        }
        let ids: Vec<i64> = recent.iter().take(take).map(|m| m.match_id).collect();
        let mut analyzed = 0usize;
        for chunk in ids.chunks(replay_rs::config::REPLAYFILES_BATCH) {
            let per = api::get_replay_files(&client, chunk)?;
            for id in chunk {
                let Some(files) = per.get(id) else {
                    eprintln!("match {id}: replay expired/aged out — skipped");
                    continue;
                };
                let Some(best) = api::best_file(files) else {
                    eprintln!("match {id}: no uploaded replay — skipped");
                    continue;
                };
                let Some(url) = best.url.clone() else {
                    eprintln!("match {id}: replay has no url — skipped");
                    continue;
                };
                let game = match api::download_replay(&client, &url)
                    .and_then(|b| aoe2rec::Savegame::from_bytes(b)
                        .map_err(|e| anyhow::anyhow!("parse: {e}")))
                {
                    Ok(g) => g,
                    Err(e) => { eprintln!("match {id}: {e:#} — skipped"); continue; }
                };
                // Err here = profile not a player in this replay (e.g. an oddball
                // history entry) — warn + skip like the other per-match failures.
                let report = match replay_rs::analyze::analyze(&game, &replay_rs::analyze::YouSel::ProfileId(profile)) {
                    Ok(r) => r,
                    Err(e) => { eprintln!("match {id}: {e:#} — skipped"); continue; }
                };
                if json {
                    println!("{}", serde_json::to_string(&report)?); // NDJSON: one line per game
                } else {
                    println!("\n═══ match {id} ═══");
                    print!("{}", report::render(&report));
                }
                analyzed += 1;
            }
        }
        if analyzed == 0 {
            bail!("analyze: none of the {take} recent matches had a downloadable replay \
                   — replays only exist when uploaded and age out (~2-4 weeks in practice)");
        }
        return Ok(());
    }

    if matches!(you, YouSel::Auto) {
        if let Some(p) = profile { you = YouSel::ProfileId(p); }
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
                anyhow!("match {id}: expired or not found — replays age out (~2-4 weeks in practice)")
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

/// --profile-id N, or the explicit AOE2_PROFILE_ID env; absence of both is a
/// loud error (no-defaults rule: we never guess whose games to fetch). A SET
/// but unparseable env var is also a loud error — never silently treated as unset.
fn resolve_profile(flag: Option<i64>) -> Result<i64> {
    if let Some(id) = flag {
        return Ok(id);
    }
    match std::env::var("AOE2_PROFILE_ID") {
        Ok(v) => v.parse().map_err(|_| {
            anyhow::anyhow!("AOE2_PROFILE_ID is set to {v:?}, which isn't a valid profile id number")
        }),
        Err(_) => Err(anyhow::anyhow!(
            "need --profile-id N (or AOE2_PROFILE_ID env). Find yours on aoe2insights.com / aoe2companion.com"
        )),
    }
}

/// Human "how long ago" from a seconds delta (no chrono dep — keep it light).
fn ago(secs: i64) -> String {
    let s = secs.max(0);
    if s < 3600 { format!("{}m ago", s / 60) }
    else if s < 86_400 { format!("{}h ago", s / 3600) }
    else { format!("{}d ago", s / 86_400) }
}

/// `recent --profile-id P [--limit N]` — list recent ranked games, newest first.
fn cmd_recent(args: &[String]) -> Result<()> {
    let mut profile: Option<i64> = None;
    let mut limit: Option<usize> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--profile-id" => profile = Some(take_value(args, &mut i, "--profile-id")?.parse()?),
            "--limit" => limit = Some(take_value(args, &mut i, "--limit")?.parse()?),
            other => bail!("recent: unknown flag {other}"),
        }
        i += 1;
    }
    let profile = resolve_profile(profile)?;
    let client = api::build_client()?;
    let matches = api::get_recent_matches(&client, profile)?;
    if matches.is_empty() {
        bail!("recent: the API returned no recent ranked matches for profile {profile}");
    }
    let civs = replay_rs::analyze::data::load_civs();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;
    println!(
        "recent ranked games — {} (profile {profile})",
        matches[0].my_alias.as_deref().unwrap_or("?")
    );
    println!("  {:<12} {:>8}  {:<20} {:<5} {:<14} {:>5}  {}",
        "match_id", "when", "map", "mode", "civ", "elo", "result");
    for m in matches.iter().take(limit.unwrap_or(usize::MAX)) {
        // 2 members = 1v1; 4/6/8 = 2v2/3v3/4v4. source: stream-relic.mjs keepBySize.
        let mode = if m.team_size == 2 { "1v1".to_string() } else { format!("{0}v{0}", m.team_size / 2) };
        let civ = m.my_civ_id.and_then(|id| civs.get(&id).cloned()).unwrap_or_else(|| "?".into());
        let result = match m.my_won { Some(true) => "win", Some(false) => "loss", None => "?" };
        println!("  {:<12} {:>8}  {:<20} {:<5} {:<14} {:>5}  {}",
            m.match_id, ago(now - m.completed_unix), m.map_raw.as_deref().unwrap_or("?"),
            mode, civ, m.my_rating.map(|r| r.to_string()).unwrap_or_else(|| "-".into()), result);
    }
    println!("\nanalyze one:  replay-rs analyze --match-id <id> --profile-id {profile}");
    Ok(())
}

fn print_usage() {
    eprintln!(
        "replay-rs — fast in-process AoE2 replay pipeline\n\
         \n\
         USAGE:\n  \
           replay-rs seed <ids.csv|ids.txt> [--db <manifest.sqlite>]\n  \
           replay-rs run [--db <manifest.sqlite>] [--out <dir>] [--threads N] [--limit M]\n  \
           replay-rs bench <dir of .aoe2record> [--threads N] [--repeat N]\n  \
           replay-rs analyze <file>|--match-id N|--latest [N|all] [--you NAME] [--profile-id P] [--json]\n  \
           replay-rs recent --profile-id P [--limit N]\n\
         \n\
         DEFAULTS: --db {DEFAULT_DB}  --out {DEFAULT_OUT}  --threads {DEFAULT_THREADS}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ago_formats_minutes_hours_days() {
        assert_eq!(ago(90), "1m ago");
        assert_eq!(ago(2 * 3600 + 100), "2h ago");
        assert_eq!(ago(3 * 86_400 + 5), "3d ago");
        assert_eq!(ago(-5), "0m ago"); // clock skew never goes negative
    }

    #[test]
    fn latest_value_parses_default_number_and_all() {
        assert!(matches!(parse_latest(None), Ok(Latest::N(1))));
        assert!(matches!(parse_latest(Some("3")), Ok(Latest::N(3))));
        assert!(matches!(parse_latest(Some("all")), Ok(Latest::All)));
        assert!(parse_latest(Some("banana")).is_err());
    }
}
