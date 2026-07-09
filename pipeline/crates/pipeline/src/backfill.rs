//! [`backfill`] — the ONGOING recent→old replay-enrichment run loop behind `pipeline backfill`.
//!
//! ## What it does
//! The historical `source='aoestats'` corpus (~30M matches back to 2022) carries only AGGREGATE
//! stats (civ / opening label / age uptimes / elo) — no per-command gameplay. This loop walks it
//! NEWEST-first, downloads each match's real `.aoe2record` from the age archive, re-derives the
//! full replay enrichment (build orders, APM, unit composition, watched-tech CLICK timings — the
//! SAME `replay::derive` path the crawl uses via [`crate::to_batch`]), and UPGRADES the match in
//! place to `source='replay'`. The upgrade is what distinguishes this from [`crate::crawl`]: since
//! `aoestats`' `game_id` IS the Relic match_id, an old match's replay collides with its existing
//! aggregate row, and `matches`' `ON CONFLICT (match_id) DO NOTHING` would SKIP it — so this loop
//! ingests with [`ingest::ConflictPolicy::UpgradeAoestats`], which deletes the aggregate row (+ its
//! `match_players`/`match_ages` children) first, then re-inserts the richer replay row.
//!
//! ## The archive is a rolling ~12-month window (probed 2026-07-08), not years-deep
//! `GetMatchReplay` only retains a match's replay for roughly the last year; older matches 404. So
//! the reachable pool is the RECENT aoestats slice, and it SHRINKS as matches age out — hence
//! newest-first (grab a replay while it still exists) and [`BackfillConfig::archive_floor`] (don't
//! waste archive calls descending past the retention edge into guaranteed-404 territory). A 404 is
//! per-participant, so `fetch::download_archive_replay` tries several participants before giving up.
//!
//! ## State lives entirely in Postgres — the container needs no volume
//! Resumability is derived from the DB, not a local cursor/manifest file:
//! - A SUCCESS flips the row to `source='replay'`, so it naturally drops out of this loop's
//!   `WHERE source='aoestats'` discovery on the next run.
//! - A TERMINAL failure (archive `not_found`, structurally `bad_payload`, un-`parse_failed`, or
//!   un-`compose_failed`) is recorded in `replay_backfill_misses` (migration #19), which the
//!   discovery query `NOT EXISTS`-excludes — so it's never re-attempted.
//! - A TRANSIENT failure (a surviving 429, a network blip) is recorded NOWHERE, so the next run
//!   re-selects and retries it.
//!
//! This makes every run idempotent and self-advancing with no external cursor.
//!
//! ## Bounded on every axis (the operator directive: "resources limited")
//! [`BackfillConfig::limit`] caps matches per run (the Dagster schedule is the cadence, not a
//! firehose); [`BackfillConfig::rate`] throttles archive requests GENTLY (the endpoint soft-slows
//! under sustained load) via `fetch::FetchClient`'s own GCRA governor; [`BackfillConfig::concurrency`]
//! bounds in-flight fetch+parse workers; [`BackfillConfig::batch_size`] keeps each ingest
//! transaction small (no repeat of the 2026-07-06 unbatched-write DB-host wedge); and a
//! slow-batch → [`BackfillConfig::pause`] backoff mirrors `import_shards`' load-safety valve.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use fetch::{download_archive_replay, ArchiveFetch, DiscoverySeed, FetchClient, RelicMatchType};
use ingest::{ingest_batch_with_policy, ConflictPolicy, ReplayBatch};
use pipeline_core::{MatchId, ProfileId};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_postgres::Client;
use tokio_util::sync::CancellationToken;

use crate::dimfilter::{filter_to_known_dims, load_dim_ids, merge_into};

/// One `backfill` invocation's tunables — see the module doc's "Bounded on every axis" section for
/// why each exists.
#[derive(Debug, Clone)]
pub struct BackfillConfig {
    /// Max `aoestats` matches to attempt this run (`ORDER BY played_at DESC LIMIT`).
    pub limit: usize,
    /// Max concurrent in-flight archive-fetch + parse + compose worker tasks — also the underlying
    /// archive HTTP client's own concurrency bound.
    pub concurrency: usize,
    /// Steady archive request rate (requests/min) the underlying HTTP client throttles to (GCRA).
    /// Keep GENTLE — much lower than the recent crawl's fast-path rate.
    pub rate: u32,
    /// Matches per `ingest_batch` (UpgradeAoestats) transaction. Small — bounds per-tx work.
    pub batch_size: usize,
    /// Don't descend past this `played_at` — matches older than the archive's rolling retention
    /// edge are guaranteed 404s, so attempting them just wastes archive calls (see the module doc).
    pub archive_floor: DateTime<Utc>,
    /// A batch taking longer than this is treated as a DB-load-safety signal — pause [`Self::pause`]
    /// before the next one.
    pub slow_batch: Duration,
    /// How long to pause after a slow batch (see [`Self::slow_batch`]).
    pub pause: Duration,
    /// Discover + plan only: never download, parse, or ingest. Still runs the (read-only) discovery
    /// query — safe for wiring validation.
    pub dry_run: bool,
}

