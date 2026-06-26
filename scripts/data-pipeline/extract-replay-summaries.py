#!/usr/bin/env python3
# scripts/data-pipeline/extract-replay-summaries.py
#
# Extract aoestats' `replay_summary_raw` (the per-age build summary that exists
# for ~4.33M replay-enhanced ranked matches / ~15M player-rows) into a tidy,
# queryable shape. This is depth we already OWN — villager counts, military,
# economy buildings, techs, and exact age-up times per player per age — that was
# never pulled out of the raw blob. aoe2.net is dead and the replay blobs went
# private, so this is the maximum replay-derived data obtainable; we mine it once.
#
# `replay_summary_raw` is a PYTHON-REPR string (single quotes, True/False/None),
# NOT JSON — so it's parsed with ast.literal_eval, not json.loads.
#
# Pipe newline-delimited JSON rows in (game_id, profile_id, civ, winner,
# replay_summary_raw) from the duckdb CLI; emit one NDJSON row per player-per-age:
#   { game_id, profile_id, civ, won, ladder?, age, uptime, reached,
#     villagers, fishing_ships, military, n_buildings, n_research,
#     units{}, buildings{}, research[] }
#
#   ~/bin/duckdb -c "COPY (SELECT game_id, profile_id, civ, winner,
#       replay_summary_raw FROM read_parquet('~/aoestats/p_*.parquet')
#       WHERE replay_summary_raw IS NOT NULL AND length(replay_summary_raw)>50)
#       TO '/dev/stdout' (FORMAT JSON)" | python3 extract-replay-summaries.py > out.ndjson

import sys, ast, json

AGES = ("dark", "feudal", "castle", "imperial")
# non-military population we don't want to count as "military"
NON_MIL = {"villager", "fishing ship", "trade cart", "trade cog", "sheep", "llama",
           "cow", "goat", "turkey", "water buffalo", "fish trap"}

def main():
    rows = bad = out = 0
    w = sys.stdout.write
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        rows += 1
        try:
            rec = json.loads(line)
            summ = rec.get("replay_summary_raw")
            d = ast.literal_eval(summ) if isinstance(summ, str) else None
            ages = (d or {}).get("age_stats") or {}
        except Exception:
            bad += 1
            continue
        base_gid = rec.get("game_id")
        base_pid = rec.get("profile_id")
        base_civ = rec.get("civ")
        base_won = rec.get("winner")
        for age in AGES:
            a = ages.get(age)
            if not a:
                continue
            units = a.get("unit_counts") or {}
            blds = a.get("building_counts") or {}
            research = a.get("research") or []
            villagers = int(units.get("villager", 0) or 0)
            fishing = int(units.get("fishing ship", 0) or 0)
            military = sum(int(v or 0) for k, v in units.items() if k not in NON_MIL)
            w(json.dumps({
                "game_id": base_gid,
                "profile_id": base_pid,
                "civ": base_civ,
                "won": bool(base_won) if base_won is not None else None,
                "age": age,
                "uptime": a.get("uptime"),               # seconds to reach this age
                "reached": bool(a.get("age_researched")),
                "villagers": villagers,
                "fishing_ships": fishing,
                "military": military,
                "n_buildings": sum(int(v or 0) for v in blds.values()),
                "n_research": len(research),
                "units": units,
                "buildings": blds,
                "research": research,
            }, separators=(",", ":")))
            w("\n")
            out += 1
    sys.stderr.write(f"extract: {rows} player-rows in, {out} age-rows out, {bad} unparseable\n")

if __name__ == "__main__":
    main()
