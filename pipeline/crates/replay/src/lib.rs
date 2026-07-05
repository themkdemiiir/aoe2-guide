//! `replay` — pure `.aoe2record` -> [`ParsedReplay`] decoder.
//!
//! This is the "functional core" of the ingest side (per `pipeline/docs/rust-playbook.md`):
//! bytes in, typed rows out, zero IO. No `tokio`/`reqwest`/`rusqlite`/`pg` anywhere in this
//! crate's dependency tree — a future `fetch` crate downloads the bytes and hands them to
//! [`parse`]; a future `ingest` adapter turns [`ParsedReplay`] into DB rows. Neither concern
//! lives here.
//!
//! Faithful, parity-checked port of the OLD extractor at
//! `scripts/data-pipeline/replay-rs/src/{extract.rs,postgame.rs,config.rs}` (untouched —
//! strangler rule; see `.superpowers/sdd/task-4b-replay-report.md` for the parity gate).

mod config;
mod error;
mod parse;
mod postgame;
mod types;

pub use error::{Error, Result};
pub use parse::parse;
pub use types::{ParsedReplay, ReplayAge, ReplayEvent, ReplayPlayer};
