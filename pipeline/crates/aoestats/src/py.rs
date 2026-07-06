//! Runs `pipeline/py/aoestats_summaries.py` — the ONE Python step this pipeline allows (see the
//! crate's module doc and the task brief) — as a `python3 -c <embedded source>` subprocess:
//! NDJSON in, NDJSON out. The script's source is embedded at compile time via `include_str!`
//! rather than resolved as a runtime file path, so the built binary is self-contained (works from
//! any working directory, doesn't depend on the source tree's layout at run time) and can never
//! drift from what `pipeline/py/aoestats_summaries.py` actually contains — edit that one file and
//! rebuild, there is no second copy to keep in sync.
//!
//! Fails loud with [`AoestatsError::PythonNotFound`] if `python3` isn't on `PATH` — the crate's
//! one required external tool (see `db`'s module doc and the crate's README).

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::error::{AoestatsError, Result};

/// The embedded source of `pipeline/py/aoestats_summaries.py` (3 levels up from this file's
/// directory: `crates/aoestats/src/` -> `crates/aoestats/` -> `crates/` -> `pipeline/`).
const SUMMARIES_SCRIPT: &str = include_str!("../../../py/aoestats_summaries.py");

/// One row fed to the python subprocess's stdin: the exact projection the task brief specifies
/// off `p_*.parquet` (`replay_summary_raw IS NOT NULL AND length(replay_summary_raw) > 50`,
/// applied in Rust by [`crate::parquet_read::read_player_age_sources`] — see its doc for why the
/// filter runs there instead of as a SQL `WHERE`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgeSourceRow {
    pub game_id: Option<String>,
    pub profile_id: Option<f64>,
    pub civ: Option<String>,
    pub winner: Option<bool>,
    pub replay_summary_raw: String,
}

/// One per-player-per-age row the script emits on stdout — exactly the fields `match_ages` has
/// columns for (see `pipeline/py/aoestats_summaries.py`'s module doc for the fields the original
/// `scripts/data-pipeline/extract-replay-summaries.py` also emits that this port drops). `age` is
/// validated against [`pipeline_core::Age`] by the caller ([`crate::db::import_ages`]), not here —
/// this struct is the raw wire shape.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AgeOutputRow {
    pub game_id: Option<String>,
    pub profile_id: Option<f64>,
    pub civ: Option<String>,
    pub won: Option<bool>,
    pub age: String,
    /// Seconds to reach this age (the caller's SQL does the `* 1000` -> `uptime_ms` conversion —
    /// see `db`'s module doc). `None` only if a future python change ever omits the key; every row
    /// in the real archive supplies a number, including a `-1` sentinel some rows carry for an age
    /// that was reached in `age_stats` but not actually researched (`age_researched: false`) —
    /// ported through as-is, not special-cased, per the "never fabricate" rule.
    pub uptime: Option<f64>,
    pub villagers: Option<i32>,
    pub military: Option<i32>,
    pub n_buildings: Option<i32>,
    pub n_research: Option<i32>,
}

/// Feeds `rows` to the embedded `aoestats_summaries.py` script as NDJSON on stdin and returns its
/// NDJSON stdout, parsed one [`AgeOutputRow`] per line. Synchronous/blocking by design — the async
/// caller runs this via `tokio::task::spawn_blocking` (see `db::import_ages`) rather than this
/// module taking on a `tokio::process` dependency for what is, per run, a single one-shot
/// subprocess call.
pub fn run_summaries(rows: Vec<AgeSourceRow>) -> Result<Vec<AgeOutputRow>> {
    let mut child = Command::new("python3")
        .arg("-c")
        .arg(SUMMARIES_SCRIPT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                AoestatsError::PythonNotFound
            } else {
                AoestatsError::PythonIo(source)
            }
        })?;

    let mut stdin = child.stdin.take().ok_or(AoestatsError::PythonPipeSetup)?;
    let stdout = child.stdout.take().ok_or(AoestatsError::PythonPipeSetup)?;
    let mut stderr = child.stderr.take().ok_or(AoestatsError::PythonPipeSetup)?;

    // Written on a separate thread rather than "write everything, then read stdout": with a
    // large batch, the OS pipe buffers for stdin and stdout can each fill while the other side of
    // this process is blocked on the other pipe, deadlocking both sides. Writing and reading
    // concurrently avoids that regardless of batch size.
    let writer = std::thread::spawn(move || -> Result<()> {
        for row in &rows {
            let line = serde_json::to_string(row).map_err(|source| {
                AoestatsError::PythonInputSerialize {
                    game_id: row.game_id.clone(),
                    source,
                }
            })?;
            stdin
                .write_all(line.as_bytes())
                .and_then(|()| stdin.write_all(b"\n"))
                .map_err(AoestatsError::PythonIo)?;
        }
        Ok(())
        // `stdin` drops here, closing the pipe so the script sees EOF on stdin.
    });

    let mut output = Vec::new();
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(AoestatsError::PythonIo)?;
        if line.trim().is_empty() {
            continue;
        }
        let row: AgeOutputRow = serde_json::from_str(&line).map_err(|source| {
            AoestatsError::PythonOutputParse {
                line: line.clone(),
                source,
            }
        })?;
        output.push(row);
    }

    writer
        .join()
        .map_err(|_panic_payload| AoestatsError::PythonStdinThreadPanicked)??;

    let status = child.wait().map_err(AoestatsError::PythonIo)?;
    let mut stderr_text = String::new();
    let _ = stderr.read_to_string(&mut stderr_text);
    if !stderr_text.trim().is_empty() {
        tracing::info!(
            stderr = %stderr_text.trim(),
            "aoestats_summaries.py diagnostic output"
        );
    }
    if !status.success() {
        return Err(AoestatsError::PythonExited {
            status,
            stderr: stderr_text,
        });
    }

    Ok(output)
}
