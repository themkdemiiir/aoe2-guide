//! Orchestrator: pending match_ids -> getReplayFiles -> download -> parse -> store.
//!
//! Port of `scripts/data-pipeline/replays/pipeline.py`, with rayon in place of the
//! Python thread pool and in-process aoe2rec parsing instead of a subprocess.
//!
//! Design (clean, fully resumable via the manifest):
//!   - The main thread batches pending ids and calls getReplayFiles (rate-limited),
//!     immediately resolving dead ends (expired / no_replay) in the manifest.
//!   - Downloadable matches are processed by a rayon parallel iterator that
//!     downloads + gunzips + parses each replay concurrently across cores, and
//!     appends rows to the shared NDJSON store (each table is mutex-guarded).
//!   - Manifest status updates are collected and written in batches by the main
//!     thread, so every match still ends in exactly one terminal status.

use anyhow::Result;
use rayon::prelude::*;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::api::{self, RateLimiter};
use crate::extract;
use crate::manifest::{fmt_counts, Manifest, Update};
use crate::store::Store;
use replay_rs::config;

/// A match that resolved to a downloadable signed URL.
struct Downloadable {
    match_id: i64,
    url: String,
    n_files: i64,
    best_size: i64,
    best_pid: Option<i64>,
}

/// A match that aged out of getReplayFiles but has participant profile ids, so
/// the age-archive fallback (api.ageofempires.com) can still try it.
struct FallbackCandidate {
    match_id: i64,
    profile_ids: Vec<i64>,
}

/// Outcome of one download+parse, ready to fold into a manifest Update.
struct Outcome {
    match_id: i64,
    status: &'static str,
    error: Option<String>,
    events: Option<i64>,
    n_files: i64,
    best_size: i64,
    best_pid: Option<i64>,
}

impl Outcome {
    /// Seed an outcome for a match with the carried-over file metadata.
    fn for_match(d: &Downloadable, status: &'static str) -> Self {
        Outcome {
            match_id: d.match_id,
            status,
            error: None,
            events: None,
            n_files: d.n_files,
            best_size: d.best_size,
            best_pid: d.best_pid,
        }
    }

    fn with_error(mut self, msg: String) -> Self {
        self.error = Some(truncate(&msg, 200));
        self
    }

    fn with_events(mut self, n: i64) -> Self {
        self.events = Some(n);
        self
    }
}

pub struct RunConfig {
    pub db: String,
    pub out: String,
    pub threads: usize,
    pub limit: Option<usize>,
    /// How many pending ids to pull per outer loop iteration.
    pub batch_lookahead: usize,
    /// Cap on archive-fallback HTTP requests per run (each is ~30s of pacing;
    /// the cap keeps a cron-driven run inside its window).
    pub archive_limit: usize,
}

