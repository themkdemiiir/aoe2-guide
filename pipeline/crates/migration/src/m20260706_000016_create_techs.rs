use sea_orm_migration::prelude::*;

/// `techs` — dimension table: game `technology_type` `tech_id`s, their English display name, and
/// (for debugging/provenance only) the aoe2techtree `internal_name`. Source-derived from
/// `pipeline_core::techs` (`src/data/tech-names.json`, regenerated via
/// `pnpm build:unit-tech-names` — never hand-edited).
///
/// Exists to put a REAL foreign key on `match_player_techs.tech_id` (added in a following
/// migration) — hard-constraining that column the way `civs`/`maps` already hard-constrain
/// `match_players.civ_id`/`matches.map_id`.
///
/// **Forward-compat note:** unlike `units` (`m20260706_000015_create_units.rs`), this dim can
/// never be the cause of a FK violation from normal ingest: `replay::derive::player_techs` only ever
/// emits `replay::config::WATCHED_TECHS`'s 10 ids (Loom, Wheelbarrow, Hand Cart, Double-Bit Axe,
/// Bow Saw, Horse Collar, Heavy Plow, Gold Mining, Gold Shaft Mining, Stone Mining), all of which
/// resolve here (see `pipeline_core::techs`'s test `watched_techs_all_resolve_to_a_name`). A FK
/// violation on `match_player_techs.tech_id` would mean `WATCHED_TECHS` and this dim drifted
/// apart — a bug to fix in code, not a new-DLC id to backfill.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("techs"))
                    .col(
                        ColumnDef::new(Alias::new("tech_id"))
                            .integer()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Alias::new("name")).text().not_null())
                    .col(ColumnDef::new(Alias::new("internal_name")).text())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Alias::new("techs")).to_owned())
            .await
    }
}
