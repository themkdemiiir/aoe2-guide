// Extract unit/building/tech costs from aoe2techtree data.json -> replay-rs/data/costs.json.
// Run from repo root: node scripts/data-pipeline/build-costs.mjs  (reads .cache/aoe2-data/data.json)
import { readFileSync, writeFileSync } from "node:fs";
const src = JSON.parse(readFileSync(".cache/aoe2-data/data.json", "utf8")).data;
const cost = (c = {}) => ({ food: c.Food || 0, wood: c.Wood || 0, gold: c.Gold || 0, stone: c.Stone || 0 });
const pick = (obj) => Object.fromEntries(Object.entries(obj).map(([id, v]) => [id, cost(v.Cost)]));
const out = { _source: "aoe2techtree data/data.json", unit: pick(src.Unit), building: pick(src.Building), tech: pick(src.Tech) };
writeFileSync("scripts/data-pipeline/replay-rs/data/costs.json", JSON.stringify(out));
console.log(`costs.json: ${Object.keys(out.unit).length} units, ${Object.keys(out.building).length} buildings, ${Object.keys(out.tech).length} techs`);