pub fn run(cfg: RunConfig) -> Result<()> {
    // Quiet panic hook: a corrupt replay panics deep inside binrw/aoe2rec, and
    // we catch+record it per-match (see `process_one`). Without this, every such
    // panic would dump a full backtrace to stderr and flood a 411k-match run.
    // The manifest still records the panic message in the `error` column.
    std::panic::set_hook(Box::new(|_info| {}));

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(cfg.threads)
        .build()?;

    let mut manifest = Manifest::open(&cfg.db)?;
    let store = Store::open(Path::new(&cfg.out))?;
    let client = api::build_client()?;
    let limiter = RateLimiter::new(config::REPLAYFILES_PER_MIN);
    // Archive fallback: one request per ~30s, shared budget across the run.
    let archive_limiter = RateLimiter::new((60.0 / config::ARCHIVE_SPACING_SECS) as u32);
    let mut archive_requests = 0usize;
    let mut archive_blocked = false; // set on a persistent 429 wall

    // Recover non-terminal rows from any prior partial run (downloadable/error
    // -> pending) so a crashed run resumes cleanly without stranding matches.
    let reset = manifest.reset_inflight()?;
    if reset > 0 {
        println!("reset {reset} in-flight (downloadable/error) -> pending");
    }

    let mut processed: usize = 0;
    let parsed_events = AtomicU64::new(0); // cumulative across the run
    let t0 = Instant::now();
    println!("start — manifest counts: {}", fmt_counts(&manifest.counts()?));

    loop {
        if let Some(limit) = cfg.limit {
            if processed >= limit {
                break;
            }
        }
        let take = match cfg.limit {
            Some(limit) => cfg.batch_lookahead.min(limit - processed),
            None => cfg.batch_lookahead,
        };
        let pending = manifest.take(config::PENDING, take)?;
        if pending.is_empty() {
            break;
        }

        // --- look up signed URLs in chunks of REPLAYFILES_BATCH (rate-limited) ---
        let ids: Vec<i64> = pending.iter().map(|r| r.match_id).collect();
        // Participant ids per match (archive-fallback key), parsed defensively:
        // the seeder validates the format, but the manifest is hand-editable.
        let pids_by_match: std::collections::HashMap<i64, Vec<i64>> = pending
            .iter()
            .filter_map(|r| {
                let pids: Vec<i64> = r
                    .profile_ids
                    .as_deref()?
                    .split(';')
                    .filter_map(|p| p.trim().parse().ok())
                    .collect();
                (!pids.is_empty()).then_some((r.match_id, pids))
            })
            .collect();
        let mut downloadables: Vec<Downloadable> = Vec::new();
        let mut fallbacks: Vec<FallbackCandidate> = Vec::new();
        let mut pre_updates: Vec<Update> = Vec::new();

        for chunk in ids.chunks(config::REPLAYFILES_BATCH) {
            limiter.wait();
            let per_match = match api::get_replay_files(&client, chunk) {
                Ok(m) => m,
                Err(e) => {
                    // Transient: leave pending for retry via the `error` status.
                    for m in chunk {
                        let mut u = Update::new(*m, config::ERROR);
                        u.error = Some(truncate(&format!("getReplayFiles: {e}"), 200));
                        pre_updates.push(u);
                    }
                    continue;
                }
            };
            for &m in chunk {
                match per_match.get(&m) {
                    None => {
                        // Absent from the response = aged out of the fast path.
                        // With participant ids we can still try the age archive;
                        // without them the match is done.
                        match pids_by_match.get(&m) {
                            Some(pids) => fallbacks.push(FallbackCandidate {
                                match_id: m,
                                profile_ids: pids.clone(),
                            }),
                            None => pre_updates.push(Update::new(m, config::EXPIRED)),
                        }
                    }
                    Some(files) => match api::best_file(files) {
                        None => {
                            let mut u = Update::new(m, config::NO_REPLAY);
                            u.n_files = Some(files.len() as i64);
                            u.best_size = Some(-1);
                            pre_updates.push(u);
                        }
                        Some(best) => downloadables.push(Downloadable {
                            match_id: m,
                            url: best.url.clone().unwrap(),
                            n_files: files.len() as i64,
                            best_size: best.size.unwrap_or(-1),
                            best_pid: best.profile_id,
                        }),
                    },
                }
            }
        }

        if !pre_updates.is_empty() {
            manifest.update_many(&pre_updates)?;
        }

        // --- download + parse in parallel across cores; append rows directly ---
        // Each task is independent: download -> gunzip -> parse -> store.add.
        // `store` and `client` are shared by reference (Send + Sync).
        let processed_in_batch = downloadables.len();
        let outcomes: Vec<Outcome> = pool.install(|| {
            downloadables
                .par_iter()
                .map(|d| process_one(d, &client, &store, &parsed_events))
                .collect()
        });

        // --- archive fallback: strictly serial, heavily paced ------------------
        // Every candidate leaves this block with a status: parsed, parse_failed,
        // expired (archive has no copy), or error (retryable) — otherwise the
        // outer loop would re-take the same pending rows forever.
        let mut fallback_outcomes: Vec<Outcome> = Vec::new();
        let n_fallbacks = fallbacks.len();
        for cand in fallbacks {
            let budget_left = cfg.archive_limit.saturating_sub(archive_requests) > 0;
            if archive_blocked || !budget_left {
                fallback_outcomes.push(Outcome {
                    match_id: cand.match_id,
                    status: config::ERROR,
                    error: Some(if archive_blocked {
                        "archive: rate limited (deferred to next run)".into()
                    } else {
                        "archive: request budget exhausted (deferred to next run)".into()
                    }),
                    events: None,
                    n_files: 0,
                    best_size: -1,
                    best_pid: None,
                });
                continue;
            }
            match api::download_archive_replay(
                &client,
                &archive_limiter,
                cand.match_id,
                &cand.profile_ids,
                &mut archive_requests,
            ) {
                Ok(api::ArchiveFetch::Ok(raw, pid)) => {
                    let d = Downloadable {
                        match_id: cand.match_id,
                        url: String::new(),
                        n_files: 1,
                        best_size: raw.len() as i64,
                        best_pid: Some(pid),
                    };
                    fallback_outcomes.push(parse_and_store(&d, raw, &store, &parsed_events));
                }
                Ok(api::ArchiveFetch::NotFound) => fallback_outcomes.push(Outcome {
                    match_id: cand.match_id,
                    status: config::EXPIRED,
                    error: Some("archive: 404 for all participants".into()),
                    events: None,
                    n_files: 0,
                    best_size: -1,
                    best_pid: None,
                }),
                // Terminal: a bad ZIP is deterministic, so parse_failed (not the
                // retryable error) — otherwise it re-burns budget every run.
                Ok(api::ArchiveFetch::BadPayload(msg)) => fallback_outcomes.push(Outcome {
                    match_id: cand.match_id,
                    status: config::PARSE_FAILED,
                    error: Some(truncate(&msg, 200)),
                    events: None,
                    n_files: 0,
                    best_size: -1,
                    best_pid: None,
                }),
                Ok(api::ArchiveFetch::RateLimited) => {
                    archive_blocked = true;
                    fallback_outcomes.push(Outcome {
                        match_id: cand.match_id,
                        status: config::ERROR,
                        error: Some("archive: rate limited (deferred to next run)".into()),
                        events: None,
                        n_files: 0,
                        best_size: -1,
                        best_pid: None,
                    });
                }
                Err(e) => fallback_outcomes.push(Outcome {
                    match_id: cand.match_id,
                    status: config::ERROR,
                    error: Some(truncate(&format!("archive: {e}"), 200)),
                    events: None,
                    n_files: 0,
                    best_size: -1,
                    best_pid: None,
                }),
            }
        }

        // --- fold outcomes into manifest updates (single writer) --------------
        let updates: Vec<Update> = outcomes
            .into_iter()
            .chain(fallback_outcomes)
            .map(|o| {
                let mut u = Update::new(o.match_id, o.status);
                u.n_files = Some(o.n_files);
                u.best_size = Some(o.best_size);
                u.best_pid = o.best_pid;
                u.events = o.events;
                u.error = o.error;
                u
            })
            .collect();
        manifest.update_many(&updates)?;
        store.flush()?;

        processed += processed_in_batch + n_fallbacks;
        let rate = processed as f64 / t0.elapsed().as_secs_f64().max(1e-9);
        println!(
            "  processed {processed} ({rate:.1}/s) — {}",
            fmt_counts(&manifest.counts()?)
        );
    }

    store.flush()?;
    let secs = t0.elapsed().as_secs_f64();
    let events = parsed_events.load(Ordering::Relaxed);
    println!(
        "done — {processed} matches in {secs:.1}s ({:.1}/min, {events} events) — final counts: {}",
        processed as f64 / secs.max(1e-9) * 60.0,
        fmt_counts(&manifest.counts()?)
    );
    Ok(())
}

