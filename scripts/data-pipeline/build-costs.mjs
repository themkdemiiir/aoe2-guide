// Extract unit/building/tech costs from aoe2techtree data.json -> replay-rs/data/costs.json.
// Run from repo root: node scripts/data-pipeline/build-costs.mjs  (reads .cache/aoe2-data/data.json)
import { readFileSync, writeFileSync } from "node:fs";
const src = JSON.parse(readFileSync(".cache/aoe2-data/data.json", "utf8")).data;
const missing = [];
const cost = (c) => ({ food: c.Food || 0, wood: c.Wood || 0, gold: c.Gold || 0, stone: c.Stone || 0 });
const pick = (kind, obj) =>
  Object.fromEntries(
    Object.entries(obj).map(([id, v]) => {
      if (v.Cost == null) { missing.push(`${kind}/${id}`); return [id, null]; }
      return [id, cost(v.Cost)];
    }),
  );
const out = { _source: "aoe2techtree data/data.json", unit: pick("unit", src.Unit), building: pick("building", src.Building), tech: pick("tech", src.Tech) };
// Fail loud on an ABSENT Cost (data gap) — a present empty/zero Cost is legitimate and passes.
if (missing.length) {
  throw new Error(`build-costs: ${missing.length} entr${missing.length > 1 ? "ies" : "y"} with ABSENT Cost (data gap, not free): ${missing.join(", ")}`);
}
writeFileSync("scripts/data-pipeline/replay-rs/data/costs.json", JSON.stringify(out));
console.log(`costs.json: ${Object.keys(out.unit).length} units, ${Object.keys(out.building).length} buildings, ${Object.keys(out.tech).length} techs`);
