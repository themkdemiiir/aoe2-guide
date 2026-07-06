//! `export` — the read-side mirror of `ingest`/`aoestats`: query the `pipeline/dbt`-modeled
//! aggregation views and write the AOE2 guide's `src/data/*.json` shapes. Task M5a establishes the
//! pattern with ONE exporter, `civ-meta.json` (see `civ_meta.rs`'s doc for the aggregation and
//! `model.rs`'s doc for the exact shape being reproduced); M5b/c replicate the same
//! dbt-view -> typed-query -> pure-assembly -> serde-write pipeline for the rest.
//!
//! **Deliberate deviation from a Rust-side civ_id -> slug lookup:** the `civ_meta` dbt view
//! already resolves `civ_id` to `slug` via a SQL `JOIN` against the live `civs` dim (the same
//! table `pipeline_core`'s `refdata`-gated lookup would otherwise duplicate from a repo-root JSON
//! file). So this crate takes `pipeline-core` with `default-features = false` — no `refdata`, no
//! `include_str!`'d data files, no extra Docker-build surface — mirroring `aoestats`'s own
//! no-Rust-side-refdata posture (see its `Cargo.toml`/`lib.rs` doc for the identical reasoning).

pub mod civ_meta;
pub mod error;
pub mod model;
pub mod query;
pub mod shape;
pub mod stats;

pub use civ_meta::{build_doc, CivMetaInputs};
pub use error::{ExportError, Result};
pub use model::CivMetaDoc;
