//! [`IngestSink`] — where a composed [`ingest::ReplayBatch`] goes once one match's fetch + parse +
//! [`crate::to_batch`] succeeds. Abstracted like [`crate::source::ReplaySource`] so the crawl
//! loop's own tests (`tests/crawl.rs`) can assert what WOULD have been ingested without a real
//! Postgres — [`PgSink`] is the one production implementation, wrapping `ingest::ingest_batch` on
//! the playbook's single owned `&mut Client` (one batch = one `BEGIN...COMMIT`).
//!
//! [`crate::crawl::crawl`] always `.await`s a sink call directly on its own task (never inside a
//! spawned worker), so — unlike [`crate::source::ReplaySource`] — nothing here strictly NEEDS a
//! `Send` future. It's declared anyway (explicit `impl Future + Send`, not plain `async fn`) to
//! avoid the `async_fn_in_trait` lint on a `pub` trait and to keep the two adapter seams in this
//! crate stylistically consistent.

use std::future::Future;

use ingest::{ingest_batch, IngestStats, ReplayBatch};

/// See the module doc.
pub trait IngestSink {
    /// Load one batch, returning the row counts `ingest_batch` reports (or a batch-load failure —
    /// `anyhow`, per the playbook's "a typed error variant must earn its keep" rule: nothing here
    /// branches on ingest's failure kind. The crawl loop treats any `Err` here the same as a
    /// fetch/parse failure: mark the match `Error` and continue, never crash the loop).
    fn ingest<'a>(
        &'a mut self,
        batch: &'a ReplayBatch,
    ) -> impl Future<Output = anyhow::Result<IngestStats>> + Send + 'a;
}

/// Production [`IngestSink`]: `ingest::ingest_batch` on a borrowed, single-owned
/// `tokio_postgres::Client`.
pub struct PgSink<'a> {
    client: &'a mut tokio_postgres::Client,
}

impl<'a> PgSink<'a> {
    pub fn new(client: &'a mut tokio_postgres::Client) -> Self {
        PgSink { client }
    }
}

impl IngestSink for PgSink<'_> {
    fn ingest<'a>(
        &'a mut self,
        batch: &'a ReplayBatch,
    ) -> impl Future<Output = anyhow::Result<IngestStats>> + Send + 'a {
        ingest_batch(self.client, batch)
    }
}
