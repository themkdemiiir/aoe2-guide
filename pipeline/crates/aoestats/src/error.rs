//! One `thiserror` enum for the crate's library edge (`pipeline/docs/rust-playbook.md`'s "thiserror
//! at library edges" rule). The two variants that exist purely to catch schema drift
//! (`MissingColumn`/`UnexpectedColumnType`) are deliberate paranoia: the column layout below was
//! verified against a real sample (`m_2022-08-28.parquet`/`p_2022-08-28.parquet`), but the
//! archive spans 172 files scraped over years — a later file with a renamed or differently-typed
//! column must fail loud, never silently misread a column as something it isn't (see
//! `parquet_read`'s module doc for the `duration` column's own schema surprise).

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AoestatsError {
    #[error("failed to open parquet file {path}")]
    Open {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read parquet metadata/schema from {path}")]
    Parquet {
        path: PathBuf,
        #[source]
        source: parquet::errors::ParquetError,
    },

    #[error("{path} is missing expected column '{column}' — aoestats schema drift from the verified layout")]
    MissingColumn { path: PathBuf, column: &'static str },

    #[error(
        "{path} column '{column}' has an unexpected arrow type ({actual}) — aoestats schema drift from the verified layout"
    )]
    UnexpectedColumnType {
        path: PathBuf,
        column: &'static str,
        actual: String,
    },

    #[error("database error")]
    Db(#[from] tokio_postgres::Error),

    // --- Task M4b: the `python3 aoestats_summaries.py` subprocess (`crate::py`). ---
    /// The one required external tool for the ages-import path (`import_ages`) — see the crate's
    /// module doc. Deliberately its own variant (not folded into `PythonIo`) so callers/logs can
    /// tell "not installed" apart from a transient pipe/IO failure.
    #[error(
        "python3 not found on PATH — it's required to import aoestats ages (parses \
         `replay_summary_raw`'s Python-repr blob; see pipeline/py/aoestats_summaries.py)"
    )]
    PythonNotFound,

    #[error("failed to set up a stdin/stdout/stderr pipe for the python3 subprocess")]
    PythonPipeSetup,

    #[error("io error talking to the python3 subprocess")]
    PythonIo(#[source] std::io::Error),

    #[error("the python3 stdin-writer thread panicked")]
    PythonStdinThreadPanicked,

    #[error("python3 exited with {status} running aoestats_summaries.py — stderr: {stderr}")]
    PythonExited {
        status: std::process::ExitStatus,
        stderr: String,
    },

    #[error("failed to parse a line of aoestats_summaries.py's NDJSON output: {line:?}")]
    PythonOutputParse {
        line: String,
        #[source]
        source: serde_json::Error,
    },

    /// An `AgeSourceRow` failed to serialize to NDJSON before it could even be sent to python —
    /// only possible for a non-finite `profile_id` (`NaN`/`inf`), which JSON has no
    /// representation for.
    #[error("failed to serialize an ages-import input row to NDJSON (game_id={game_id:?})")]
    PythonInputSerialize {
        game_id: Option<String>,
        #[source]
        source: serde_json::Error,
    },

    /// `aoestats_summaries.py` emitted an `age` value outside the closed
    /// `dark`/`feudal`/`castle`/`imperial` vocabulary — a data-corruption signal from the python
    /// step (see `pipeline_core::age`'s module doc), not a per-row skip.
    #[error("aoestats_summaries.py emitted an unexpected age value: {0}")]
    AgeValidation(#[from] pipeline_core::age::UnknownAge),

    #[error("the blocking python subprocess task panicked or was cancelled")]
    BlockingTask(#[from] tokio::task::JoinError),
}

pub type Result<T> = std::result::Result<T, AoestatsError>;
