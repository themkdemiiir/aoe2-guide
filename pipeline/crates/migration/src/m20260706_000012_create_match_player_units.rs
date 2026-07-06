use sea_orm_migration::prelude::*;

/// `match_player_units` — Phase B of the replay-analytics enrichment (`task-enrichB`): one row
/// per (match, player, DISTINCT unit_id) trained-unit total, derived from a replay's own `train`
/// events (`replay::derive::player_units`). **Replay-source ONLY** — aoestats' archive gives no
/// per-unit breakdown, so this table is never populated from that path; it scales with replay
/// count, not the much larger aoestats match volume, and stays small.
///
/// `trained` is Σ `amount` over that player's `train` commands for `unit_id` — units QUEUED, not
/// surviving army (the replay format has no deaths/losses, so survivorship is honestly
/// unknowable from this data alone; see `replay::derive`'s module doc for why the column is named
/// `trained`, never `count`/`built`/`army`). Includes every trained unit_id, eco and military
/// alike — a later consumer (the Phase E winner-comps exporter) can split by
/// `pipeline_core::config::is_eco_unit` on read; this table doesn't pre-filter.
///
/// Same shape/posture as `replay_ages`/`match_ages` (see `m20260705_000009_create_replay_ages.rs`):
/// no primary key, no foreign keys, not partitioned yet. Deliberately no `units` dimension table
/// either — `unit_id` is a plain integer here (Phase E maps ids to names, not this migration).
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("match_player_units"))
                    .col(
                        ColumnDef::new(Alias::new("match_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("profile_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("unit_id")).integer().not_null())
                    .col(ColumnDef::new(Alias::new("trained")).integer().not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("match_player_units"))
                    .to_owned(),
            )
            .await
    }
}