/// Process one downloadable match: download -> gunzip -> parse -> store.
///
/// Panic isolation: aoe2rec parses a rare corrupt replay by `unwrap`-ing a bad
/// `binrw` field or allocating a `Vec` from a garbage count, which *panics* (e.g.
/// "capacity overflow") rather than returning `Err`. Without isolation, one bad
/// replay would unwind through rayon and abort the whole run. So the parse+store
/// step runs inside `catch_unwind`: a panic OR an `Err` both become a terminal
/// `parse_failed` status (with the message recorded in the manifest), and the run
/// continues. The download step stays *outside* the catch — a network failure is
/// transient and maps to the retryable `error` status instead.
fn process_one(
    d: &Downloadable,
    client: &reqwest::blocking::Client,
    store: &Store,
    parsed_events: &AtomicU64,
) -> Outcome {
    // 1. download (transient failures -> retryable `error`, not parse_failed)
    let raw = match api::download_replay(client, &d.url) {
        Ok(b) => b,
        Err(e) => {
            return Outcome::for_match(d, config::ERROR)
                .with_error(format!("download: {e}"));
        }
    };

    parse_and_store(d, raw, store, parsed_events)
}

/// Parse + extract + store raw replay bytes, isolated from panics. Shared by
/// the signed-URL path and the archive fallback. The closure only borrows
/// shared data (the mutex-guarded Store, the atomic counter) and owns `raw`;
/// a panic mid-parse cannot leave our own state inconsistent, so
/// AssertUnwindSafe is sound here.
fn parse_and_store(
    d: &Downloadable,
    raw: bytes::Bytes,
    store: &Store,
    parsed_events: &AtomicU64,
) -> Outcome {
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let data = extract::extract(d.match_id, raw)?;
        let n_ev = data.events.len() as i64;
        store.add(&data)?; // per-table mutex + buffered; never blocks long
        Ok::<i64, anyhow::Error>(n_ev)
    }));

    match result {
        Ok(Ok(n_ev)) => {
            parsed_events.fetch_add(n_ev as u64, Ordering::Relaxed);
            Outcome::for_match(d, config::PARSED).with_events(n_ev)
        }
        // aoe2rec / extract / store returned an Err — corrupt or unsupported replay.
        Ok(Err(e)) => {
            Outcome::for_match(d, config::PARSE_FAILED).with_error(format!("parse: {e}"))
        }
        // aoe2rec panicked (unwrap / capacity overflow) — same terminal status.
        Err(panic) => {
            let msg = panic_message(panic.as_ref());
            Outcome::for_match(d, config::PARSE_FAILED).with_error(format!("panic: {msg}"))
        }
    }
}

