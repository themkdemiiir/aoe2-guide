use sea_orm_migration::prelude::*;

/// Hard-constrains `match_player_units.unit_id -> units(unit_id)` and
/// `match_player_techs.tech_id -> techs(tech_id)` — the same "real FK, not a plain integer
/// column" posture `match_players.civ_id`/`matches.map_id` already have against `civs`/`maps`.
///
/// Added as a standalone migration (not folded into `m20260706_000012_create_match_player_units`/
/// `m20260706_000014_create_match_player_techs`, which predate the `units`/`techs` dims existing)
/// so those two tables' original migrations stay an accurate historical record of what actually
/// ran against them at the time.
///
/// Both `match_player_units`/`match_player_techs` are REPLAY-SOURCE ONLY and, as of this
/// migration, still empty (the replay crawl hasn't run yet) — so adding these constraints is
/// instant and cannot violate on existing rows. See each dim migration's forward-compat note for
/// what happens once rows exist and a future id is missing from its dim (units: possible on a new
/// DLC, fail loud on purpose; techs: should never happen, see that note).
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk-match_player_units-unit_id")
                    .from(Alias::new("match_player_units"), Alias::new("unit_id"))
                    .to(Alias::new("units"), Alias::new("unit_id"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk-match_player_techs-tech_id")
                    .from(Alias::new("match_player_techs"), Alias::new("tech_id"))
                    .to(Alias::new("techs"), Alias::new("tech_id"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name("fk-match_player_techs-tech_id")
                    .table(Alias::new("match_player_techs"))
                    .to_owned(),
            )
            .await?;

        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name("fk-match_player_units-unit_id")
                    .table(Alias::new("match_player_units"))
                    .to_owned(),
            )
            .await
    }
}
