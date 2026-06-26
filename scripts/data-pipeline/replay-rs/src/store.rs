//! Gzipped newline-delimited JSON shard writer for the extracted replay rows.
//!
//! Replaces the parquet writer in `store.py`. Each table is appended to a single
//! gzip-compressed NDJSON file under `<out>/` (`*.ndjson.gz`); DuckDB's
//! `read_json_auto` reads `.gz` natively, so downstream is unaffected. Gzip is
//! essential at scale: the events table dominates, and the full 411k-match run
//! would be ~100GB+ raw — gzip cuts the event stream roughly 10x.
//!
//! NDJSON keeps the writer trivial and dependency-light (no polars/arrow). Every
//! table has its own `Mutex<GzEncoder<BufWriter>>` so the rayon workers append
//! concurrently without interleaving partial lines.

use anyhow::Result;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::Serialize;
use std::fs::{create_dir_all, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;

use crate::extract::Extracted;

type GzShard = GzEncoder<BufWriter<std::fs::File>>;

/// One append-only gzipped NDJSON file behind a mutex.
struct Sink {
    writer: Mutex<GzShard>,
}

impl Sink {
    fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        // Each `add()` flushes the gzip stream, so appending to an existing file
        // concatenates independent gzip members — which gzip/zcat/DuckDB read as
        // one logical stream. That keeps the writer resumable across runs.
        let encoder = GzEncoder::new(BufWriter::new(file), Compression::default());
        Ok(Sink {
            writer: Mutex::new(encoder),
        })
    }

    /// Serialise each row as a JSON object on its own line, then compress+write.
    fn write_rows<T: Serialize>(&self, rows: &[T]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut buf = Vec::with_capacity(rows.len() * 128);
        for r in rows {
            serde_json::to_writer(&mut buf, r)?;
            buf.push(b'\n');
        }
        // Recover from a poisoned lock: a worker that panicked mid-parse never
        // wrote a partial line (extract runs before store.add), so the gzip
        // stream is still consistent and it's safe to keep appending.
        let mut w = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        w.write_all(&buf)?;
        Ok(())
    }

    fn flush(&self) -> Result<()> {
        // GzEncoder::flush emits a sync flush, finishing the current member so the
        // bytes on disk are a valid (re-openable) gzip stream after each batch.
        self.writer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .flush()?;
        Ok(())
    }
}

/// The four output shards. Shared (`&self`) across rayon workers.
pub struct Store {
    meta: Sink,
    players: Sink,
    events: Sink,
    ages: Sink,
}

impl Store {
    /// Open (creating the dir) the four `*.ndjson.gz` shard files in `out`.
    pub fn open(out: &Path) -> Result<Self> {
        create_dir_all(out)?;
        Ok(Store {
            meta: Sink::open(&out.join("meta.ndjson.gz"))?,
            players: Sink::open(&out.join("players.ndjson.gz"))?,
            events: Sink::open(&out.join("events.ndjson.gz"))?,
            ages: Sink::open(&out.join("ages.ndjson.gz"))?,
        })
    }

    /// Append one match's four tables. Called from worker threads.
    pub fn add(&self, data: &Extracted) -> Result<()> {
        self.meta.write_rows(std::slice::from_ref(&data.meta))?;
        self.players.write_rows(&data.players)?;
        self.events.write_rows(&data.events)?;
        self.ages.write_rows(&data.ages)?;
        Ok(())
    }

    pub fn flush(&self) -> Result<()> {
        self.meta.flush()?;
        self.players.flush()?;
        self.events.flush()?;
        self.ages.flush()?;
        Ok(())
    }
}