/// Best-effort extraction of a panic payload's message string.
fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

/// Byte-safe truncation for error strings stored in the manifest.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s[..end].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The parse+store step must turn a corrupt replay into a `parse_failed`
    /// outcome — whether aoe2rec returns Err OR panics (unwrap / capacity
    /// overflow) — and must never let the panic escape the closure.
    #[test]
    fn corrupt_replay_is_caught_not_propagated() {
        // Silence the expected panic backtraces during this test.
        std::panic::set_hook(Box::new(|_| {}));
        let store = Store::open(std::path::Path::new(
            &std::env::temp_dir().join("replay_rs_test_shards"),
        ))
        .unwrap();
        let counter = AtomicU64::new(0);
        let d = Downloadable {
            match_id: 1,
            url: String::new(),
            n_files: 1,
            best_size: 1,
            best_pid: None,
        };

        // Garbage bytes drive aoe2rec down its bad-length/unwrap paths. Run the
        // exact isolated step `process_one` uses; assert no unwind escapes.
        for bad in [
            bytes::Bytes::from_static(b""),
            bytes::Bytes::from_static(&[0xFFu8; 64]),
            bytes::Bytes::from(vec![0u8; 4096]),
        ] {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let data = extract::extract(d.match_id, bad.clone())?;
                store.add(&data)?;
                Ok::<(), anyhow::Error>(())
            }));
            // Either Err (graceful) or Err-of-panic (caught) — never an escape,
            // and never Ok (these bytes are not a valid replay).
            assert!(matches!(result, Ok(Err(_)) | Err(_)));
        }
        let _ = counter; // counter only bumps on success; none here
    }
}
