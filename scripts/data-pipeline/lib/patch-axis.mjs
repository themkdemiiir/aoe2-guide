// scripts/data-pipeline/lib/patch-axis.mjs
//
// One place that turns a crawl record into a game BUILD ("179158"), from
// src/data/patch-index.json:
//   - gamemod_id → build (replay-verified / release-date-aligned pairs)
//   - legacy snapshot records (pre-stream crawlers) carry no gamemod_id:
//     assigned by the official release calendar instead. Release-day matches
//     land on the NEW build (dates are UTC-midnight; the switchover hour is
//     not knowable from the API).
//   - documented anomaly gamemods → null (caller skips)
//   - anything else → throw. A new game update means a new gamemod_id; add it
//     to patch-index.json rather than letting records default anywhere.

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const patchIndex = JSON.parse(readFileSync(path.join(REPO, "src/data/patch-index.json"), "utf8"));

const labelOf = Object.fromEntries(patchIndex.builds.map((b) => [String(b.build), b.label]));
const byGamemod = patchIndex.gamemodToBuild;
const ignoreGamemods = new Set(patchIndex.ignoreGamemods ?? []);
// oldest → newest, for "latest release ≤ completed" lookup
const calendar = patchIndex.builds
  .map((b) => ({ build: String(b.build), ts: Date.parse(`${b.released}T00:00:00Z`) / 1000 }))
  .sort((a, b) => a.ts - b.ts);

export const patchLabel = (build) => labelOf[String(build)] ?? `Update ${build}`;

export function buildOf(m) {
  if (m.gamemod_id != null) {
    if (ignoreGamemods.has(m.gamemod_id)) return null;
    const b = byGamemod[String(m.gamemod_id)];
    if (!b) {
      throw new Error(
        `patch-axis: gamemod_id ${m.gamemod_id} has no build mapping (match ${m.match_id}, ` +
          `completed ${new Date(m.completed * 1000).toISOString().slice(0, 10)}) — ` +
          "a new game update shipped; add it to src/data/patch-index.json",
      );
    }
    return String(b);
  }
  let cur = null;
  for (const c of calendar) {
    if (m.completed >= c.ts) cur = c.build;
    else break;
  }
  if (cur == null) {
    throw new Error(`patch-axis: match ${m.match_id} completed before the earliest indexed build`);
  }
  return cur;
}
