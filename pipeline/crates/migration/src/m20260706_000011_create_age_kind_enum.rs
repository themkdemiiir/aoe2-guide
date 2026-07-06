use sea_orm_migration::prelude::*;

/// The live DB's `benchmark_vils` view (`pipeline/dbt/models/benchmark_vils.sql`, materialized by
/// `dbt run` — NOT created by any SeaORM migration) reads `match_ages.age` via plain `age IN
/// ('dark', 'feudal', 'castle')`/`age = 'castle'` comparisons. Postgres refuses `ALTER COLUMN …
/// TYPE` on a column any view depends on ("cannot alter type of a column used by a view or
/// rule") — discovered live when this migration's `up` was first run against the real DB (not
/// anticipated by the task brief, which only listed `migration`/`aoestats`/`ingest` in scope).
/// The view must be dropped before the `ALTER` and recreated after. Its query is copy-pasted
/// UNCHANGED from the dbt model (verified in a scratch DB: bare string-literal `IN`/`=`
/// comparisons against an enum column resolve the same way they do against `TEXT` — Postgres
/// types the literal from the left-hand operand either way), so this is a lifecycle
/// accommodation, not a semantic change: a later `dbt run` reproduces the identical view, and
/// `down` recreates the SAME query (it's equally valid against the `TEXT` column `down` restores).
const BENCHMARK_VILS_VIEW_SQL: &str = r#"
CREATE VIEW benchmark_vils AS
with valid_ages as (
    select match_id, profile_id, civ_id, won, age, villagers
    from match_ages
    where age in ('dark', 'feudal', 'castle')
      and uptime_ms >= 0
),
per_player as (
    select
        match_id,
        profile_id,
        civ_id,
        bool_or(won) as won,
        sum(villagers) filter (where age in ('dark', 'feudal')) as vils_by_castle,
        bool_or(age = 'castle') as reached_castle,
        count(*) filter (where age in ('dark', 'feudal')) as n_early_windows
    from valid_ages
    group by 1, 2, 3
),
qualifying as (
    select match_id, profile_id, civ_id, vils_by_castle
    from per_player
    where won
      and reached_castle
      and n_early_windows = 2
      and vils_by_castle is not null
),
joined as (
    select
        c.slug as civ_slug,
        mo.slug as map_slug,
        mp.elo_bucket,
        m.ladder::text as mode,
        q.vils_by_castle
    from qualifying q
    inner join matches m on m.match_id = q.match_id
    inner join maps mo on mo.map_id = m.map_id
    inner join civs c on c.civ_id = q.civ_id
    inner join match_players mp on mp.match_id = q.match_id and mp.profile_id = q.profile_id
    where c.civ_id <> 0
      and mp.elo_bucket is not null
)
select
    civ_slug,
    coalesce(map_slug, 'all') as map_slug,
    coalesce(elo_bucket, 'all') as elo_bucket,
    coalesce(mode, 'all') as mode,
    percentile_cont(0.5) within group (order by vils_by_castle) as vils_median,
    count(*) as n
from joined
group by grouping sets (
    (civ_slug, map_slug, elo_bucket, mode),
    (civ_slug, map_slug, mode),
    (civ_slug, map_slug)
)
having count(*) >= 50
"#;

/// Promotes `replay_ages.age`/`match_ages.age` from plain `TEXT` to a native `age_kind` enum —
/// same raw-SQL convention as `m20260705_000001_create_enums.rs` (`source_kind`/`ladder_kind`/
/// `map_family`): SeaORM has no cleaner enum-define-then-alter flow than raw SQL. Verified against
/// the live DB before this migration was written (60,044,792 `match_ages` rows, all four values;
/// `replay_ages` empty) — see `pipeline_core::age::Age::as_db_str` for the vocabulary source of
/// truth. `('dark', 'feudal', 'castle', 'imperial')` — order is game progression, not alphabetical.
///
/// Also drops/recreates the `benchmark_vils` view around the `ALTER`s — see
/// [`BENCHMARK_VILS_VIEW_SQL`]'s doc for why. `DROP VIEW IF EXISTS` (not a bare `DROP VIEW`) so
/// this migration also applies cleanly on a from-scratch DB that hasn't had `dbt run` yet (the
/// testcontainers suites, and a brand-new deploy before its first dbt run) — there, the view
/// simply doesn't exist yet, so the drop is a no-op and the subsequent `CREATE VIEW` defines it
/// fresh (a later `dbt run` will just recreate the identical view over it either way).
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP VIEW IF EXISTS benchmark_vils")
            .await?;
        db.execute_unprepared(
            "CREATE TYPE age_kind AS ENUM ('dark', 'feudal', 'castle', 'imperial')",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE replay_ages ALTER COLUMN age TYPE age_kind USING age::age_kind",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE match_ages ALTER COLUMN age TYPE age_kind USING age::age_kind",
        )
        .await?;
        db.execute_unprepared(BENCHMARK_VILS_VIEW_SQL).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP VIEW IF EXISTS benchmark_vils")
            .await?;
        db.execute_unprepared(
            "ALTER TABLE match_ages ALTER COLUMN age TYPE TEXT USING age::text",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE replay_ages ALTER COLUMN age TYPE TEXT USING age::text",
        )
        .await?;
        db.execute_unprepared("DROP TYPE age_kind").await?;
        db.execute_unprepared(BENCHMARK_VILS_VIEW_SQL).await?;
        Ok(())
    }
}
