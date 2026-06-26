//! Resumable job manifest (SQLite, WAL).
//!
//! Direct port of `scripts/data-pipeline/replays/manifest.py`. One row per
//! `match_id` records exactly where each replay stands: pending, parsed,
//! no_replay, expired, parse_failed, or error. The pipeline is fully resumable
//! and idempotent because every decision is recorded here, not in memory.
//!
//! SQLite (rather than DuckDB) is used because the manifest takes many small
//! status updates, which SQLite handles cleanly. WAL mode keeps reads and the
//! single writer from blocking each other.

use anyhow::Result;
use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS matches (
  match_id   INTEGER PRIMARY KEY,
  ladder     TEXT,
  played_at  INTEGER,
  status     TEXT NOT NULL DEFAULT 'pending',
  attempts   INTEGER NOT NULL DEFAULT 0,
  n_files    INTEGER,
  best_size  INTEGER,
  best_pid   INTEGER,
  events     INTEGER,        -- # events stored (sanity / progress)
  error      TEXT,
  updated_at INTEGER
);
CREATE INDEX IF NOT EXISTS idx_matches_status ON matches(status);
";

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// A row to seed (existing ids are left untouched — resumable).
pub struct SeedRow {
    pub match_id: i64,
    pub ladder: Option<String>,
    pub played_at: Option<i64>,
}

/// A status update for one match. Only `match_id` + `status` are required; the
/// rest mirror the optional columns the Python `update_many` accepts.
#[derive(Default)]
pub struct Update {
    pub match_id: i64,
    pub status: String,
    pub attempts_inc: bool,
    pub n_files: Option<i64>,
    pub best_size: Option<i64>,
    pub best_pid: Option<i64>,
    pub events: Option<i64>,
    pub error: Option<String>,
}

impl Update {
    pub fn new(match_id: i64, status: &str) -> Self {
        Update {
            match_id,
            status: status.to_string(),
            attempts_inc: true,
            ..Default::default()
        }
    }
}

pub struct Manifest {
    con: Connection,
}

impl Manifest {
    pub fn open(path: &str) -> Result<Self> {
        let con = Connection::open(path)?;
        con.pragma_update(None, "journal_mode", "WAL")?;
        con.pragma_update(None, "synchronous", "NORMAL")?;
        con.execute_batch(SCHEMA)?;
        Ok(Manifest { con })
    }

    /// Insert seed rows. New ids start pending; existing ids are ignored.
    pub fn seed(&mut self, rows: &[SeedRow]) -> Result<usize> {
        let now = now();
        let tx = self.con.transaction()?;
        let mut inserted = 0usize;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO matches(match_id, ladder, played_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for r in rows {
                inserted += stmt.execute(rusqlite::params![
                    r.match_id,
                    r.ladder,
                    r.played_at,
                    now
                ])?;
            }
        }
        tx.commit()?;
        Ok(inserted)
    }

    /// Up to `limit` (match_id, ladder, played_at) rows in a status, newest first.
    pub fn take(
        &self,
        status: &str,
        limit: usize,
    ) -> Result<Vec<(i64, Option<String>, Option<i64>)>> {
        let mut stmt = self.con.prepare(
            "SELECT match_id, ladder, played_at FROM matches
             WHERE status = ?1 ORDER BY played_at DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![status, limit as i64], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Apply a batch of updates in a single transaction.
    pub fn update_many(&mut self, updates: &[Update]) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }
        let now = now();
        let tx = self.con.transaction()?;
        for u in updates {
            // Build the SET clause dynamically so we only touch supplied columns,
            // mirroring manifest.py's update_many.
            let mut sets: Vec<String> = vec!["status = ?".into(), "updated_at = ?".into()];
            let mut vals: Vec<Box<dyn rusqlite::types::ToSql>> =
                vec![Box::new(u.status.clone()), Box::new(now)];
            if let Some(v) = u.n_files {
                sets.push("n_files = ?".into());
                vals.push(Box::new(v));
            }
            if let Some(v) = u.best_size {
                sets.push("best_size = ?".into());
                vals.push(Box::new(v));
            }
            if let Some(v) = u.best_pid {
                sets.push("best_pid = ?".into());
                vals.push(Box::new(v));
            }
            if let Some(v) = u.events {
                sets.push("events = ?".into());
                vals.push(Box::new(v));
            }
            if let Some(v) = &u.error {
                sets.push("error = ?".into());
                vals.push(Box::new(v.clone()));
            }
            if u.attempts_inc {
                sets.push("attempts = attempts + 1".into());
            }
            vals.push(Box::new(u.match_id));
            let sql = format!(
                "UPDATE matches SET {} WHERE match_id = ?",
                sets.join(", ")
            );
            let params: Vec<&dyn rusqlite::types::ToSql> =
                vals.iter().map(|b| b.as_ref()).collect();
            tx.execute(&sql, params.as_slice())?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Reset non-terminal statuses left by a crashed/partial prior run back to
    /// `pending` so they're retried. `downloadable` is a transient pre-parse
    /// state that should never persist; `error` is the retryable failure status.
    /// Terminal statuses (parsed / parse_failed / no_replay / expired) are left
    /// untouched, so completed work is never redone. Returns rows reset.
    pub fn reset_inflight(&mut self) -> Result<usize> {
        let now = now();
        let n = self.con.execute(
            "UPDATE matches SET status = 'pending', updated_at = ?1
             WHERE status IN ('downloadable', 'error')",
            rusqlite::params![now],
        )?;
        Ok(n)
    }

    /// status -> count.
    pub fn counts(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self
            .con
            .prepare("SELECT status, count(*) FROM matches GROUP BY status ORDER BY status")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Total rows in the manifest. Handy for ad-hoc inspection / tests.
    #[allow(dead_code)]
    pub fn total(&self) -> Result<i64> {
        Ok(self
            .con
            .query_row("SELECT count(*) FROM matches", [], |r| r.get(0))?)
    }
}

/// Pretty status counts, e.g. `{parsed: 12, expired: 3}`.
pub fn fmt_counts(counts: &[(String, i64)]) -> String {
    let inner = counts
        .iter()
        .map(|(s, n)| format!("{s}: {n}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{{inner}}}")
}
