// scripts/data-pipeline/lib/duck.mjs
//
// One-shot DuckDB query helper shared by the aggregate-* generators (previously
// three local copies). Runs `duckdb -json -c <sql>` via execFileSync (argv, no
// shell string interpolation), whitespace-squashes the SQL, and returns the
// parsed rows ([] when the query emits nothing).

import { execFileSync } from "node:child_process";

export function duck(sql, { duckdb = `${process.env.HOME}/bin/duckdb`, maxBuffer = 1 << 30 } = {}) {
  const oneLine = sql.replace(/\s+/g, " ").trim();
  const out = execFileSync(duckdb, ["-json", "-c", oneLine], { maxBuffer }).toString().trim();
  return out ? JSON.parse(out) : [];
}
