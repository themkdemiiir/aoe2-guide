//! `pipeline` — the composition root: joins `replay`'s gameplay-only [`replay::ParsedReplay`]
//! with `fetch`'s discovery-seed metadata ([`fetch::DiscoverySeed`]) into the typed DTOs
//! `ingest::ingest_batch` loads.
//!
//! This is the ONE place `replay` + `fetch` + `ingest` compose (see
//! `pipeline/docs/rust-playbook.md`'s "composition root" pattern) — none of those three crates
//! depend on each other or on this one; a replay knows gameplay but not `source`/`ladder`/
//! `played_at`, and a discovery seed knows those but not gameplay. [`to_batch`] is pure (no IO,
//! no `tokio`/`reqwest`/`pg` in this crate's dependency tree beyond what `replay`/`fetch`/
//! `ingest` themselves already pull in for their own adapters).
//!
//! SCOPE: alongside that pure join, this crate now also ships the M6 discover -> download ->
//! parse -> `to_batch` -> ingest RUN LOOP itself ([`crawl`]) — see that module's doc for the full
//! design (resilience, bounded concurrency, graceful shutdown, dry-run). `src/main.rs` is the
//! `pipeline crawl` CLI over it, driven standalone or (eventually) by Dagster.

mod compose;
mod crawl;
mod error;
mod sink;
mod source;

pub use compose::to_batch;
pub use crawl::{crawl, CrawlConfig, CrawlError, CrawlSummary};
pub use error::{Error, Result};
pub use sink::{IngestSink, PgSink};
pub use source::{FetchSource, ReplayFetch, ReplaySource};
