//! Shared binary bootstrap: `tracing` init, `DATABASE_URL` read, and the terminal-error handler.
//!
//! `init_tracing` + read-`DATABASE_URL`-or-exit + `redact_secret`-then-log-then-exit was
//! copy-pasted between `migration/src/main.rs` and `ingest/src/main.rs`, diverging only in the
//! log-filter string — and every future pipeline binary (`replay`, `fetch`, `export`) wants the
//! exact same shape. Hoisting it here means it can only diverge in one place. (5b rewires
//! migration/ingest onto this module; this task only adds it — their `main.rs` are untouched.)
//!
//! This is a shared-shell helper, not a framework: `core` gains `anyhow`/`tracing`/
//! `tracing-subscriber` dependencies for it, but the rest of the crate (`ids`, `elo`, `slug`,
//! `civs`, `maps`, `redact`) stays pure and IO-free.

use tracing_subscriber::EnvFilter;

use crate::redact::redact_secret;
use crate::secret::Secret;

/// Initializes the global `tracing` subscriber: structured output to stderr, filtered by
/// `RUST_LOG` (falling back to `default_filter` when `RUST_LOG` is unset or invalid).
pub fn init_tracing(default_filter: &str) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .init();
}

/// Reads `DATABASE_URL` from the environment, wrapped in a [`Secret`] so it can't be accidentally
/// logged downstream. Fails loud — `bail!`s, never defaults to a placeholder connection string —
/// when the variable is unset or blank.
pub fn database_url() -> anyhow::Result<Secret> {
    match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => Ok(Secret::new(url)),
        _ => anyhow::bail!("DATABASE_URL is not set"),
    }
}

/// Renders a binary's terminal error for logging: redacts `url` out of `err`'s `{:#}` rendering,
/// emits it via `tracing::error!`, and returns the process exit code the caller should use.
pub fn log_error_and_code(err: &anyhow::Error, url: &Secret) -> i32 {
    let message = redact_secret(&format!("{err:#}"), url.expose());
    tracing::error!(error = %message, "pipeline command failed");
    1
}
