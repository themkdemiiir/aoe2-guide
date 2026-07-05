//! `pipeline-core` — the shared, typed, regex-free data core for the AOE2 guide's data pipeline.
//!
//! Each module below is the single canonical Rust implementation of a transform that today lives
//! duplicated between JS (`scripts/data-pipeline/lib/buckets.mjs`, `lib/relic-map.mjs`) and the
//! existing `replay-rs` crate (`src/analyze/{maps.rs,data.rs}`). This crate does not replace either
//! of those yet — it stands alone until a later milestone cuts callers over — but every function
//! here is verified byte-for-byte against the behavior it will eventually replace (see each
//! module's tests).

pub mod civs;
pub mod elo;
pub mod maps;
pub mod slug;
