//! Placeholder for the future M6 discover -> download -> parse -> `to_batch` -> ingest run loop
//! (Dagster-driven). Task 4d ships only the pure `to_batch` join (see `lib.rs`) — this binary is a
//! skeleton so the crate compiles as the composition root's eventual entry point, but it does not
//! run anything live: no network, no database, no manifest.

fn main() {
    pipeline_core::cli::init_tracing("info");
    tracing::info!(
        "pipeline binary is a skeleton — the M6 discover/download/parse/ingest run loop is not \
         implemented yet (see src/main.rs)"
    );
}