/// One `backfill` invocation's outcome counts, for logging/monitoring.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackfillSummary {
    /// Candidate `aoestats` matches the discovery query returned.
    pub discovered: usize,
    /// Candidates actually handed to a worker (fetch attempted). Equals `discovered` unless
    /// cancellation cut the run short.
    pub attempted: usize,
    /// Matches successfully upgraded aoestats→replay (the ingest's `matches_upgraded` total).
    pub upgraded: u64,
    /// Matches the ingest reported as newly inserted (should equal [`Self::upgraded`] — every
    /// candidate was an existing aoestats row — but tracked separately to catch any drift).
    pub inserted: u64,
    /// Archive had no copy for any tried participant (recorded as a `not_found` miss).
    pub not_found: usize,
    /// A 200 arrived but the ZIP was structurally unusable (recorded as a `bad_payload` miss).
    pub bad_payload: usize,
    /// The `.aoe2record` downloaded but `replay::parse` rejected it (recorded as a `parse_failed`
    /// miss).
    pub parse_failed: usize,
    /// The parsed replay couldn't compose into a batch — mismatched id, missing map, or an
    /// out-of-vocab DB ladder (recorded as a `compose_failed` miss).
    pub compose_failed: usize,
    /// A surviving 429 — TRANSIENT, NOT recorded, retried on a later run.
    pub rate_limited: usize,
    /// A network / other transient download error — NOT recorded, retried on a later run.
    pub errored: usize,
    /// Rows written to `replay_backfill_misses` this run (terminal failures — see the module doc).
    pub misses_recorded: u64,
    /// FK-unsafe `match_player_units` rows dropped (out-of-dim unit ids — see [`crate::dimfilter`]).
    pub dropped_unit_rows: u64,
    /// FK-unsafe `match_player_techs` rows dropped.
    pub dropped_tech_rows: u64,
    /// DISTINCT out-of-dim unit ids seen (surfaced so an operator can decide whether to extend the
    /// dim — never fabricated into a row).
    pub unknown_unit_ids: BTreeSet<i32>,
    /// DISTINCT out-of-dim tech ids seen (expected empty).
    pub unknown_tech_ids: BTreeSet<i32>,
}

/// One discovered `aoestats` match to try upgrading. Carries exactly what
/// [`crate::to_batch`] needs that a parsed replay can't supply — the `ladder` (→ `RelicMatchType`)
/// and `played_at` — plus the `profile_ids` the archive endpoint keys on. This is why the backfill
/// has NO "seed persistence gap" the crawl has (see `crawl`'s doc): the full seed is reconstructed
/// from the DB row, not a fresh discovery call.
struct Candidate {
    match_id: MatchId,
    ladder: String,
    played_at: DateTime<Utc>,
    profile_ids: Vec<ProfileId>,
}

/// A terminal per-match failure — recorded in `replay_backfill_misses` so it's never retried.
/// Its [`Self::as_db_str`] MUST stay within migration #19's `reason` CHECK vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MissReason {
    NotFound,
    BadPayload,
    ParseFailed,
    ComposeFailed,
}

impl MissReason {
    fn as_db_str(self) -> &'static str {
        match self {
            MissReason::NotFound => "not_found",
            MissReason::BadPayload => "bad_payload",
            MissReason::ParseFailed => "parse_failed",
            MissReason::ComposeFailed => "compose_failed",
        }
    }
}

/// One worker's terminal outcome, drained serially by [`backfill`] into ingest calls + miss rows.
/// A worker only does IO + parse + compose; FK-safe dim filtering happens on the serial drain (it
/// needs the dims and is cheap), so this carries just the composed batch, not the dims/counts.
enum MatchOutcome {
    /// Downloaded, parsed, and composed — ready for the drain to dim-filter and upgrade-ingest.
    Ready(ReplayBatch),
    /// A terminal failure — record in `replay_backfill_misses`, never retry.
    Miss(MissReason),
    /// A surviving 429 — transient, NOT recorded, retried next run.
    RateLimited,
    /// A network / other transient download error — NOT recorded, retried next run.
    Errored,
}

