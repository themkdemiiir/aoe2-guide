// Client-side query engine for the Data Explorer — pure functions over the shipped
// win-rate cube (public/civ-cube.json). No R2, no WASM, no cost: everything the
// explorer asks is a sum over ≤40k in-memory rows. Same math as the civ/map filters,
// factored out so it can be unit-tested in isolation.

export type Cube = {
  civs: string[];
  elos: string[];
  maps: string[];
  months: { patch: string; label?: string }[];
  rows: number[][]; // each: [civIdx, eloIdx, mapIdx, patchIdx, games, wins]
};

export type Sel = {
  civ: string; // required civ slug
  map?: string; // map key, or "" = any
  elo?: string; // band ("1650-1799"), cumulative ("cum:1650"), or "" = any
  patch?: string; // patch id, or "" = all-time
};

export type SliceResult = { games: number; wins: number; wr: number | null };
export type BreakdownRow = { key: string; label: string; games: number; wr: number };

/** Predicate over an elo-bucket index for a selection value. */
function eloMatcher(cube: Cube, elo: string | undefined): (eloIdx: number) => boolean {
  if (!elo) return () => true;
  if (elo.startsWith("cum:")) {
    const cut = +elo.slice(4);
    const start = cube.elos.findIndex((b) => Number.parseInt(b, 10) >= cut);
    return start < 0 ? () => false : (i) => i >= start;
  }
  const idx = cube.elos.indexOf(elo);
  return idx < 0 ? () => false : (i) => i === idx;
}

/** Sum games+wins for one civ × map × elo × patch slice → win rate. */
export function sliceWinRate(cube: Cube, sel: Sel): SliceResult {
  const ci = cube.civs.indexOf(sel.civ);
  if (ci < 0) return { games: 0, wins: 0, wr: null };
  const eloOk = eloMatcher(cube, sel.elo);
  const mi = sel.map ? cube.maps.indexOf(sel.map) : -1;
  const pi = sel.patch ? cube.months.findIndex((m) => m.patch === sel.patch) : -1;
  // A specified-but-absent map/patch means "no data", never silently widen.
  if ((sel.map && mi < 0) || (sel.patch && pi < 0)) return { games: 0, wins: 0, wr: null };
  let g = 0;
  let w = 0;
  for (const r of cube.rows) {
    if (r[0] !== ci) continue;
    if (!eloOk(r[1])) continue;
    if (mi >= 0 && r[2] !== mi) continue;
    if (pi >= 0 && r[3] !== pi) continue;
    g += r[4];
    w += r[5];
  }
  return { games: g, wins: w, wr: g > 0 ? +((w / g) * 100).toFixed(2) : null };
}

/**
 * Break the slice down across one dimension — fix the other selectors, group by the
 * chosen dim's index, return per-group win rates (games ≥ 1). Ordered by the cube's
 * natural order for elo/patch, by win rate (desc) for map.
 */
export function breakdown(cube: Cube, sel: Sel, dim: "elo" | "map" | "patch"): BreakdownRow[] {
  const ci = cube.civs.indexOf(sel.civ);
  if (ci < 0) return [];
  const eloOk = eloMatcher(cube, sel.elo);
  const mi = sel.map ? cube.maps.indexOf(sel.map) : -1;
  const pi = sel.patch ? cube.months.findIndex((m) => m.patch === sel.patch) : -1;
  if ((sel.map && mi < 0) || (sel.patch && pi < 0)) return [];
  const col = dim === "elo" ? 1 : dim === "map" ? 2 : 3;
  const agg = new Map<number, [number, number]>(); // dimIdx -> [games, wins]
  for (const r of cube.rows) {
    if (r[0] !== ci) continue;
    if (dim !== "elo" && !eloOk(r[1])) continue;
    if (dim !== "map" && mi >= 0 && r[2] !== mi) continue;
    if (dim !== "patch" && pi >= 0 && r[3] !== pi) continue;
    const a = agg.get(r[col]) ?? [0, 0];
    a[0] += r[4];
    a[1] += r[5];
    agg.set(r[col], a);
  }
  const labelOf = (idx: number) =>
    dim === "elo"
      ? cube.elos[idx]
      : dim === "map"
        ? cube.maps[idx]
        : (cube.months[idx]?.label ?? cube.months[idx]?.patch ?? String(idx));
  const rows: BreakdownRow[] = [...agg.entries()]
    .filter(([, gw]) => gw[0] > 0)
    .map(([idx, gw]) => ({
      key: String(idx),
      label: labelOf(idx),
      games: gw[0],
      wr: +((gw[1] / gw[0]) * 100).toFixed(2),
    }));
  if (dim === "map") rows.sort((a, b) => b.wr - a.wr);
  else rows.sort((a, b) => +a.key - +b.key); // natural elo/patch order
  return rows;
}
