//! The resumable download manifest: one row per `match_id` recording exactly where each replay
//! stands, so a crawl is fully resumable and idempotent (every decision is on disk, not in memory).
//!
//! Restructured from `scripts/data-pipeline/replay-rs/src/manifest.rs` to the playbook's shape:
//! the manifest POLICY (what's eligible to attempt now, when an errored match may retry, dedup on
//! seed) is captured by the [`ReplayManifest`] trait; [`SqliteManifest`] is the one production
//! implementation (rusqlite, WAL, `busy_timeout`). Consumers are generic `fn …<M: ReplayManifest>`
//! so the scheduling policy can be unit-tested exhaustively against an in-memory fake with no
//! SQLite — this is the one trait in `fetch` that earns its keep (the playbook's ≥2-impls rule).
//!
//! The trait is SYNC: rusqlite is a synchronous C library. The M6 run-loop owns a single
//! `SqliteManifest` on a dedicated task and reaches it via `spawn_blocking` (never a scattered
//! `Mutex<Connection>`); this crate provides the policy + storage, not that orchestration.

use std::time::Duration;

use rusqlite::Connection;

use pipeline_core::MatchId;

use crate::error::Result;

// --- retry policy (the manifest's scheduling POLICY, as pure, testable constants + fn) ---------

/// An errored match is abandoned after this many attempts (it stops being handed out by
/// [`ReplayManifest::take_ready`]). A genuinely-transient failure clears well within this; a match
/// that fails this many times is effectively dead and shouldn't burn the crawl budget forever.
///
/// MUST stay strictly greater than [`BACKOFF_CAP_SHIFT`]: `retry_ready`'s shift is
/// `attempts.clamp(0, BACKOFF_CAP_SHIFT)`, so the plateau only actually engages once `attempts`
/// reaches the shift cap. `MAX_ATTEMPTS <= BACKOFF_CAP_SHIFT` would mean every match is abandoned
/// before the backoff ever plateaus — dead code, caught by
/// `retry_ready_respects_attempt_cap_and_exponential_backoff`'s plateau assertions.
pub const MAX_ATTEMPTS: i64 = 8;
/// Base backoff before the first retry of an errored match (seconds).
pub const BACKOFF_BASE_SECS: i64 = 60;
/// Cap on the exponential shift, so the backoff plateaus (at `BACKOFF_BASE_SECS << CAP`) instead of
/// growing unbounded with the attempt count. See [`MAX_ATTEMPTS`]'s doc for the invariant between
/// the two constants.
pub const BACKOFF_CAP_SHIFT: i64 = 6;

// Compile-time guard for the invariant documented on `MAX_ATTEMPTS`: catches a future edit that
// re-introduces the dead-plateau bug at build time rather than relying on the test to catch it.
const _: () = assert!(MAX_ATTEMPTS > BACKOFF_CAP_SHIFT);

/// Is an errored match eligible to retry *now*? Pure so it can be tested exhaustively and reused
/// verbatim by both [`SqliteManifest`] (whose SQL `WHERE` mirrors it — see the query) and the
/// in-memory fake, so the two can never diverge on the policy.
///
/// Eligible iff it's under the attempt cap AND at least an exponentially-growing backoff window
/// has elapsed since its last update. All arithmetic is saturating: a corrupt/huge `updated_at`
/// or `attempts` can never panic.
pub fn retry_ready(attempts: i64, updated_at: i64, now: i64) -> bool {
    if attempts >= MAX_ATTEMPTS {
        return false;
    }
    let shift = attempts.clamp(0, BACKOFF_CAP_SHIFT) as u32;
    let backoff = BACKOFF_BASE_SECS.saturating_mul(1i64 << shift);
    now.saturating_sub(updated_at) >= backoff
}

// --- typed status vocabulary (parse, don't validate) ------------------------------------------