/// Discovery: the recent→old, skip-done-and-known-misses candidate query. Read-only.
///
/// LIMIT-FIRST, then join: the inner subquery picks the `$2` newest still-`aoestats`, not-yet-missed
/// matches via the `played_at` index (a bounded backward index scan — NOT a scan of all ~8.5M
/// aoestats rows), and ONLY THEN does the outer query join `match_players` to collect their
/// `profile_ids`. Folding the `JOIN … GROUP BY` in before the `LIMIT` (the obvious phrasing) makes
/// PG aggregate every aoestats match's players before ordering — a ~66s full pass measured live;
/// this keeps discovery near-instant regardless of corpus size.
const DISCOVER_SQL: &str = r#"
SELECT c.match_id,
       c.ladder,
       c.played_at,
       array_agg(mp.profile_id ORDER BY mp.profile_id) AS profile_ids
FROM (
    SELECT m.match_id, m.ladder::text AS ladder, m.played_at
    FROM matches m
    WHERE m.source = 'aoestats'
      AND m.played_at >= $1
      AND NOT EXISTS (SELECT 1 FROM replay_backfill_misses x WHERE x.match_id = m.match_id)
    ORDER BY m.played_at DESC
    LIMIT $2
) c
JOIN match_players mp ON mp.match_id = c.match_id
GROUP BY c.match_id, c.ladder, c.played_at
ORDER BY c.played_at DESC
"#;

/// Batch-record terminal misses. `ON CONFLICT DO NOTHING` guards the (theoretically impossible,
/// since discovery excludes existing misses) re-record; `attempted_at` defaults to `now()`.
const RECORD_MISSES_SQL: &str = "\
INSERT INTO replay_backfill_misses (match_id, reason) \
SELECT * FROM UNNEST($1::bigint[], $2::text[]) \
ON CONFLICT (match_id) DO NOTHING";

async fn discover(client: &Client, cfg: &BackfillConfig) -> Result<Vec<Candidate>> {
    let rows = client
        .query(DISCOVER_SQL, &[&cfg.archive_floor, &(cfg.limit as i64)])
        .await
        .context("backfill discovery query failed")?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let match_id: i64 = row.get("match_id");
        let ladder: String = row.get("ladder");
        let played_at: DateTime<Utc> = row.get("played_at");
        let profile_ids: Vec<i64> = row.get("profile_ids");
        out.push(Candidate {
            match_id: MatchId(match_id),
            ladder,
            played_at,
            profile_ids: profile_ids.into_iter().map(ProfileId).collect(),
        });
    }
    Ok(out)
}

/// Download + parse + compose one candidate into an [`MatchOutcome`]. NO DB access and no dim filtering
/// — those belong to the serial drain in [`backfill`], which owns the single `&mut Client`.
#[tracing::instrument(skip(client, cand), fields(match_id = %cand.match_id))]
async fn process_one(client: Arc<FetchClient>, cand: Candidate) -> (MatchId, MatchOutcome) {
    let match_id = cand.match_id;

    let bytes = match download_archive_replay(&client, match_id, &cand.profile_ids).await {
        Ok(ArchiveFetch::Ok(bytes, _pid)) => bytes,
        Ok(ArchiveFetch::NotFound) => return (match_id, MatchOutcome::Miss(MissReason::NotFound)),
        Ok(ArchiveFetch::BadPayload(msg)) => {
            tracing::debug!(%msg, "archive returned a structurally bad ZIP — terminal");
            return (match_id, MatchOutcome::Miss(MissReason::BadPayload));
        }
        Ok(ArchiveFetch::RateLimited { retry_after }) => {
            tracing::debug!(?retry_after, "archive rate-limited this match — will retry next run");
            return (match_id, MatchOutcome::RateLimited);
        }
        Err(err) => {
            tracing::debug!(error = %err, "archive download errored — will retry next run");
            return (match_id, MatchOutcome::Errored);
        }
    };

    // CPU-bound decode off the async runtime thread (playbook rule, same as `FetchSource`).
    let parsed = match tokio::task::spawn_blocking(move || replay::parse(match_id, bytes)).await {
        Ok(Ok(parsed)) => parsed,
        Ok(Err(err)) => {
            tracing::debug!(error = %err, "replay::parse rejected the archived record — terminal");
            return (match_id, MatchOutcome::Miss(MissReason::ParseFailed));
        }
        Err(join_err) => {
            tracing::warn!(error = %join_err, "parse task panicked — treating as parse failure");
            return (match_id, MatchOutcome::Miss(MissReason::ParseFailed));
        }
    };

    // Reconstruct the seed the replay can't carry from the DB row. `from_db_ladder` fails loud on
    // any ladder outside the `ladder_kind` enum's two values (unreachable — it came FROM that enum
    // — but never guessed); treat that as a compose-level terminal miss.
    let match_type = match RelicMatchType::from_db_ladder(&cand.ladder) {
        Ok(t) => t,
        Err(err) => {
            tracing::warn!(error = %err, ladder = %cand.ladder, "unmappable DB ladder — terminal");
            return (match_id, MatchOutcome::Miss(MissReason::ComposeFailed));
        }
    };
    let seed = DiscoverySeed {
        match_id,
        match_type,
        played_at: cand.played_at,
        new_rating: None,
    };

    match crate::to_batch(parsed, seed) {
        Ok(batch) => (match_id, MatchOutcome::Ready(batch)),
        Err(err) => {
            tracing::debug!(error = %err, "to_batch rejected the composed replay — terminal");
            (match_id, MatchOutcome::Miss(MissReason::ComposeFailed))
        }
    }
}

