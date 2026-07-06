//! `ingest` — idempotent binary-`COPY` bulk loader: typed replay row batches into the AOE2
//! guide's live PostgreSQL schema (the SeaORM migrations in `migration`, applied separately —
//! this crate never touches DDL).
//!
//! This is the write side only. Producers (the replay parser, the aoestats ETL) build a
//! [`ReplayBatch`] from their own data and hand it to [`ingest_batch`]; this crate owns the
//! DB-write machinery (staging-table `COPY` + set-based, idempotent INSERTs) and nothing else —
//! no SeaORM entities, no query/aggregation layer (that is dbt/SQL, a later milestone).

mod dto;
mod ingest;

pub use dto::{
    IngestStats, Ladder, MatchSource, NewMatch, NewMatchPlayer, NewMatchPlayerTech,
    NewMatchPlayerUnit, NewReplayAge, NewReplayEvent, ReplayBatch,
};
pub use ingest::ingest_batch;