/// The closed set of states a match can be in. Unlike [`RelicMatchType`](crate::RelicMatchType)
/// this is OUR vocabulary, so it is deliberately exhaustive (no `#[non_exhaustive]`): adding a
/// state must force every match site to handle it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatchStatus {
    /// Not yet looked up (the seed default).
    Pending,
    /// Downloaded + parsed + stored (terminal).
    Parsed,
    /// Files exist but all are empty/`-1` (terminal).
    NoReplay,
    /// Aged out of `getReplayFiles` with no archive fallback available (terminal unless a later
    /// seed enriches it with participant ids — see [`ReplayManifest::seed`]).
    Expired,
    /// Downloaded but could not be parsed — deterministic, so terminal (retrying re-burns budget).
    ParseFailed,
    /// A transient failure (network, rate-limit); retryable per [`retry_ready`].
    Error,
}

impl MatchStatus {
    /// The exact lowercase token stored in the `status` column — MUST round-trip with [`Self::parse`].
    pub const fn as_db_str(self) -> &'static str {
        match self {
            MatchStatus::Pending => "pending",
            MatchStatus::Parsed => "parsed",
            MatchStatus::NoReplay => "no_replay",
            MatchStatus::Expired => "expired",
            MatchStatus::ParseFailed => "parse_failed",
            MatchStatus::Error => "error",
        }
    }

    /// Parse a stored status token, failing loud on anything outside the vocabulary (a corrupt
    /// manifest must surface, not be silently coerced).
    pub fn parse(s: &str) -> std::result::Result<Self, UnknownStatus> {
        match s {
            "pending" => Ok(MatchStatus::Pending),
            "parsed" => Ok(MatchStatus::Parsed),
            "no_replay" => Ok(MatchStatus::NoReplay),
            "expired" => Ok(MatchStatus::Expired),
            "parse_failed" => Ok(MatchStatus::ParseFailed),
            "error" => Ok(MatchStatus::Error),
            other => Err(UnknownStatus(other.to_owned())),
        }
    }
}

/// A `status` token in the manifest outside the known vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown manifest status {0:?}")]
pub struct UnknownStatus(pub String);

// --- row/update value types -------------------------------------------------------------------

/// A row to seed. New ids start `pending`; an existing id is left as-is except that a NULL
/// `profile_ids` is enriched (and an `expired` row flipped back to `pending`) when a seed first
/// supplies participant ids — see [`ReplayManifest::seed`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedRow {
    pub match_id: MatchId,
    /// Unix seconds the match was played, used only to order `take_ready` (newest first).
    pub played_at: Option<i64>,
    /// Semicolon-separated participant profile ids — the age-archive fallback key.
    pub profile_ids: Option<String>,
}

/// A match handed back by [`ReplayManifest::take_ready`], ready to attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMatch {
    pub match_id: MatchId,
    pub played_at: Option<i64>,
    pub profile_ids: Option<String>,
    /// Attempts already spent (0 for a fresh `pending` row).
    pub attempts: i64,
}

/// A status transition for one match. Only `match_id` + `status` are required; the metadata
/// mirrors the optional columns the crawl records for inspection/progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusUpdate {
    pub match_id: MatchId,
    pub status: MatchStatus,
    /// Bump the attempt counter (set for a real attempt; clear for a pure metadata touch).
    pub attempts_inc: bool,
    pub n_files: Option<i64>,
    pub best_size: Option<i64>,
    pub best_pid: Option<i64>,
    pub events: Option<i64>,
    pub error: Option<String>,
}

impl StatusUpdate {
    /// A plain "match reached `status` after one attempt" update, no metadata.
    pub fn new(match_id: MatchId, status: MatchStatus) -> Self {
        StatusUpdate {
            match_id,
            status,
            attempts_inc: true,
            n_files: None,
            best_size: None,
            best_pid: None,
            events: None,
            error: None,
        }
    }
}

// --- the policy trait -------------------------------------------------------------------------

/// The resumable-manifest POLICY: seed/dedup, hand out what's eligible now, record outcomes,
/// report counts. One production impl ([`SqliteManifest`]) + an in-memory test fake exercise the
/// same scheduling logic ([`retry_ready`]).
pub trait ReplayManifest {
    /// Insert seed rows, deduping by `match_id`. Returns the count NEWLY inserted. For an existing
    /// row that had no `profile_ids`, a seed carrying them enriches it in place — and if that row
    /// was `expired`, flips it back to `pending` exactly once (expiry only meant the fast path aged
    /// out; participant ids open the archive-fallback avenue). Once the archive also fails, the row
    /// re-`expired`s WITH ids set, so a later identical seed can't flip it again.
    fn seed(&mut self, rows: &[SeedRow]) -> Result<usize>;

