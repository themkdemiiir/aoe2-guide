use sea_orm_migration::prelude::*;

/// `match_player_techs` — Phase D of the replay-analytics enrichment (`task-enrichD`): one row
/// per (match, player, watched tech ACTUALLY researched), derived from a replay's own `research`
/// events (`replay::derive::player_techs`). **Replay-source ONLY** — aoestats' archive carries no
/// per-tech-research timings at all, so this table is never populated from that path; it scales
/// with replay count, not the much larger aoestats match volume, and stays small.
///
/// **`t_ms` is the first-research CLICK time (research START) — NOT completion.** This is the
/// OPPOSITE basis from `match_players.feudal_t`/`castle_t`/`imperial_t` (COMPLETION seconds,
/// chosen so those columns pool against aoestats' `*_age_uptime`): tech-research timings have no
/// aoestats equivalent to pool against, so there's no reconciliation need, and click is the
/// honest, directly-recorded value — see `replay::derive`'s module doc ("Tech-timings basis") for
/// the full rationale. **Never conflate the two bases when querying across both tables.**
///
/// Includes only the 10 `replay::config::WATCHED_TECHS` ids (Loom, Wheelbarrow, Hand Cart,
/// Double-Bit Axe, Bow Saw, Horse Collar, Heavy Plow, Gold Mining, Gold Shaft Mining, Stone
/// Mining) — a tech never researched by a player has NO row (honest absence, never a fabricated
/// sentinel time).
///
/// Same shape/posture as `match_player_units` (see
/// `m20260706_000012_create_match_player_units.rs`): no primary key, not partitioned yet.
///
/// `tech_id` gets a real FK to the `techs` dimension table (id -> English name) in
/// `m20260706_000016_create_techs.rs`/`m20260706_000017_add_units_techs_fks.rs` — added later,
/// once that dim existed, rather than retrofitted here.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("match_player_techs"))
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
                    .col(ColumnDef::new(Alias::new("tech_id")).integer().not_null())
                    .col(ColumnDef::new(Alias::new("t_ms")).integer().not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("match_player_techs"))
                    .to_owned(),
            )
            .await
    }
}
