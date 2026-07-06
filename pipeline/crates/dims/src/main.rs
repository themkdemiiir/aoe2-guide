//! Thin CLI over the `dims` library: idempotently loads `maps`/`civs`/`civs_relic`/`patch_index`
//! into Postgres from the committed refdata `pipeline_core` (`refdata` feature) parses. Every
//! `matches` row FK-references `maps`/`civs`, so this MUST run before any replay/aoestats ingest
//! against a fresh schema — see the crate doc.
//!
//! Like `migration`/`ingest`, `DATABASE_URL` is read from the environment (via
//! `pipeline_core::cli::database_url`) and never placed in a clap arg, help string, or log line —
//! that leak already caused a real password rotation once.

use anyhow::Context;
use dims::load_dims;
use pipeline_core::cli::{database_url, init_tracing, log_error_and_code};
use pipeline_core::redact_secret;
use tokio_postgres::NoTls;

/// Default log filter when `RUST_LOG` is unset.
const DEFAULT_LOG_FILTER: &str = "info";

#[tokio::main]
async fn main() {
    init_tracing(DEFAULT_LOG_FILTER);

    let secret = match database_url() {
        Ok(secret) => secret,
        Err(err) => {
            tracing::error!(error = %err, "failed to read DATABASE_URL");
            std::process::exit(1);
        }
    };

    if let Err(err) = run(secret.expose()).await {
        std::process::exit(log_error_and_code(&err, &secret));
    }
}

async fn run(database_url: &str) -> anyhow::Result<()> {
    let (mut client, connection) = tokio_postgres::connect(database_url, NoTls)
        .await
        .context("failed to connect to the database")?;

    // Drive the connection on its own task, as tokio_postgres requires; log (redacted) if it
    // ever fails instead of silently dropping the error.
    let database_url_for_log = database_url.to_owned();
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            let message = redact_secret(&format!("{err:#}"), &database_url_for_log);
            tracing::error!(error = %message, "database connection closed with an error");
        }
    });

    let stats = load_dims(&mut client).await.context("load_dims failed")?;

    tracing::info!(
        maps = stats.maps,
        civs = stats.civs,
        civs_relic = stats.civs_relic,
        patch_index = stats.patch_index,
        "dims load complete"
    );
    Ok(())
}
