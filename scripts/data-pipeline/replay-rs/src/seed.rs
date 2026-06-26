//! Seed the manifest with match_ids to process.
//!
//! Accepts a newline-delimited list or a CSV. The first column is the match_id;
//! optional `ladder` and `played_at` columns (by header name, case-insensitive)
//! are picked up when present. Lines that are not parseable as an integer
//! match_id are skipped (so a header row is tolerated).
//!
//! (Parquet input is intentionally not supported here — keeping the dependency
//! list light, per the brief. Feed parquet through DuckDB to a CSV/txt first.)

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::manifest::{Manifest, SeedRow};

/// Parse a seed file into rows. Auto-detects CSV (has a comma) vs newline list.
pub fn parse_seed_file(path: &Path) -> Result<Vec<SeedRow>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading seed file {}", path.display()))?;
    let is_csv = text.lines().next().map(|l| l.contains(',')).unwrap_or(false);
    if is_csv {
        parse_csv(&text)
    } else {
        Ok(parse_lines(&text))
    }
}

/// Newline list: one match_id per line. Blank lines and non-numeric lines skip.
fn parse_lines(text: &str) -> Vec<SeedRow> {
    text.lines()
        .filter_map(|l| l.trim().parse::<i64>().ok())
        .map(|match_id| SeedRow {
            match_id,
            ladder: None,
            played_at: None,
        })
        .collect()
}

/// Minimal CSV: header row maps column names; first numeric column is match_id.
/// Recognised optional headers: `match_id`/`matchid`, `ladder`, `played_at`.
fn parse_csv(text: &str) -> Result<Vec<SeedRow>> {
    let mut lines = text.lines();
    let header = lines.next().unwrap_or("");
    let cols: Vec<String> = header.split(',').map(|c| c.trim().to_lowercase()).collect();
    let find = |names: &[&str]| cols.iter().position(|c| names.contains(&c.as_str()));
    let id_idx = find(&["match_id", "matchid", "id"]).unwrap_or(0);
    let ladder_idx = find(&["ladder"]);
    let played_idx = find(&["played_at", "playedat", "ts"]);

    let mut rows = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        let match_id = match fields.get(id_idx).and_then(|f| f.parse::<i64>().ok()) {
            Some(v) => v,
            None => continue, // tolerate a stray header / blank
        };
        rows.push(SeedRow {
            match_id,
            ladder: ladder_idx
                .and_then(|i| fields.get(i))
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
            played_at: played_idx
                .and_then(|i| fields.get(i))
                .and_then(|s| s.parse::<i64>().ok()),
        });
    }
    Ok(rows)
}

/// Seed the manifest at `db` from `path`. Returns (parsed_rows, newly_inserted).
pub fn seed(db: &str, path: &Path) -> Result<(usize, usize)> {
    let rows = parse_seed_file(path)?;
    let mut manifest = Manifest::open(db)?;
    let inserted = manifest.seed(&rows)?;
    Ok((rows.len(), inserted))
}
