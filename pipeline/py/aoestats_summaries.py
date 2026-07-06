#!/usr/bin/env python3
# pipeline/py/aoestats_summaries.py
#
# Task M4b's ONE sanctioned non-Rust step: aoestats' `replay_summary_raw` is a PYTHON-REPR string
# (single quotes, True/False/None), NOT JSON, so it has to go through `ast.literal_eval` — no Rust
# crate parses Python-repr cleanly. This is a byte-faithful PORT of the metric logic in
# `scripts/data-pipeline/extract-replay-summaries.py` (the AGES tuple, the NON_MIL set, the
# villager/military tallying, the `uptime` passthrough) — do NOT re-derive that logic here if it
# ever needs to change; port the update from the original instead.
#
# The one deliberate difference from the original: this port's OUTPUT only carries the fields the
# `match_ages` Postgres table has columns for. The original also emits `reached`/`fishing_ships`/
# `units{}`/`buildings{}`/`research[]` (and reads an optional `ladder` field) — dead weight for
# this table, so they're dropped here rather than carried uselessly through the
# `pipeline/crates/aoestats` COPY path. If `match_ages` ever grows those columns, port them back in
# from the original rather than re-deriving.
#
# Invoked by `pipeline/crates/aoestats/src/py.rs` (`run_summaries`) as a `python3 -c <this file's
# embedded source>` subprocess — stdlib only (`sys`, `ast`, `json`), no third-party deps, so it
# runs anywhere `python3` is on `PATH`.
#
# Reads NDJSON lines `{game_id, profile_id, civ, winner, replay_summary_raw}` from stdin; emits one
# NDJSON row per player-per-age reached:
#   { game_id, profile_id, civ, won, age, uptime, villagers, military, n_buildings, n_research }
#
# Standalone manual invocation (matches the original's own usage comment):
#   ~/bin/duckdb -c "COPY (SELECT game_id, profile_id, civ, winner,
#       replay_summary_raw FROM read_parquet('~/aoestats/p_*.parquet')
#       WHERE replay_summary_raw IS NOT NULL AND length(replay_summary_raw)>50)
#       TO '/dev/stdout' (FORMAT JSON)" | python3 aoestats_summaries.py > out.ndjson

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
            military = sum(int(v or 0) for k, v in units.items() if k not in NON_MIL)
            w(json.dumps({
                "game_id": base_gid,
                "profile_id": base_pid,
                "civ": base_civ,
                "won": bool(base_won) if base_won is not None else None,
                "age": age,
                "uptime": a.get("uptime"),               # seconds to reach this age
                "villagers": villagers,
                "military": military,
                "n_buildings": sum(int(v or 0) for v in blds.values()),
                "n_research": len(research),
            }, separators=(",", ":")))
            w("\n")
            out += 1
    sys.stderr.write(f"aoestats_summaries: {rows} player-rows in, {out} age-rows out, {bad} unparseable\n")

if __name__ == "__main__":
    main()
