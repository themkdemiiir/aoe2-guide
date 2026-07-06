//! [`crawl`] — the M6 run loop: discover -> download -> parse -> [`crate::to_batch`] -> ingest,
//! driven off `fetch`'s resumable [`fetch::ReplayManifest`]. This is the one place
//! [`crate::source::ReplaySource`], [`fetch::ReplayManifest`], and [`crate::sink::IngestSink`]
//! compose into an executable crawl (the pure per-batch join, [`crate::to_batch`], already
//! existed — this module is what actually DRIVES it, per the crate's top-level doc).
//!
//! ## Shape
//! 1. **Discover** (network-free when [`CrawlConfig::profile_id`] is `None`): recent ranked
//!    matches for one profile, seeded into the manifest (dedup on `match_id` — see
//!    [`fetch::ReplayManifest::seed`]).
//! 2. **Plan**: `take_ready(limit, now)` — every `pending` row plus every `error` row whose
//!    backoff window has elapsed, newest first, capped at [`CrawlConfig::limit`].
//! 3. **Process** (skipped entirely in [`CrawlConfig::dry_run`] mode): each planned match is
//!    fetched + parsed by a worker task, bounded by an `Arc<Semaphore>` sized
//!    [`CrawlConfig::concurrency`]. A worker's outcome is drained back onto THIS task, which is
//!    the single owner of both the [`crate::sink::IngestSink`] (one `&mut Client`, one
//!    `BEGIN...COMMIT` per batch) and the manifest (rusqlite is sync, called via
//!    `spawn_blocking` — see [`manifest_call`]) — never contended from two tasks at once.
//! 4. **Record**: every outcome becomes a [`fetch::StatusUpdate`], applied in one manifest
//!    transaction at the end of the drain.
//!
//! ## Resilience (the brief's central requirement)
//! A single bad match — a download failure, an unparsable replay, an `ingest` error — is turned
//! into a `StatusUpdate` and the loop continues; NONE of those propagate as an `Err` out of
//! [`crawl`]. The only conditions that abort the whole run are the resumable manifest's own
//! storage failing ([`CrawlError::Manifest`]) — matching `fetch::Error::Manifest`'s own documented
//! policy ("the local DB is broken, stop the run") — or a caller precondition violated
//! ([`CrawlError::MissingSink`]).
//!
//! ## Two independent bounds, not one (playbook: "separate constraints")
//! [`CrawlConfig::concurrency`] bounds how many per-match worker tasks (fetch -> parse -> compose)
//! run at once. The steady request RATE is a SEPARATE constraint, already owned by whichever
//! [`crate::source::ReplaySource`] is in use — [`crate::source::FetchSource`] delegates it to
//! `fetch::FetchClient`'s own governor (GCRA) + semaphore, per `fetch`'s own module doc. This loop
//! does not duplicate a second rate limiter: reusing the adapter's existing, already-tested one is
//! the LIBRARY-FIRST choice.
//!
//! ## Seed persistence gap (a known, documented scope limit)
//! `fetch`'s manifest schema persists `match_id`/`played_at`/`profile_ids` — NOT
//! `DiscoverySeed::match_type`/`new_rating`. [`crate::to_batch`] needs the FULL seed (for
//! `ladder_for`), which only exists for matches discovered in THIS process's `discover` call. A
//! `take_ready` row surviving from an EARLIER run (a retry) whose match no longer appears in this
//! run's fresh discovery therefore cannot be composed — rather than guess a ladder (the exact
//! "no defaults, fail loud" rule that produced the civ-id-map bug fix), it is left untouched and
//! counted in [`CrawlSummary::skipped_no_seed`], to be retried once rediscovered. In practice
//! `discover_recent`'s "recent" window comfortably covers any reasonable crawl cadence's retry
//! backoff.
//!
//! ## Graceful shutdown
//! One root [`CancellationToken`] (owned by the caller — `main.rs` cancels it on Ctrl-C). Checked
//! before starting each new worker (stop taking new work); in-flight workers are always let
//! finish and drained; an in-flight `ingest` call is raced against cancellation specifically (a
//! mid-transaction cancel drops the transaction, rolling it back — see [`crate::sink::IngestSink`]).
//!
//! ## Raw-replay archiving
//! [`process_one`] persists the raw downloaded bytes ([`crate::raw::RawArchive::save`]) for every
//! match that reaches [`ReplayFetch::Parsed`] or [`ReplayFetch::ParseFailed`] — i.e. every
//! successful DOWNLOAD, including ones `replay::parse` then rejects, which is the entire point
//! (`crate::raw`'s module doc). This runs inside the SAME spawned worker task as fetch+parse
//! (still off the serial drain, so it does not slow the loop's per-batch bookkeeping), gated on
//! [`CrawlConfig::raw_dir`] being `Some` (never for a dry-run — see that field's doc).
//!
//! **Raw-save failures are logged and counted, never fatal to the match.** A disk hiccup (or a
//! failed write-then-verify — see `crate::raw`'s "Integrity" section) must not throw away a
//! successful parse+ingest over an archive-side problem, so [`process_one`] logs
//! (`tracing::warn!`) and moves on; the match's [`MatchOutcome`]/manifest status is driven ENTIRELY
//! by fetch/parse/ingest, never by whether the raw archive succeeded. One documented consequence:
//! if a match's ingest succeeds (manifest -> `Parsed`, terminal) but its raw-save failed, that
//! match will not naturally be retried by this run loop (a `Parsed` row is never offered by
//! `take_ready` again) — wiring raw-save failures into the manifest's own retry vocabulary would
//! need a `fetch::ReplayManifest` schema/trait change, out of this feature's scope (see the task
//! brief's "you touch only `pipeline/crates/pipeline/**`" boundary). In practice a raw-save
//! failure is a local disk problem (full disk, permissions), not a per-match condition, so it is
//! expected to either resolve before the next match or surface loudly in logs long before it
//! matters at the corpus level.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use fetch::{DiscoverySeed, MatchStatus, ReplayManifest, SeedRow, StatusUpdate};
use pipeline_core::{MatchId, ProfileId};
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::raw::{RawArchive, SaveOutcome};
use crate::sink::IngestSink;
use crate::source::{ReplayFetch, ReplaySource};

