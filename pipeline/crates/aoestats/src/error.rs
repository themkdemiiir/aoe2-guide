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
}

pub type Result<T> = std::result::Result<T, AoestatsError>;
