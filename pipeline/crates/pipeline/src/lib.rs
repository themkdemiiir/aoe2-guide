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
//! SCOPE: this crate today only ships the pure join. The eventual
//! discover -> download -> parse -> `to_batch` -> ingest RUN LOOP (driven by Dagster) is a later
//! milestone (M6) — `src/main.rs` is a placeholder skeleton for that future binary, not a live
//! loop.

mod compose;
mod error;

pub use compose::to_batch;
pub use error::{Error, Result};
