//! [`reparse_dir`] — reads the raw `.aoe2record.zst` archive [`crate::raw::RawArchive`] writes
//! back off disk and re-runs `replay::parse` against every entry, proving the archived bytes are
//! faithfully re-usable by a future (possibly improved) parser. This is the whole point of
//! keeping the raw corpus in the first place — see `crate::raw`'s module doc.
//!
//! ## Scope: parse-and-count only, NOT full re-ingest
//! This deliberately stops at "decompress + `replay::parse` + count outcomes." It does NOT
//! re-compose a [`ingest::ReplayBatch`] or write to Postgres, and it never reads `DATABASE_URL`.
//! Full re-ingest needs the same seed metadata (`played_at`/`ladder`) `crate::crawl`'s own module
//! doc already flags as a persistence gap: `fetch`'s manifest keeps `match_id`/`played_at`/
//! `profile_ids`, not `match_type`/`new_rating`, and the raw archive itself carries neither (it is
//! intentionally just the bytes `download_replay` returned). Re-deriving that metadata for an
//! already-archived match would mean either re-querying the Relic API (defeating the point of an
//! OFFLINE archive) or joining against the DB's `matches` row already recorded for that
//! `match_id` (which already has `ladder`/`played_at` from the original ingest) — both are real,
//! buildable follow-ups, but out of scope here: the brief's essential deliverable is KEEPING the
//! raw bytes, and this entry point's job is to prove that keeping them is actually useful, not to
//! build the re-ingest pipeline itself.

use std::path::Path;

use bytes::Bytes;

use crate::raw::RawArchive;

/// [`reparse_dir`]'s outcome counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReparseSummary {
    /// Archived replays found under `raw_dir` (after `limit`, if any, was applied).
    pub found: usize,
    /// Decompressed and `replay::parse`d successfully.
    pub parsed_ok: usize,
    /// Failed to open, decompress, or parse — logged individually (`tracing::warn!`), counted
    /// here rather than aborting the whole run (one bad archive entry must not stop re-parsing
    /// the rest, same resilience rule the crawl loop itself follows).
    pub parsed_err: usize,
    /// Total decompressed bytes read across every successfully-decompressed entry (parsed or
    /// not) — a rough throughput/corpus-size figure for the summary log.
    pub bytes_read: u64,
}

/// Walks every archived replay under `raw_dir` ([`RawArchive::iter_saved`]), decompresses it, and
/// `replay::parse`s it — see the module doc for exactly what this does and does not do. Entries
/// are visited in ascending `match_id` order for deterministic, resumable-by-eye output.
/// `limit` (when `Some`) stops after that many entries.
///
/// Synchronous and CPU-bound (zstd decompress + the parser) — intentionally not `async`: this is
/// a one-shot CLI utility, not a concurrent server, so there is nothing else for a tokio runtime
/// to interleave it with (see `main.rs`'s `reparse` command, which calls this directly from
/// inside `#[tokio::main]` without `spawn_blocking`).
pub fn reparse_dir(raw_dir: &Path, limit: Option<usize>) -> std::io::Result<ReparseSummary> {
    let archive = RawArchive::new(raw_dir.to_path_buf());
    let mut entries = archive.iter_saved()?;
    entries.sort_by_key(|(match_id, _)| match_id.0);
    if let Some(limit) = limit {
        entries.truncate(limit);
    }

    let mut summary = ReparseSummary {
        found: entries.len(),
        ..ReparseSummary::default()
    };

    for (match_id, path) in entries {
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(err) => {
                tracing::warn!(match_id = %match_id, error = %err, "reparse: failed to open archived replay");
                summary.parsed_err += 1;
                continue;
            }
        };
        let raw = match zstd::stream::decode_all(file) {
            Ok(raw) => raw,
            Err(err) => {
                tracing::warn!(match_id = %match_id, error = %err, "reparse: failed to decompress archived replay");
                summary.parsed_err += 1;
                continue;
            }
        };
        summary.bytes_read += raw.len() as u64;

        match replay::parse(match_id, Bytes::from(raw)) {
            Ok(_parsed) => {
                summary.parsed_ok += 1;
                tracing::debug!(match_id = %match_id, "reparse: parsed ok");
            }
            Err(err) => {
                summary.parsed_err += 1;
                tracing::warn!(match_id = %match_id, error = %err, "reparse: parse failed");
            }
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use pipeline_core::MatchId;

    use super::*;

    #[test]
    fn reparse_dir_on_an_empty_archive_reports_zero_found() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let summary = reparse_dir(dir.path(), None).expect("must not error on an empty archive");
        assert_eq!(summary, ReparseSummary::default());
    }

    #[test]
    fn reparse_dir_counts_a_non_replay_archived_blob_as_a_parse_failure_not_a_crash() {
        // The raw archive stores WHATEVER bytes were downloaded — `reparse` must not assume they
        // are a valid `.aoe2record` (that is exactly the "prove failures aren't lost" case this
        // whole feature exists for). Garbage bytes must be counted as a parse failure, never
        // panic the walk.
        let dir = tempfile::tempdir().expect("create temp dir");
        let archive = RawArchive::new(dir.path());
        archive
            .save(MatchId(1), &Bytes::from_static(b"not a real aoe2record"))
            .expect("save must succeed regardless of content");

        let summary = reparse_dir(dir.path(), None).expect("must not error");
        assert_eq!(summary.found, 1);
        assert_eq!(summary.parsed_ok, 0);
        assert_eq!(summary.parsed_err, 1);
        assert!(
            summary.bytes_read > 0,
            "the bytes were still decompressed and counted"
        );
    }

    #[test]
    fn reparse_dir_respects_the_limit() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let archive = RawArchive::new(dir.path());
        for id in [1, 2, 3] {
            archive
                .save(MatchId(id), &Bytes::from_static(b"garbage but present"))
                .unwrap();
        }

        let summary = reparse_dir(dir.path(), Some(2)).expect("must not error");
        assert_eq!(summary.found, 2, "limit caps how many entries are visited");
    }
}
