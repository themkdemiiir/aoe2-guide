//! Parity-gate harness (Task 4b): parse a `.aoe2record` file and print the resulting
//! `ParsedReplay` as JSON.
//!
//! Not part of the crate's public API — a throwaway CLI used to diff this crate's `parse()`
//! against the OLD `replay-rs` extractor's output on a real fixture (see
//! `.superpowers/sdd/task-4b-replay-report.md`). `players`/`ages` are sorted before printing
//! because `parse()` builds them from a `HashMap` (iteration order is not stable across runs);
//! `events` are already in the replay's own deterministic operation-stream order.
//!
//! Usage: `cargo run --release --example parse_json -p replay -- <file.aoe2record> <match_id>`

use std::path::PathBuf;

use bytes::Bytes;
use pipeline_core::MatchId;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("usage: parse_json <file.aoe2record> <match_id>"))?,
    );
    let match_id: i64 = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: parse_json <file.aoe2record> <match_id>"))?
        .parse()?;

    let bytes = Bytes::from(std::fs::read(&path)?);
    let mut parsed = replay::parse(MatchId(match_id), bytes)
        .map_err(|e| anyhow::anyhow!("parse {}: {e}", path.display()))?;

    parsed.players.sort_by_key(|p| p.player_number);
    parsed
        .ages
        .sort_by_key(|a| (a.profile_id.0, a.age.as_db_str()));

    println!("{}", serde_json::to_string_pretty(&parsed)?);
    Ok(())
}
