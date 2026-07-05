// Pinned upstream source SHAs shared by the sync/build scripts. Single source
// of truth — bump here (or override via env) rather than editing literals in
// each script.
//
//   SiegeEngineers/aoe2techtree — MIT (code) · MS Game Content Usage Rules (assets)
//     used by: sync-assets.mjs, build-icon-map.mjs, sync-game-data.mjs
//   aalises/age-of-empires-II-api — BSD-3-Clause
//     used by: sync-game-data.mjs, build-game-facts.mjs
//
// Refresh policy: bump the constants below in a deliberate PR, or override via
// AOE2TT_SHA / AALISES_SHA env vars for a one-off run.

export const AOE2TECHTREE_SHA =
  process.env.AOE2TT_SHA || "b9d494df6921d4080df69b22f9dbb7a4d1dcd9f0";

export const AALISES_SHA = process.env.AALISES_SHA || "3ec582fa0ebd5ea11b2d1ff405e61836c6f3a99d";
