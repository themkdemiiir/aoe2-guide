//! SeaORM migrations for the AOE2 guide's gameplay-first, normalized PostgreSQL schema:
//! dimension tables (`maps`, `civs`, `civs_relic`, `patch_index`) and fact tables
//! (`matches`, `match_players`, `replay_events`, `replay_ages`, `match_ages`). See each migration
//! module for the exact DDL. Do NOT partition any table here — that is a separate step before the
//! 100M-row replay-event load.

pub use sea_orm_migration::prelude::*;

mod m20260705_000001_create_enums;
mod m20260705_000002_create_maps;
mod m20260705_000003_create_civs;
mod m20260705_000004_create_civs_relic;
mod m20260705_000005_create_patch_index;
mod m20260705_000006_create_matches;
mod m20260705_000007_create_match_players;
mod m20260705_000008_create_replay_events;
mod m20260705_000009_create_replay_ages;
mod m20260706_000010_create_match_ages;
mod m20260706_000011_create_age_kind_enum;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260705_000001_create_enums::Migration),
            Box::new(m20260705_000002_create_maps::Migration),
            Box::new(m20260705_000003_create_civs::Migration),
            Box::new(m20260705_000004_create_civs_relic::Migration),
            Box::new(m20260705_000005_create_patch_index::Migration),
            Box::new(m20260705_000006_create_matches::Migration),
            Box::new(m20260705_000007_create_match_players::Migration),
            Box::new(m20260705_000008_create_replay_events::Migration),
            Box::new(m20260705_000009_create_replay_ages::Migration),
            Box::new(m20260706_000010_create_match_ages::Migration),
            Box::new(m20260706_000011_create_age_kind_enum::Migration),
        ]
    }
}