/// Ingest `pending` (if non-empty) in ONE `UpgradeAoestats` transaction, folding the outcome into
/// `summary` and resetting `pending`. Unlike `import_shards`' flush, an ingest error is FATAL here
/// (propagated): the known FK risk (out-of-dim units) is already handled by `dimfilter`, so a
/// surviving ingest error is genuinely unexpected (DB down, disk full) and must fail the run loudly
/// rather than silently loop on it. A slow batch triggers the load-safety pause.
async fn flush(
    client: &mut Client,
    pending: &mut ReplayBatch,
    cfg: &BackfillConfig,
    summary: &mut BackfillSummary,
    cancel: &CancellationToken,
) -> Result<()> {
    if pending.matches.is_empty() {
        return Ok(());
    }
    let n = pending.matches.len();
    let started = Instant::now();

    let ingested = tokio::select! {
        res = ingest_batch_with_policy(client, pending, ConflictPolicy::UpgradeAoestats) => res,
        _ = cancel.cancelled() => {
            tracing::info!(matches = n, "cancelled mid-ingest — batch rolls back on drop; left for retry");
            *pending = ReplayBatch::default();
            return Ok(());
        }
    };
    let stats = ingested.context("backfill upgrade-ingest batch failed")?;
    summary.upgraded += stats.matches_upgraded;
    summary.inserted += stats.matches_inserted;
    let elapsed = started.elapsed();
    tracing::info!(
        matches = n,
        upgraded = stats.matches_upgraded,
        inserted = stats.matches_inserted,
        elapsed_ms = elapsed.as_millis() as u64,
        "backfill batch upgraded",
    );
    *pending = ReplayBatch::default();

    if elapsed > cfg.slow_batch {
        tracing::warn!(
            elapsed_ms = elapsed.as_millis() as u64,
            pause_ms = cfg.pause.as_millis() as u64,
            "slow backfill batch — pausing before the next (DB load-safety)",
        );
        tokio::select! {
            _ = tokio::time::sleep(cfg.pause) => {}
            _ = cancel.cancelled() => {}
        }
    }
    Ok(())
}

/// Write the run's terminal misses to `replay_backfill_misses`. Returns the rows actually written.
/// The `MatchId` newtype is unwrapped to the raw `i64` the wire needs only HERE, at the DB boundary.
async fn record_misses(client: &mut Client, misses: &[(MatchId, MissReason)]) -> Result<u64> {
    if misses.is_empty() {
        return Ok(0);
    }
    let (match_ids, reasons): (Vec<i64>, Vec<String>) = misses
        .iter()
        .map(|(id, reason)| (id.0, reason.as_db_str().to_owned()))
        .unzip();
    client
        .execute(RECORD_MISSES_SQL, &[&match_ids, &reasons])
        .await
        .context("failed to record backfill misses")
}