/// [`crawl`]'s closed, fatal failure surface — see the module doc's "Resilience" section for why
/// this is deliberately small (almost everything else is recovered from, not propagated).
#[derive(Debug, Error)]
pub enum CrawlError {
    /// The resumable manifest's own storage failed (its own doc: "the local DB is broken, stop
    /// the run") — distinct from one match's fetch/parse/ingest failure, which the loop instead
    /// records as a [`StatusUpdate`] and continues past.
    #[error("resumable manifest operation failed")]
    Manifest(#[from] fetch::Error),

    /// The blocking manifest task didn't return normally (panicked, or the runtime is shutting
    /// down). There is no way to hand a manifest we no longer own back to the caller, so this is
    /// unconditionally fatal.
    #[error("manifest task did not complete: {0}")]
    ManifestTask(String),

    /// `crawl` was called for a live (non-dry-run) run with no [`crate::sink::IngestSink`] — a
    /// caller precondition, surfaced loudly rather than silently skipping every ingest.
    #[error("a live (non-dry-run) crawl requires an IngestSink")]
    MissingSink,
}

/// [`crawl`]'s result alias.
pub type Result<T> = std::result::Result<T, CrawlError>;

/// One `crawl` invocation's tunables. See the module doc for how [`Self::profile_id`] and
/// [`Self::dry_run`] interact (both independently control whether ANY network call happens).
#[derive(Debug, Clone)]
pub struct CrawlConfig {
    /// Discover this profile's recent matches. `None` skips discovery entirely (no network call)
    /// — useful for draining an already-seeded manifest, and for a `--dry-run` invocation that
    /// must never touch the live Relic API at all (see `main.rs`'s CLI doc).
    pub profile_id: Option<ProfileId>,
    /// Cap on how many manifest-eligible matches this run attempts (`take_ready`'s own `LIMIT`).
    pub limit: usize,
    /// Max concurrent in-flight per-match worker tasks. See the module doc's "Two independent
    /// bounds" section for why this is NOT the same knob as the request rate.
    pub concurrency: usize,
    /// Discover + plan only: never download, parse, or ingest. A fresh discover's `seed` call
    /// still runs (enqueuing is harmless local bookkeeping, not a live effect on Postgres).
    pub dry_run: bool,
    /// Root directory for the raw `.aoe2record.zst` archive ([`crate::raw::RawArchive`]) — `None`
    /// disables raw archiving entirely (no disk writes; the CLI's `--no-raw`). A live crawl with
    /// `Some(dir)` archives every successfully-downloaded replay, including ones that then fail
    /// to parse — see the module doc's "Raw-replay archiving" section. Irrelevant when
    /// [`Self::dry_run`] is set: the "Process" step (the only place a save could happen) never
    /// runs in a dry-run, so raw archiving is structurally a no-op regardless of this field —
    /// `main.rs` also sets this to `None` for a dry-run explicitly, as defense in depth.
    pub raw_dir: Option<PathBuf>,
}

/// One `crawl` invocation's outcome counts, for logging/monitoring.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrawlSummary {
    /// Newly-inserted manifest rows from this run's discovery (0 when `profile_id` is `None`).
    pub seeded: usize,
    /// Matches `take_ready` returned as eligible to attempt.
    pub planned: usize,
    /// Planned matches skipped for lacking a fresh in-run seed (see the module doc's "Seed
    /// persistence gap").
    pub skipped_no_seed: usize,
    /// Planned matches never started because shutdown was requested first.
    pub cancelled_before_start: usize,
    /// Matches actually handed to a worker task (fetch + parse attempted).
    pub attempted: usize,
    /// Matches that fetched, parsed, composed, AND ingested successfully.
    pub succeeded: usize,
    /// Matches that failed at any stage (fetch, parse, compose, or ingest) — each is recorded in
    /// the manifest for a future retry or as terminal, per [`MatchStatus`].
    pub failed: usize,
    /// Raw replays freshly written + read-back-verified to the archive this run (see the module
    /// doc's "Raw-replay archiving" section). 0 whenever [`CrawlConfig::raw_dir`] is `None`.
    pub raw_saved: usize,
    /// Raw replays that were already present in the archive (a resumed crawl re-attempting a
    /// match) and so were left untouched rather than re-written.
    pub raw_already_present: usize,
    /// Raw-archive writes that failed (disk error, or a write-then-verify integrity mismatch —
    /// see `crate::raw`'s "Integrity" section) — logged individually, never fatal to the match.
    pub raw_failed: usize,
    /// Total COMPRESSED bytes freshly written to the raw archive this run — the "periodic bytes
    /// tally" the raw-archive brief asks for; one crawl invocation is this loop's natural
    /// "period" (see the module doc's "Raw-replay archiving" section).
    pub raw_bytes_written: u64,
}

