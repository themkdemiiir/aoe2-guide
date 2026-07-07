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
//!
//! It also ships the raw-replay archive ([`raw`]) [`crawl`] writes every downloaded
//! `.aoe2record` into (even ones that fail to parse), and [`reparse`], a `pipeline reparse` entry
//! point that reads that archive back and proves it re-usable — see each module's doc.
//!
//! [`import_shards`] is the `pipeline import-shards` one-time migration of the OLD
//! `scripts/data-pipeline/replay-rs`-extracted shard corpus into this same live schema (also
//! `source='replay'`), reusing [`to_batch`]/`ingest_batch` exactly like [`crawl`] does — see that
//! module's doc for the full design.
//!
//! Finally, [`backfill`] is the `pipeline backfill` ONGOING recent→old enrichment loop: it walks
//! the historical `source='aoestats'` corpus newest-first, downloads each match's real replay from
//! the age archive, and UPGRADES it in place to a rich `source='replay'` row — reusing the SAME
//! [`to_batch`] + [`dimfilter`] + `ingest` spine, but with `ingest::ConflictPolicy::UpgradeAoestats`
//! so the aggregate row is REPLACED, not skipped. See that module's doc for the resumable-from-DB
//! design and the archive's rolling-retention reality.

mod backfill;
mod compose;
mod crawl;
mod dimfilter;
mod error;
mod import_shards;
mod raw;
mod reparse;
mod sink;
mod source;

pub use backfill::{backfill, BackfillConfig, BackfillSummary};
pub use compose::to_batch;
pub use crawl::{crawl, CrawlConfig, CrawlError, CrawlSummary};
pub use error::{Error, Result};
pub use import_shards::{import_shards, ImportShardsConfig, ImportShardsSummary};
pub use raw::{RawArchive, SaveOutcome};
pub use reparse::{reparse_dir, ReparseSummary};
pub use sink::{IngestSink, PgSink};
pub use source::{FetchSource, ReplayFetch, ReplaySource};