/// Run one recent→old backfill pass. See the module doc for the full shape. Never propagates a
/// per-match failure (each becomes a miss row or a retry-next-run); the only fatal conditions are
/// the discovery query, dim load, client build, an ingest-batch error, or the misses write.
#[tracing::instrument(skip(client, cfg, cancel))]
pub async fn backfill(
    client: &mut Client,
    cfg: &BackfillConfig,
    cancel: &CancellationToken,
) -> Result<BackfillSummary> {
    let mut summary = BackfillSummary::default();

    // 1. Discover (read-only).
    let candidates = discover(client, cfg).await?;
    summary.discovered = candidates.len();
    tracing::info!(
        discovered = candidates.len(),
        archive_floor = %cfg.archive_floor,
        limit = cfg.limit,
        "backfill discovered candidate aoestats matches (newest-first)",
    );

    if cfg.dry_run {
        tracing::info!("dry-run: no download/parse/ingest performed");
        return Ok(summary);
    }
    if candidates.is_empty() {
        tracing::info!("no candidates — nothing to backfill this run");
        return Ok(summary);
    }

    // 2. Dims for FK-safe child filtering (see `dimfilter`) — used by the serial drain below.
    let dims = load_dim_ids(client).await?;

    // 3. Gentle, rate-limited archive client.
    let fetch_client = Arc::new(
        FetchClient::with_limits(cfg.rate, cfg.concurrency.max(1))
            .context("failed to build the archive fetch client")?,
    );

    // 4. Fetch + parse + compose with bounded concurrency.
    let semaphore = Arc::new(Semaphore::new(cfg.concurrency.max(1)));
    let mut joinset: JoinSet<(MatchId, MatchOutcome)> = JoinSet::new();
    for cand in candidates {
        if cancel.is_cancelled() {
            tracing::info!("cancellation requested — no further matches will be started this run");
            break;
        }
        // `.ok()` (not `.unwrap()`): mirrors `crawl::crawl`'s idiom — proceed unbounded rather than
        // crash on an acquire failure (unreachable in practice; the semaphore is never closed).
        let permit = semaphore.clone().acquire_owned().await.ok();
        let fc = Arc::clone(&fetch_client);
        summary.attempted += 1;
        joinset.spawn(async move {
            let _permit = permit;
            process_one(fc, cand).await
        });
    }

    // 5. Drain SERIALLY: accumulate Ready into batch_size-sized upgrade-ingest transactions;
    //    collect misses; count transients. This task is the single owner of `&mut Client`.
    let mut pending = ReplayBatch::default();
    let mut misses: Vec<(MatchId, MissReason)> = Vec::new();

    while let Some(joined) = joinset.join_next().await {
        let (match_id, outcome) = match joined {
            Ok(pair) => pair,
            Err(join_err) => {
                tracing::error!(error = %join_err, "backfill worker panicked — leaving its match for a future run");
                continue;
            }
        };

        match outcome {
            MatchOutcome::Ready(mut batch) => {
                // FK-safe dim filtering on the serial drain (cheap, and it needs the dims) — drop
                // out-of-dim unit/tech rows before batching so one can't abort the whole tx.
                let counts = filter_to_known_dims(&mut batch, &dims);
                summary.dropped_unit_rows += counts.dropped_unit_rows;
                summary.dropped_tech_rows += counts.dropped_tech_rows;
                summary.unknown_unit_ids.extend(counts.unknown_unit_ids);
                summary.unknown_tech_ids.extend(counts.unknown_tech_ids);
                // `to_batch` yields exactly one match per candidate, so `pending.matches.len()` IS
                // the accumulated match count — no separate counter to keep in sync. `flush` resets
                // `pending`, so the length drops back to 0 after a flush.
                merge_into(&mut pending, batch);
                if pending.matches.len() >= cfg.batch_size {
                    flush(client, &mut pending, cfg, &mut summary, cancel).await?;
                }
            }
            MatchOutcome::Miss(reason) => {
                match reason {
                    MissReason::NotFound => summary.not_found += 1,
                    MissReason::BadPayload => summary.bad_payload += 1,
                    MissReason::ParseFailed => summary.parse_failed += 1,
                    MissReason::ComposeFailed => summary.compose_failed += 1,
                }
                misses.push((match_id, reason));
            }
            MatchOutcome::RateLimited => summary.rate_limited += 1,
            MatchOutcome::Errored => summary.errored += 1,
        }
    }

    // Final partial batch.
    flush(client, &mut pending, cfg, &mut summary, cancel).await?;

    // 6. Record terminal misses so they're never retried.
    summary.misses_recorded = record_misses(client, &misses).await?;

    tracing::info!(?summary, "backfill run complete");
    Ok(summary)
}
