use sea_orm_migration::prelude::*;

/// `units` — dimension table: game DAT `unit_id`s, their English display name, and (for
/// debugging/provenance only) the aoe2techtree `internal_name`. Source-derived from
/// `pipeline_core::units` (`src/data/unit-names.json`, regenerated via
/// `pnpm build:unit-tech-names` — never hand-edited).
///
/// Exists to put a REAL foreign key on `match_player_units.unit_id` (added in a following
/// migration) — hard-constraining that column the way `civs`/`maps` already hard-constrain
/// `match_players.civ_id`/`matches.map_id`.
///
/// **Forward-compat note:** `unit-names.json` only covers ids resolvable from a BASE-GAME civ's
/// visible tech tree at the pinned aoe2techtree SHA (see that script's module doc) — currently
/// 238 of 245 known DAT unit ids; the other 7 are internal alt-form/legacy ids never reachable by
/// a player's `train` command, so they're never expected here. A future DLC unit id NOT yet in
/// this dim WILL fail the `match_player_units.unit_id` FK the first time a replay trains it —
/// that is the INTENDED fail-loud signal to re-run `pnpm build:unit-tech-names` (which re-syncs
/// against aoe2techtree) and re-populate this dim via `dims::load_dims`, never a silent accept.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("units"))
                    .col(
                        ColumnDef::new(Alias::new("unit_id"))
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
            .drop_table(Table::drop().table(Alias::new("units")).to_owned())
            .await
    }
}
