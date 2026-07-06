use sea_orm_migration::prelude::*;

/// `match_ages` — one row per (match, player, age) age-up summary parsed from aoestats'
/// `replay_summary_raw` blob (Task M4b: `pipeline/crates/aoestats`'s `import_ages`). Same shape
/// and posture as `replay_ages` (see `m20260705_000009_create_replay_ages.rs`): no primary key, no
/// foreign keys, not partitioned yet. Deliberately a SEPARATE table from `replay_ages` rather than
/// a shared one — the two are populated by different producers (`matches.source` distinguishes
/// them: `'aoestats'` vs `'replay'`) and a later milestone (M5) `UNION`s them for querying, instead
/// of this migration trying to merge the two source shapes upfront.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("match_ages"))
                    .col(ColumnDef::new(Alias::new("match_id")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("profile_id")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("civ_id")).integer().not_null())
                    .col(ColumnDef::new(Alias::new("won")).boolean())
                    .col(ColumnDef::new(Alias::new("age")).text().not_null())
                    .col(ColumnDef::new(Alias::new("uptime_ms")).integer().not_null())
                    .col(ColumnDef::new(Alias::new("villagers")).integer())
                    .col(ColumnDef::new(Alias::new("military")).integer())
                    .col(ColumnDef::new(Alias::new("n_buildings")).integer())
                    .col(ColumnDef::new(Alias::new("n_research")).integer())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Alias::new("match_ages")).to_owned())
            .await
    }
}
