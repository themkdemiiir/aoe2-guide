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

/// Columns added after the original schema shipped; applied by `open` when
/// missing so old manifests upgrade in place.
/// `profile_ids`: semicolon-separated participant ids — the archive fallback
/// (api.ageofempires.com) requires a participant profileId per download.
const MIGRATIONS: &[(&str, &str)] = &[("profile_ids", "ALTER TABLE matches ADD COLUMN profile_ids TEXT")];

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// A row to seed. New ids start pending; existing ids are left untouched except
/// that a NULL `profile_ids` is enriched when the seed provides one (see `seed`).
pub struct SeedRow {
    pub match_id: i64,
    pub ladder: Option<String>,
    pub played_at: Option<i64>,
    /// Semicolon-separated participant profile ids (archive-fallback key).
    pub profile_ids: Option<String>,
}

/// A pending row handed to the pipeline.
pub struct PendingRow {
    pub match_id: i64,
    #[allow(dead_code)] // carried for parity with the seed columns
    pub ladder: Option<String>,
    #[allow(dead_code)]
    pub played_at: Option<i64>,
    pub profile_ids: Option<String>,
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
        // In-place upgrade of pre-migration manifests.
        let existing: std::collections::HashSet<String> = {
            let mut stmt = con.prepare("SELECT name FROM pragma_table_info('matches')")?;
            let cols = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<_>>()?;
            cols
        };
        for (col, ddl) in MIGRATIONS {
            if !existing.contains(*col) {
                con.execute_batch(ddl)?;
            }
        }
        Ok(Manifest { con })
    }

    /// Insert seed rows. New ids start pending; existing ids keep their status
    /// and data, with two deliberate exceptions when the seed carries
    /// `profile_ids` for a row that had none:
    ///   1. the NULL `profile_ids` is filled in (pure enrichment), and
    ///   2. an `expired` row flips back to `pending` — expiry only said the
    ///      fast path (getReplayFiles) aged out, and profile_ids just opened the
    ///      archive-fallback avenue. Self-limiting: once the archive also fails,
    ///      the row re-expires WITH profile_ids set, so later seeds never flip
    ///      it again.
    pub fn seed(&mut self, rows: &[SeedRow]) -> Result<usize> {
        let now = now();
        let tx = self.con.transaction()?;
        let mut inserted = 0usize;
        {
            let mut ins = tx.prepare(
                "INSERT OR IGNORE INTO matches(match_id, ladder, played_at, profile_ids, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
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
                    r.match_id,
                    r.ladder,
                    r.played_at,
                    r.profile_ids,
                    now
                ])?;
                inserted += n;
                if n == 0 {
                    if let Some(pids) = &r.profile_ids {
                        enrich.execute(rusqlite::params![r.match_id, pids, now])?;
                    }
                }
            }
        }
        tx.commit()?;
        Ok(inserted)
    }

    /// Up to `limit` rows in a status, newest first.
    pub fn take(&self, status: &str, limit: usize) -> Result<Vec<PendingRow>> {
        let mut stmt = self.con.prepare(
            "SELECT match_id, ladder, played_at, profile_ids FROM matches
             WHERE status = ?1 ORDER BY played_at DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![status, limit as i64], |r| {
                Ok(PendingRow {
                    match_id: r.get(0)?,
                    ladder: r.get(1)?,
                    played_at: r.get(2)?,
                    profile_ids: r.get(3)?,
                })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_manifest(name: &str) -> (String, Manifest) {
        let path = std::env::temp_dir()
            .join(format!("replay_rs_manifest_{name}_{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let p = path.to_str().unwrap().to_string();
        let m = Manifest::open(&p).unwrap();
        (p, m)
    }

    fn row(id: i64, pids: Option<&str>) -> SeedRow {
        SeedRow {
            match_id: id,
            ladder: None,
            played_at: Some(id), // distinct timestamps keep take() order deterministic
            profile_ids: pids.map(str::to_string),
        }
    }

    #[test]
    fn open_migrates_a_pre_profile_ids_manifest() {
        let path = std::env::temp_dir()
            .join(format!("replay_rs_manifest_old_{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        // Original schema, without the profile_ids column.
        let con = Connection::open(&path).unwrap();
        con.execute_batch(
            "CREATE TABLE matches (
               match_id INTEGER PRIMARY KEY, ladder TEXT, played_at INTEGER,
               status TEXT NOT NULL DEFAULT 'pending', attempts INTEGER NOT NULL DEFAULT 0,
               n_files INTEGER, best_size INTEGER, best_pid INTEGER, events INTEGER,
               error TEXT, updated_at INTEGER);
             INSERT INTO matches(match_id, status) VALUES (7, 'expired');",
        )
        .unwrap();
        drop(con);

        let mut m = Manifest::open(path.to_str().unwrap()).unwrap();
        // Migration added the column; enrichment now works on the old row.
        m.seed(&[row(7, Some("11;22"))]).unwrap();
        let pending = m.take("pending", 10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].profile_ids.as_deref(), Some("11;22"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn seed_enriches_once_and_flips_expired_exactly_once() {
        let (path, mut m) = tmp_manifest("flip");
        // New row seeds pending with pids.
        assert_eq!(m.seed(&[row(1, Some("10;20"))]).unwrap(), 1);
        // Existing row without pids: expired -> pending + enriched.
        m.seed(&[row(2, None)]).unwrap();
        m.update_many(&[Update::new(2, "expired")]).unwrap();
        m.seed(&[row(2, Some("30;40"))]).unwrap();
        let pending = m.take("pending", 10).unwrap();
        assert_eq!(pending.len(), 2);
        let p2 = pending.iter().find(|r| r.match_id == 2).unwrap();
        assert_eq!(p2.profile_ids.as_deref(), Some("30;40"));
        // Archive also failed -> expired WITH pids; a re-seed must NOT flip again.
        m.update_many(&[Update::new(2, "expired")]).unwrap();
        m.seed(&[row(2, Some("30;40"))]).unwrap();
        assert!(m.take("pending", 10).unwrap().iter().all(|r| r.match_id != 2));
        // And a parsed row is never touched by enrichment status-wise.
        m.update_many(&[Update::new(1, "parsed")]).unwrap();
        m.seed(&[row(1, Some("99"))]).unwrap();
        let counts = m.counts().unwrap();
        assert!(counts.iter().any(|(s, n)| s == "parsed" && *n == 1));
        let _ = std::fs::remove_file(&path);
    }
}
