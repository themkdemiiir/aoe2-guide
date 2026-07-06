//! One `thiserror` enum for the crate's library edge (`pipeline/docs/rust-playbook.md`'s
//! "thiserror at library edges" rule). Every variant here is something a caller (`main.rs`, or a
//! future orchestrator) genuinely branches on: `Db` vs `Io` vs a malformed row from a view whose
//! own shape drifted from what this crate expects.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("database error")]
    Db(#[from] tokio_postgres::Error),

    #[error("failed to write {path}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to serialize the civ-meta document to JSON")]
    Serialize(#[from] serde_json::Error),

    /// A `civ_meta`/`civ_meta_by_*` row carried an `elo_bucket`/`ladder` value outside the closed
    /// vocabulary this crate expects — the dbt model's own `accepted_values` test should have
    /// caught this first; seeing it here means the view and this crate's assumptions have drifted
    /// apart. Fail loud rather than silently drop the row.
    #[error("unexpected {field} value from the civ_meta view: {value:?}")]
    UnexpectedValue { field: &'static str, value: String },
}

pub type Result<T> = std::result::Result<T, ExportError>;
