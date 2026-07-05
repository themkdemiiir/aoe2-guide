//! `fetch::Error` — the closed failure surface of the Relic adapter.
//!
//! Enumerated so the M6 run-loop can BRANCH on failure kind, which is the whole reason this is a
//! `thiserror` enum and not `anyhow` (playbook: "a typed error variant must earn its keep"):
//!   - [`Error::RateLimited`] / [`Error::Http`] / [`Error::Body`] → transient, sleep + retry;
//!   - [`Error::Manifest`] → the local DB is broken, stop the run;
//!   - [`Error::UnknownMatchType`] / [`Error::BadArchive`] / [`Error::BadTimestamp`] → the record
//!     is deterministically bad, skip this match (never silently mislabel — the civ-id-bug lesson).
//!
//! Every variant is `Send + Sync + 'static` (all wrapped errors already are), so a binary can
//! `.context()` these into `anyhow::Result` at its edge.

use thiserror::Error;

use crate::seed::UnknownMatchType;

/// The fetch adapter's result alias.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    /// A 429 wall persisted past the retry middleware. Carries the server's `Retry-After` (secs)
    /// when present so the caller can defer precisely rather than guess.
    #[error("relic api rate-limited (retry-after: {retry_after:?}s)")]
    RateLimited { retry_after: Option<u64> },

    /// The request pipeline (connection, middleware, retry exhaustion) failed.
    #[error("http request failed")]
    Http(#[from] reqwest_middleware::Error),

    /// The request succeeded but reading the response body failed mid-stream.
    #[error("reading response body failed")]
    Body(#[from] reqwest::Error),

    /// A non-429 non-success HTTP status. Carries only the code (never the signed URL, which holds
    /// a short-lived SAS token — see [`crate::client`]).
    #[error("relic api returned http {status} for {endpoint}")]
    Status { status: u16, endpoint: &'static str },

    /// A Relic JSON response didn't match the expected shape.
    #[error("decoding relic api json failed")]
    Json(#[from] serde_json::Error),

    /// Gunzip of a signed replay blob (or inflate of an archive ZIP member) failed.
    #[error("decompressing replay blob failed")]
    Decompress(#[from] std::io::Error),

    /// The age-archive endpoint returned 200 but the ZIP was structurally unusable — a
    /// deterministic failure (retrying only re-burns the archive budget), so terminal.
    #[error("malformed archive payload: {0}")]
    BadArchive(String),

    /// The resumable manifest's SQLite backend failed.
    #[error("manifest db error")]
    Manifest(#[from] rusqlite::Error),

    /// A Relic match's ladder/mode id fell outside the known ranked vocabulary — fail loud rather
    /// than mislabel it (a new DLC ladder id must surface, not silently vanish).
    #[error("unknown relic match type")]
    UnknownMatchType(#[from] UnknownMatchType),

    /// A Relic `completiontime` didn't fit a representable UTC timestamp (corrupt/adversarial).
    #[error("relic completiontime {0} is out of representable range")]
    BadTimestamp(i64),
}
