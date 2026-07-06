//! SeaORM migrations for the AOE2 guide's gameplay-first, normalized PostgreSQL schema:
//! dimension tables (`maps`, `civs`, `civs_relic`, `patch_index`, `units`, `techs`) and fact
//! tables (`matches`, `match_players`, `replay_events`, `replay_ages`, `match_ages`,
//! `match_player_units`, `match_player_techs`). See each migration module for the exact DDL. Do
//! NOT partition any table here — that is a separate step before the 100M-row replay-event load.

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
mod m20260706_000012_create_match_player_units;
mod m20260706_000013_add_match_players_apm;
mod m20260706_000014_create_match_player_techs;
mod m20260706_000015_create_units;
mod m20260706_000016_create_techs;
mod m20260706_000017_add_units_techs_fks;
mod m20260706_000018_add_opening_kind;

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
            Box::new(m20260706_000012_create_match_player_units::Migration),
            Box::new(m20260706_000013_add_match_players_apm::Migration),
            Box::new(m20260706_000014_create_match_player_techs::Migration),
            Box::new(m20260706_000015_create_units::Migration),
            Box::new(m20260706_000016_create_techs::Migration),
            Box::new(m20260706_000017_add_units_techs_fks::Migration),
            Box::new(m20260706_000018_add_opening_kind::Migration),
        ]
    }
}