fn now_unix() -> i64 {
    // Saturates to 0 on a pre-1970 clock rather than panicking — mirrors
    // `fetch::manifest::SqliteManifest::now`'s own fallback idiom.
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Runs one synchronous [`ReplayManifest`] operation on a blocking thread (rusqlite is sync — see
/// the crate playbook's "spawn_blocking for the SQLite manifest writes" rule), threading
/// ownership of `manifest` through so no `Arc<Mutex<_>>` is needed: this crawl loop is the
/// manifest's only owner, so there is never contention to justify one.
async fn manifest_call<M, F, R>(manifest: M, f: F) -> Result<(M, R)>
where
    M: ReplayManifest + Send + 'static,
    F: FnOnce(&mut M) -> fetch::Result<R> + Send + 'static,
    R: Send + 'static,
{
    let (manifest, outcome) = tokio::task::spawn_blocking(move || {
        let mut manifest = manifest;
        let outcome = f(&mut manifest);
        (manifest, outcome)
    })
    .await
    .map_err(|join_err| CrawlError::ManifestTask(join_err.to_string()))?;

    match outcome {
        Ok(value) => Ok((manifest, value)),
        Err(err) => Err(CrawlError::Manifest(err)),
    }
}

/// One match's terminal outcome from the fetch+parse worker stage, before the driver turns it
/// into a [`StatusUpdate`] (and, on success, an ingest call). Never surfaced as an `Err` out of
/// [`crawl`] — every variant is recovered from and counted, per the module doc's resilience rule.
enum MatchOutcome {
    Ready(ingest::ReplayBatch),
    NoReplay,
    ParseFailed(String),
    Retryable {
        message: String,
        retry_after: Option<u64>,
    },
}

/// One match's raw-archive outcome from [`process_one`], folded into [`CrawlSummary`] by the
/// serial drain loop in [`crawl`]. See the module doc's "Raw-replay archiving" section.
enum RawSaveOutcome {
    /// No [`CrawlConfig::raw_dir`] configured, or nothing was downloaded for this match
    /// ([`ReplayFetch::NoReplay`] / [`ReplayFetch::FetchFailed`]) — there is nothing to archive.
    NotAttempted,
    Saved {
        bytes_written: u64,
    },
    AlreadyPresent,
    /// A disk error or a write-then-verify integrity failure (`crate::raw`'s "Integrity" section)
    /// — logged by [`process_one`] as it happens; never fatal to the match.
    Failed,
}

/// Runs [`RawArchive::save`] (synchronous, CPU+IO-bound) on a blocking thread — playbook rule:
/// "spawn_blocking for ... CPU-bound replay parse" applies just as much to zstd -19 compression +
/// its read-back verification decompress.
async fn save_raw(archive: Arc<RawArchive>, match_id: MatchId, raw: Bytes) -> RawSaveOutcome {
    let result = tokio::task::spawn_blocking(move || archive.save(match_id, &raw)).await;
    match result {
        Ok(Ok(SaveOutcome::Saved { bytes_written, .. })) => {
            tracing::debug!(match_id = %match_id, bytes_written, "raw replay archived");
            RawSaveOutcome::Saved { bytes_written }
        }
        Ok(Ok(SaveOutcome::AlreadyPresent)) => {
            tracing::debug!(match_id = %match_id, "raw replay already archived — skipped");
            RawSaveOutcome::AlreadyPresent
        }
        Ok(Err(err)) => {
            tracing::warn!(
                match_id = %match_id,
                error = %err,
                "failed to save raw replay archive — parse/ingest unaffected, continuing"
            );
            RawSaveOutcome::Failed
        }
        Err(join_err) => {
            tracing::warn!(
                match_id = %match_id,
                error = %join_err,
                "raw-archive save task did not complete — parse/ingest unaffected, continuing"
            );
            RawSaveOutcome::Failed
        }
    }
}

/// Fetch + parse + compose one match, archiving the raw downloaded bytes along the way (see the
/// module doc's "Raw-replay archiving" section). `#[tracing::instrument]`ed with the `match_id`
/// per the brief's "instrument the per-match work" rule.
#[tracing::instrument(skip(source, seed, raw_archive), fields(match_id = %match_id))]
async fn process_one<S: ReplaySource>(
    source: &S,
    match_id: MatchId,
    seed: DiscoverySeed,
    raw_archive: Option<Arc<RawArchive>>,
) -> (MatchOutcome, RawSaveOutcome) {
    let fetch_outcome = source.fetch_replay(match_id).await;

    // Archive the raw bytes for the two "a download actually happened" outcomes, regardless of
    // whether the parse embedded in `fetch_outcome` succeeded — a parse failure must never lose
    // the replay (`crate::raw`'s whole reason for existing).
    let raw_bytes = match &fetch_outcome {
        ReplayFetch::Parsed { raw, .. } | ReplayFetch::ParseFailed { raw, .. } => Some(raw.clone()),
        ReplayFetch::NoReplay | ReplayFetch::FetchFailed { .. } => None,
    };
    let raw_outcome = match (raw_bytes, raw_archive) {
        (Some(raw), Some(archive)) => save_raw(archive, match_id, raw).await,
        _ => RawSaveOutcome::NotAttempted,
    };

    let match_outcome = match fetch_outcome {
        ReplayFetch::Parsed { parsed, .. } => match crate::to_batch(parsed, seed) {
            Ok(batch) => MatchOutcome::Ready(batch),
            // `to_batch`'s failures (mismatched id, missing map, unmapped ladder) are exactly as
            // deterministic as a parse failure — folded into the same terminal bucket rather than
            // burning retry budget on an input that will fail identically every time.
            Err(err) => MatchOutcome::ParseFailed(err.to_string()),
        },
        ReplayFetch::NoReplay => MatchOutcome::NoReplay,
        ReplayFetch::ParseFailed { message, .. } => MatchOutcome::ParseFailed(message),
        ReplayFetch::FetchFailed {
            message,
            retry_after,
        } => MatchOutcome::Retryable {
            message,
            retry_after,
        },
    };

    (match_outcome, raw_outcome)
}

/// Run one discover -> plan -> process -> record crawl. See the module doc for the full shape.
/// Returns the manifest back (it is moved in and threaded through every blocking call, never
/// shared) alongside a [`CrawlSummary`] of what happened.
#[tracing::instrument(skip(source, manifest, sink, cancel))]
pub async fn crawl<S, M, I>(
    source: Arc<S>,
    manifest: M,
    mut sink: Option<&mut I>,
    config: &CrawlConfig,
    cancel: &CancellationToken,
) -> Result<(M, CrawlSummary)>
where
    S: ReplaySource + 'static,
    M: ReplayManifest + Send + 'static,
    I: IngestSink,
{
    if !config.dry_run && sink.is_none() {
        return Err(CrawlError::MissingSink);
    }

    let mut summary = CrawlSummary::default();

    // 1. Discover. See the module doc + `CrawlConfig::profile_id`'s doc for when this is
    // network-free. A discovery failure is logged and treated as "no new seeds this run" rather
    // than aborting the whole crawl.
    let seeds = match config.profile_id {
        None => Vec::new(),
        Some(profile_id) => match source.discover(profile_id).await {
            Ok(seeds) => seeds,
            Err(err) => {
                tracing::warn!(error = %err, "discover_recent failed — no new seeds this run");
                Vec::new()
            }
        },
    };

    let seed_rows: Vec<SeedRow> = seeds
        .iter()
        .map(|s| SeedRow {
            match_id: s.match_id,
            played_at: Some(s.played_at.timestamp()),
            profile_ids: None,
        })
        .collect();

    let (manifest, seeded) = if seed_rows.is_empty() {
        (manifest, 0)
    } else {
        manifest_call(manifest, move |m| m.seed(&seed_rows)).await?
    };
    summary.seeded = seeded;

    // See the module doc's "Seed persistence gap": only matches discovered THIS run can be
    // composed (need the full `DiscoverySeed`, not just what the manifest persists).
    let seed_by_id: HashMap<MatchId, DiscoverySeed> =
        seeds.into_iter().map(|s| (s.match_id, s)).collect();

    // 2. Plan.
    let now = now_unix();
    let limit = config.limit;
    let (manifest, ready) = manifest_call(manifest, move |m| m.take_ready(limit, now)).await?;
    summary.planned = ready.len();

    if config.dry_run {
        for pm in &ready {
            tracing::info!(
                match_id = %pm.match_id,
                attempts = pm.attempts,
                has_fresh_seed = seed_by_id.contains_key(&pm.match_id),
                "dry-run: would process (no download/parse/ingest performed)"
            );
        }
        tracing::info!(
            seeded = summary.seeded,
            planned = summary.planned,
            "dry-run: plan complete — discover + manifest wiring exercised, no download/ingest/DB write"
        );
        return Ok((manifest, summary));
    }

    // 3. Process, bounded by `config.concurrency` in-flight worker tasks (see the module doc's
    // "Two independent bounds" section for why the request RATE is not re-bounded here).
    let semaphore = Arc::new(Semaphore::new(config.concurrency.max(1)));
    let mut joinset: JoinSet<(MatchId, MatchOutcome, RawSaveOutcome)> = JoinSet::new();

    // See the module doc's "Raw-replay archiving" section: `None` (never for a live run unless
    // `--no-raw`) means `process_one` skips archiving entirely — no directory is even created.
    let raw_archive: Option<Arc<RawArchive>> = config
        .raw_dir
        .clone()
        .map(|dir| Arc::new(RawArchive::new(dir)));

    for pm in ready {
        if cancel.is_cancelled() {
            tracing::info!("cancellation requested — no further matches will be started this run");
            break;
        }
        let Some(seed) = seed_by_id.get(&pm.match_id).cloned() else {
            tracing::debug!(
                match_id = %pm.match_id,
                "ready for retry but no fresh discovery seed this run — skipping until rediscovered"
            );
            summary.skipped_no_seed += 1;
            continue;
        };

        // Mirrors `fetch::client::FetchClient::get_response`'s own "proceed unbounded rather than
        // crash" idiom for an acquire failure (unreachable in practice — this semaphore is never
        // `close`d) instead of `.unwrap()`ing it.
        let permit = semaphore.clone().acquire_owned().await.ok();
        let match_id = pm.match_id;
        let task_source = Arc::clone(&source);
        let task_raw_archive = raw_archive.clone();
        summary.attempted += 1;
        joinset.spawn(async move {
            let _permit = permit;
            let (outcome, raw_outcome) =
                process_one(task_source.as_ref(), match_id, seed, task_raw_archive).await;
            (match_id, outcome, raw_outcome)
        });
    }
    summary.cancelled_before_start = summary
        .planned
        .saturating_sub(summary.skipped_no_seed)
        .saturating_sub(summary.attempted);

    // Drain: every worker's outcome becomes an ingest call + a manifest `StatusUpdate` strictly
    // serially HERE — workers run concurrently, this bookkeeping does not (see the module doc).
    let mut updates: Vec<StatusUpdate> = Vec::new();
    let mut pause_for: Option<u64> = None;

    while let Some(joined) = joinset.join_next().await {
        let (match_id, outcome, raw_outcome) = match joined {
            Ok(triple) => triple,
            Err(join_err) => {
                tracing::error!(
                    error = %join_err,
                    "a worker task panicked or was cancelled — leaving its match untouched for a future retry"
                );
                continue;
            }
        };

        match raw_outcome {
            RawSaveOutcome::NotAttempted => {}
            RawSaveOutcome::Saved { bytes_written } => {
                summary.raw_saved += 1;
                summary.raw_bytes_written += bytes_written;
            }
            RawSaveOutcome::AlreadyPresent => summary.raw_already_present += 1,
            RawSaveOutcome::Failed => summary.raw_failed += 1,
        }

        match outcome {
            MatchOutcome::Ready(batch) => {
                let Some(sink) = sink.as_deref_mut() else {
                    // Guarded at the top of `crawl` (a live run always has a sink) — this branch
                    // is defensive, not reachable in practice. Fail this one match loudly rather
                    // than `.expect()`/panic.
                    summary.failed += 1;
                    updates.push(StatusUpdate {
                        error: Some("internal: no IngestSink for a live crawl".to_owned()),
                        ..StatusUpdate::new(match_id, MatchStatus::Error)
                    });
                    continue;
                };
                let ingested = tokio::select! {
                    res = sink.ingest(&batch) => res,
                    _ = cancel.cancelled() => {
                        tracing::info!(
                            match_id = %match_id,
                            "cancelled mid-ingest — the batch's transaction rolls back on drop; left for retry"
                        );
                        continue;
                    }
                };
                match ingested {
                    Ok(stats) => {
                        summary.succeeded += 1;
                        updates.push(StatusUpdate {
                            events: i64::try_from(stats.events).ok(),
                            ..StatusUpdate::new(match_id, MatchStatus::Parsed)
                        });
                    }
                    Err(err) => {
                        summary.failed += 1;
                        updates.push(StatusUpdate {
                            error: Some(format!("{err:#}")),
                            ..StatusUpdate::new(match_id, MatchStatus::Error)
                        });
                    }
                }
            }
            MatchOutcome::NoReplay => {
                summary.failed += 1;
                updates.push(StatusUpdate::new(match_id, MatchStatus::Expired));
            }
            MatchOutcome::ParseFailed(msg) => {
                summary.failed += 1;
                updates.push(StatusUpdate {
                    error: Some(msg),
                    ..StatusUpdate::new(match_id, MatchStatus::ParseFailed)
                });
            }
            MatchOutcome::Retryable {
                message,
                retry_after,
            } => {
                summary.failed += 1;
                if let Some(secs) = retry_after {
                    pause_for = Some(pause_for.map_or(secs, |p| p.max(secs)));
                }
                updates.push(StatusUpdate {
                    error: Some(message),
                    ..StatusUpdate::new(match_id, MatchStatus::Error)
                });
            }
        }
    }

    let (manifest, ()) = if updates.is_empty() {
        (manifest, ())
    } else {
        manifest_call(manifest, move |m| m.record(&updates)).await?
    };

    // A sustained 429 this run: pause here (politely) rather than let the NEXT invocation start
    // immediately and hammer straight into the same wall.
    if let Some(secs) = pause_for {
        tracing::warn!(
            seconds = secs,
            "sustained rate-limit signal this run — pausing before returning"
        );
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(secs)) => {}
            _ = cancel.cancelled() => {}
        }
    }

    Ok((manifest, summary))
}
