{{ config(materialized='view') }}

-- Global (not per-civ) match counts by build, 1v1-ladder only — feeds civ-meta.json's top-level
-- `patches` list. Mirrors `aggregate-patches.mjs`'s per-BUILD axis, which is explicitly RM 1v1
-- only (see that script's own final log line: "ranked RM 1v1"). Named `patch_axis` (not
-- `patches`) to keep it visually distinct from the unrelated `patch_index` dimension table.
-- pipeline/crates/export applies MIN_PATCH_TOTAL_MATCHES/MAX_PATCHES and synthesizes the
-- `"Update {build}"` label itself.

select
    build::text as build,
    count(*) as matches
from matches
where ladder = '1v1'
  and build is not null
group by 1
