//! `pipeline-core` — the shared, typed, regex-free data core for the AOE2 guide's data pipeline.
//!
//! Each module below is the single canonical Rust implementation of a transform that today lives
//! duplicated between JS (`scripts/data-pipeline/lib/buckets.mjs`, `lib/relic-map.mjs`) and the
//! existing `replay-rs` crate (`src/analyze/{maps.rs,data.rs}`). This crate does not replace either
//! of those yet — it stands alone until a later milestone cuts callers over — but every function
//! here is verified byte-for-byte against the behavior it will eventually replace (see each
//! module's tests).

// `civs`, `maps`, `patch`, `units`, and `techs` `include_str!` repo-root data files at build time,
// so they compile only under the `refdata` feature (opt-in; off for lean `pipeline/`-context
// Docker builds — see Cargo.toml). `age`, `ids`, `elo`, `secret`, `cli`, `slug`, and `redact` are
// file-free and always compile.
pub mod age;
#[cfg(feature = "refdata")]
pub mod civs;
pub mod cli;
pub mod elo;
pub mod ids;
#[cfg(feature = "refdata")]
pub mod maps;
#[cfg(feature = "refdata")]
pub mod patch;
pub mod redact;
pub mod secret;
pub mod slug;
#[cfg(feature = "refdata")]
pub mod techs;
#[cfg(feature = "refdata")]
pub mod units;

pub use age::Age;
pub use ids::{GameCivId, GameUnitId, MatchId, ProfileId, RelicCivId, TechId};
pub use redact::redact_secret;
pub use secret::Secret;