    /// Up to `limit` matches eligible to attempt at `now` (unix seconds), newest `played_at` first:
    /// every `pending` row, plus every `error` row that [`retry_ready`] admits.
    fn take_ready(&self, limit: usize, now: i64) -> Result<Vec<PendingMatch>>;

    /// Apply a batch of status transitions atomically.
    fn record(&mut self, updates: &[StatusUpdate]) -> Result<()>;

    /// `status → count`, for progress/inspection.
    fn counts(&self) -> Result<Vec<(MatchStatus, i64)>>;
}

// --- SQLite implementation --------------------------------------------------------------------

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS matches (
  match_id    INTEGER PRIMARY KEY,
  played_at   INTEGER,
  profile_ids TEXT,
  status      TEXT NOT NULL DEFAULT 'pending',
  attempts    INTEGER NOT NULL DEFAULT 0,
  n_files     INTEGER,
  best_size   INTEGER,
  best_pid    INTEGER,
  events      INTEGER,
  error       TEXT,
  updated_at  INTEGER
);
CREATE INDEX IF NOT EXISTS idx_matches_status ON matches(status);
";

/// The rusqlite-backed [`ReplayManifest`]. Holds its own `Connection` (no `Arc<Mutex>` — the M6
/// owner task holds it exclusively). WAL + `busy_timeout` so a reader never hard-fails against the
/// single writer.
pub struct SqliteManifest {
    con: Connection,
}

impl SqliteManifest {
    /// Open (creating if absent) the manifest DB at `path`, in WAL mode with a 5s busy timeout.
    pub fn open(path: &str) -> Result<Self> {
        let con = Connection::open(path)?;
        con.pragma_update(None, "journal_mode", "WAL")?;
        con.pragma_update(None, "synchronous", "NORMAL")?;
        con.busy_timeout(Duration::from_secs(5))?;
        con.execute_batch(SCHEMA)?;
        Ok(SqliteManifest { con })
    }

    fn now() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}

impl ReplayManifest for SqliteManifest {
    fn seed(&mut self, rows: &[SeedRow]) -> Result<usize> {
        let now = Self::now();
        let tx = self.con.transaction()?;
        let mut inserted = 0usize;
        {
            let mut ins = tx.prepare(
                "INSERT OR IGNORE INTO matches(match_id, played_at, profile_ids, updated_at)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            let mut enrich = tx.prepare(
                "UPDATE matches SET
                   profile_ids = ?2,
                   status = CASE WHEN status = 'expired' THEN 'pending' ELSE status END,
                   updated_at = ?3
                 WHERE match_id = ?1 AND profile_ids IS NULL",
            )?;
            for r in rows {
                let n = ins.execute(rusqlite::params![
                    r.match_id.0,
                    r.played_at,
                    r.profile_ids,
                    now
                ])?;
                inserted += n;
                if n == 0 {
                    if let Some(pids) = &r.profile_ids {
                        enrich.execute(rusqlite::params![r.match_id.0, pids, now])?;
                    }
                }
            }
        }
        tx.commit()?;
        Ok(inserted)
    }

