//! `fetch` — the AOE2 pipeline's replay-fetch ADAPTER: Relic discovery + rate-limited download +
//! a resumable SQLite manifest.
//!
//! An adapter (all IO — tokio/reqwest/rusqlite), the imperative-shell counterpart to the pure
//! `replay` decoder. It produces the raw `.aoe2record` bytes `replay` parses and the
//! [`DiscoverySeed`]s carrying the `source`/`ladder`/`played_at` facts a replay can't; 4d wires the
//! two into `ingest`. This crate depends on neither `replay` nor `ingest` — only `pipeline-core`
//! for the id newtypes.
//!
//! SCOPE: the reusable BUILDING BLOCKS (rate-limited client, discovery, download, resumable
//! manifest + scheduling policy). The 24/7 orchestration that DRIVES them — global circuit-breaker
//! on sustained 429s, adaptive self-throttle, the partitioned now→old backfill loop — is M6
//! (Dagster). Faithful async re-port of `scripts/data-pipeline/replay-rs/src/{api,seed,manifest}.rs`
//! (untouched — strangler rule).

mod api;
mod client;
mod config;
mod error;
mod manifest;
mod seed;

pub use api::{
    best_file, discover_recent, download_archive_replay, download_replay, get_replay_files,
    ArchiveFetch, ReplayFile,
};
pub use client::FetchClient;
pub use error::{Error, Result};
pub use manifest::{
    retry_ready, MatchStatus, PendingMatch, ReplayManifest, SeedRow, SqliteManifest, StatusUpdate,
    UnknownStatus, BACKOFF_BASE_SECS, BACKOFF_CAP_SHIFT, MAX_ATTEMPTS,
};
pub use seed::{DiscoverySeed, RelicMatchType, UnknownMatchType};