    fn take_ready(&self, limit: usize, now: i64) -> Result<Vec<PendingMatch>> {
        // This `WHERE` mirrors `retry_ready` exactly (the elo_bucket-style Rust↔SQL contract): a
        // `pending` row, or an `error` row under the attempt cap whose exponential backoff window
        // (BACKOFF_BASE_SECS << min(attempts, CAP)) has elapsed. Keep the two in lockstep.
        let mut stmt = self.con.prepare(
            "SELECT match_id, played_at, profile_ids, attempts FROM matches
             WHERE status = 'pending'
                OR (status = 'error'
                    AND attempts < ?1
                    AND (?2 - COALESCE(updated_at, 0)) >= ?3 * (1 << MIN(attempts, ?4)))
             ORDER BY played_at DESC
             LIMIT ?5",
        )?;
        let rows = stmt
            .query_map(
                rusqlite::params![
                    MAX_ATTEMPTS,
                    now,
                    BACKOFF_BASE_SECS,
                    BACKOFF_CAP_SHIFT,
                    limit as i64
                ],
                |r| {
                    Ok(PendingMatch {
                        match_id: MatchId(r.get(0)?),
                        played_at: r.get(1)?,
                        profile_ids: r.get(2)?,
                        attempts: r.get(3)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn record(&mut self, updates: &[StatusUpdate]) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }
        let now = Self::now();
        let tx = self.con.transaction()?;
        for u in updates {
            // Build the SET clause from only the supplied columns, so a metadata-free transition
            // never clobbers earlier metadata with NULLs.
            let mut sets: Vec<&str> = vec!["status = ?", "updated_at = ?"];
            let mut vals: Vec<Box<dyn rusqlite::types::ToSql>> =
                vec![Box::new(u.status.as_db_str()), Box::new(now)];
            if let Some(v) = u.n_files {
                sets.push("n_files = ?");
                vals.push(Box::new(v));
            }
            if let Some(v) = u.best_size {
                sets.push("best_size = ?");
                vals.push(Box::new(v));
            }
            if let Some(v) = u.best_pid {
                sets.push("best_pid = ?");
                vals.push(Box::new(v));
            }
            if let Some(v) = u.events {
                sets.push("events = ?");
                vals.push(Box::new(v));
            }
            if let Some(v) = &u.error {
                sets.push("error = ?");
                vals.push(Box::new(v.clone()));
            }
            if u.attempts_inc {
                sets.push("attempts = attempts + 1");
            }
            vals.push(Box::new(u.match_id.0));
            let sql = format!("UPDATE matches SET {} WHERE match_id = ?", sets.join(", "));
            let params: Vec<&dyn rusqlite::types::ToSql> = vals.iter().map(|b| b.as_ref()).collect();
            tx.execute(&sql, params.as_slice())?;
        }
        tx.commit()?;
        Ok(())
    }

    fn counts(&self) -> Result<Vec<(MatchStatus, i64)>> {
        let mut stmt = self
            .con
            .prepare("SELECT status, count(*) FROM matches GROUP BY status ORDER BY status")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        // Fail loud on a status token we don't recognize rather than silently drop it.
        rows.into_iter()
            .map(|(s, n)| {
                MatchStatus::parse(&s)
                    .map(|status| (status, n))
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)).into())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// In-memory [`ReplayManifest`] fake: a `HashMap<match_id, Row>`. Exercises the SAME scheduling
    /// policy ([`retry_ready`], the seed enrichment/flip rules) as [`SqliteManifest`], with no
    /// SQLite — so the policy can be tested exhaustively and deterministically.
    #[derive(Default)]
    struct InMemoryManifest {
        rows: HashMap<i64, Row>,
        clock: i64,
    }

    #[derive(Clone)]
    struct Row {
        played_at: Option<i64>,
        profile_ids: Option<String>,
        status: MatchStatus,
        attempts: i64,
        updated_at: i64,
    }

    impl ReplayManifest for InMemoryManifest {
        fn seed(&mut self, rows: &[SeedRow]) -> Result<usize> {
            let now = self.clock;
            let mut inserted = 0;
            for r in rows {
                match self.rows.get_mut(&r.match_id.0) {
                    None => {
                        self.rows.insert(
                            r.match_id.0,
                            Row {
                                played_at: r.played_at,
                                profile_ids: r.profile_ids.clone(),
                                status: MatchStatus::Pending,
                                attempts: 0,
                                updated_at: now,
                            },
                        );
                        inserted += 1;
                    }
                    Some(existing) => {
                        // Enrich a NULL profile_ids once; flip expired→pending exactly then.
                        if existing.profile_ids.is_none() {
                            if let Some(pids) = &r.profile_ids {
                                existing.profile_ids = Some(pids.clone());
                                if existing.status == MatchStatus::Expired {
                                    existing.status = MatchStatus::Pending;
                                }
                                existing.updated_at = now;
                            }
                        }
                    }
                }
            }
            Ok(inserted)
        }

        fn take_ready(&self, limit: usize, now: i64) -> Result<Vec<PendingMatch>> {
            let mut eligible: Vec<(&i64, &Row)> = self
                .rows
                .iter()
                .filter(|(_, row)| match row.status {
                    MatchStatus::Pending => true,
                    MatchStatus::Error => retry_ready(row.attempts, row.updated_at, now),
                    _ => false,
                })
                .collect();
            // Newest played_at first; None sorts last (mirrors SQL `ORDER BY played_at DESC`).
            eligible.sort_by_key(|(_, row)| std::cmp::Reverse(row.played_at));
            Ok(eligible
                .into_iter()
                .take(limit)
                .map(|(id, row)| PendingMatch {
                    match_id: MatchId(*id),
                    played_at: row.played_at,
                    profile_ids: row.profile_ids.clone(),
                    attempts: row.attempts,
                })
                .collect())
        }

        fn record(&mut self, updates: &[StatusUpdate]) -> Result<()> {
            let now = self.clock;
            for u in updates {
                if let Some(row) = self.rows.get_mut(&u.match_id.0) {
                    row.status = u.status;
                    row.updated_at = now;
                    if u.attempts_inc {
                        row.attempts += 1;
                    }
                }
            }
            Ok(())
        }

        fn counts(&self) -> Result<Vec<(MatchStatus, i64)>> {
            let mut by: HashMap<&'static str, (MatchStatus, i64)> = HashMap::new();
            for row in self.rows.values() {
                let e = by.entry(row.status.as_db_str()).or_insert((row.status, 0));
                e.1 += 1;
            }
            let mut out: Vec<(MatchStatus, i64)> = by.into_values().collect();
            out.sort_by_key(|(s, _)| s.as_db_str());
            Ok(out)
        }
    }

    fn seed_row(id: i64, played_at: i64, pids: Option<&str>) -> SeedRow {
        SeedRow {
            match_id: MatchId(id),
            played_at: Some(played_at),
            profile_ids: pids.map(str::to_owned),
        }
    }

    // --- pure retry_ready policy (exhaustive boundary coverage) --------------------------------

    #[test]
    fn retry_ready_respects_attempt_cap_and_exponential_backoff() {
        // Under the cap, attempts=1 → backoff = 60 << 1 = 120s.
        assert!(!retry_ready(1, 1_000, 1_000 + 119), "119s < 120s window");
        assert!(retry_ready(1, 1_000, 1_000 + 120), "120s reaches the window");
        // attempts=0 → 60 << 0 = 60s.
        assert!(!retry_ready(0, 0, 59));
        assert!(retry_ready(0, 0, 60));
        // The shift is capped, so a huge attempt count (still < MAX) plateaus, never overflows.
        let capped = BACKOFF_BASE_SECS << BACKOFF_CAP_SHIFT;
        assert!(retry_ready(MAX_ATTEMPTS - 1, 0, capped));
        assert!(!retry_ready(MAX_ATTEMPTS - 1, 0, capped - 1));
        // At/over the attempt cap: never eligible, however long ago it failed.
        assert!(!retry_ready(MAX_ATTEMPTS, 0, i64::MAX / 2));
        assert!(!retry_ready(MAX_ATTEMPTS + 3, 0, i64::MAX / 2));
    }

    // --- policy through the fake --------------------------------------------------------------

    #[test]
    fn seed_dedups_by_match_id() {
        let mut m = InMemoryManifest::default();
        assert_eq!(m.seed(&[seed_row(1, 10, None), seed_row(2, 20, None)]).unwrap(), 2);
        // Re-seeding known ids inserts nothing new.
        assert_eq!(m.seed(&[seed_row(1, 10, None), seed_row(2, 20, None)]).unwrap(), 0);
        assert_eq!(m.seed(&[seed_row(3, 30, None)]).unwrap(), 1);
    }

    #[test]
    fn take_ready_returns_pending_newest_first_and_honors_limit() {
        let mut m = InMemoryManifest::default();
        m.seed(&[seed_row(1, 10, None), seed_row(2, 30, None), seed_row(3, 20, None)])
            .unwrap();
        let ready = m.take_ready(10, 1_000).unwrap();
        assert_eq!(
            ready.iter().map(|r| r.match_id.0).collect::<Vec<_>>(),
            vec![2, 3, 1],
            "newest played_at first"
        );
        // Limit caps the batch to the newest N.
        let top1 = m.take_ready(1, 1_000).unwrap();
        assert_eq!(top1.len(), 1);
        assert_eq!(top1[0].match_id.0, 2);
    }

    #[test]
    fn record_increments_attempts_and_transitions_status() {
        let mut m = InMemoryManifest {
            clock: 1_000,
            ..Default::default()
        };
        m.seed(&[seed_row(1, 10, None)]).unwrap();
        m.record(&[StatusUpdate::new(MatchId(1), MatchStatus::Error)]).unwrap();
        // Errored at t=1000 attempts=1 → not eligible until t=1000+120.
        assert!(m.take_ready(10, 1_100).unwrap().is_empty(), "still inside backoff");
        let ready = m.take_ready(10, 1_000 + 120).unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].attempts, 1, "attempt was incremented");
    }

    #[test]
    fn terminal_statuses_are_never_taken() {
        let mut m = InMemoryManifest::default();
        m.seed(&[seed_row(1, 10, None), seed_row(2, 20, None), seed_row(3, 30, None)])
            .unwrap();
        m.record(&[
            StatusUpdate::new(MatchId(1), MatchStatus::Parsed),
            StatusUpdate::new(MatchId(2), MatchStatus::Expired),
            StatusUpdate::new(MatchId(3), MatchStatus::ParseFailed),
        ])
        .unwrap();
        assert!(
            m.take_ready(10, i64::MAX / 2).unwrap().is_empty(),
            "parsed/expired/parse_failed are terminal — never re-handed-out"
        );
    }

    #[test]
    fn errored_match_is_abandoned_after_the_attempt_cap() {
        let mut m = InMemoryManifest::default();
        m.seed(&[seed_row(1, 10, None)]).unwrap();
        // Fail it MAX_ATTEMPTS times.
        for _ in 0..MAX_ATTEMPTS {
            m.record(&[StatusUpdate::new(MatchId(1), MatchStatus::Error)]).unwrap();
        }
        assert!(
            m.take_ready(10, i64::MAX / 2).unwrap().is_empty(),
            "a match at the attempt cap is no longer eligible, however long we wait"
        );
    }

    #[test]
    fn seed_enriches_profile_ids_and_flips_expired_exactly_once() {
        let mut m = InMemoryManifest {
            clock: 1,
            ..Default::default()
        };
        // A row goes expired without participant ids.
        m.seed(&[seed_row(2, 20, None)]).unwrap();
        m.record(&[StatusUpdate::new(MatchId(2), MatchStatus::Expired)]).unwrap();
        // A seed carrying ids enriches it AND flips expired→pending.
        m.seed(&[seed_row(2, 20, Some("30;40"))]).unwrap();
        let ready = m.take_ready(10, i64::MAX / 2).unwrap();
        let p2 = ready.iter().find(|r| r.match_id.0 == 2).expect("row 2 back to pending");
        assert_eq!(p2.profile_ids.as_deref(), Some("30;40"));
        // Archive also fails → expired WITH ids; a later identical seed must NOT flip it again.
        m.record(&[StatusUpdate::new(MatchId(2), MatchStatus::Expired)]).unwrap();
        m.seed(&[seed_row(2, 20, Some("30;40"))]).unwrap();
        assert!(
            m.take_ready(10, i64::MAX / 2).unwrap().iter().all(|r| r.match_id.0 != 2),
            "an already-enriched expired row is not re-flipped"
        );
    }

    // --- the SAME policy, through the real SQLite backend ---------------------------------
    //
    // The fake above proves the POLICY; these prove `SqliteManifest`'s hand-written SQL (which
    // duplicates `retry_ready`'s formula as a `WHERE` clause — see `take_ready`'s doc comment)
    // actually agrees with it, using real rusqlite (no mocking, no live network).

    fn tmp_manifest(name: &str) -> (std::path::PathBuf, SqliteManifest) {
        let path = std::env::temp_dir()
            .join(format!("fetch_manifest_{name}_{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let manifest = SqliteManifest::open(path.to_str().unwrap()).unwrap();
        (path, manifest)
    }

    #[test]
    fn sqlite_manifest_seeds_dedups_and_orders_take_ready_newest_first() {
        let (path, mut m) = tmp_manifest("basic");
        assert_eq!(
            m.seed(&[seed_row(1, 10, None), seed_row(2, 30, None), seed_row(3, 20, None)])
                .unwrap(),
            3
        );
        // Re-seeding known ids inserts nothing new (dedup).
        assert_eq!(m.seed(&[seed_row(1, 10, None)]).unwrap(), 0);
        let ready = m.take_ready(10, 1_000).unwrap();
        assert_eq!(
            ready.iter().map(|r| r.match_id.0).collect::<Vec<_>>(),
            vec![2, 3, 1],
            "newest played_at first"
        );
        let top1 = m.take_ready(1, 1_000).unwrap();
        assert_eq!(top1.len(), 1);
        assert_eq!(top1[0].match_id.0, 2);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sqlite_manifest_backoff_window_matches_the_pure_retry_ready_boundary() {
        let (path, mut m) = tmp_manifest("backoff");
        m.seed(&[seed_row(1, 10, None)]).unwrap();
        m.record(&[StatusUpdate::new(MatchId(1), MatchStatus::Error)])
            .unwrap();
        let updated_at = SqliteManifest::now();
        // attempts=1 -> backoff = 60 << 1 = 120s, per `retry_ready`'s own boundary test — the SQL
        // `WHERE` clause must land on the exact same boundary.
        assert!(
            m.take_ready(10, updated_at + 119).unwrap().is_empty(),
            "SQL must agree with retry_ready: 119s < 120s window"
        );
        assert_eq!(
            m.take_ready(10, updated_at + 120).unwrap().len(),
            1,
            "SQL must agree with retry_ready: 120s reaches the window"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sqlite_manifest_never_takes_terminal_statuses_or_abandoned_matches() {
        let (path, mut m) = tmp_manifest("terminal");
        m.seed(&[
            seed_row(1, 10, None),
            seed_row(2, 20, None),
            seed_row(3, 30, None),
        ])
        .unwrap();
        m.record(&[
            StatusUpdate::new(MatchId(1), MatchStatus::Parsed),
            StatusUpdate::new(MatchId(2), MatchStatus::NoReplay),
            StatusUpdate::new(MatchId(3), MatchStatus::Expired),
        ])
        .unwrap();
        assert!(m.take_ready(10, i64::MAX / 2).unwrap().is_empty());

        // An `error` row past the attempt cap is likewise never handed out again, however long.
        m.seed(&[seed_row(4, 40, None)]).unwrap();
        for _ in 0..MAX_ATTEMPTS {
            m.record(&[StatusUpdate::new(MatchId(4), MatchStatus::Error)])
                .unwrap();
        }
        assert!(m.take_ready(10, i64::MAX / 2).unwrap().is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sqlite_manifest_counts_group_by_status_and_reject_unknown_tokens() {
        let (path, mut m) = tmp_manifest("counts");
        m.seed(&[seed_row(1, 10, None), seed_row(2, 20, None)])
            .unwrap();
        m.record(&[StatusUpdate::new(MatchId(1), MatchStatus::Parsed)])
            .unwrap();
        let counts = m.counts().unwrap();
        assert!(counts.contains(&(MatchStatus::Parsed, 1)));
        assert!(counts.contains(&(MatchStatus::Pending, 1)));

        // A status token outside the closed vocabulary (simulating a corrupt/foreign-written
        // manifest) must fail loud, never silently drop the row or panic.
        m.con
            .execute(
                "UPDATE matches SET status = 'bogus' WHERE match_id = 1",
                [],
            )
            .unwrap();
        assert!(m.counts().is_err(), "an unrecognized status token must fail loud");
        let _ = std::fs::remove_file(&path);
    }
}
