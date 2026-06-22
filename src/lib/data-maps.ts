// Shared source of truth for data-derived map pages (maps we have aoestats
// archive rankings for). aoestats names the same map inconsistently across dumps
// (golden_pit vs goldenpit, scandanavia vs scandinavia), so everything here keys
// off a CANONICAL form — alphanumerics only — and always resolves to the single
// highest-volume variant. Both the map index and the [map] route consume this so
// they never drift and never ship two pages for one map.
import mapMeta from "@data/map-meta.json";

type Bucket = { civ: string; games: number; winRate: number }[];
type MetaMap = {
  games: { "1v1": number; team: number };
  "1v1": Record<string, Bucket> | null;
  team: Record<string, Bucket> | null;
};
const META_MAPS = (mapMeta as unknown as { maps: Record<string, MetaMap> }).maps;

// Fold a couple of misspelled aoestats keys onto the correct spelling.
const ALIAS: Record<string, string> = { scandanavia: "scandinavia" };
// Below this many ranked civs (either ladder, "all" tier) a page is too sparse.
const MIN_RANKED_CIVS = 10;

/** Canonical map identity — strip separators/case so naming variants collapse. */
export const canon = (s: string) => s.toLowerCase().replace(/[^a-z0-9]/g, "");
const display = (k: string) => ALIAS[k] ?? k;

export interface DataMap {
  key: string; // raw aoestats key with the most games, e.g. "golden_pit"
  slug: string; // URL slug from the canonical display name, e.g. "golden-pit"
  games: number; // total ranked appearances (1v1 + team) for that key
}

/**
 * Every archived map with a usable ranking, one entry per canonical map (the
 * highest-volume variant wins), sorted by total games desc.
 */
export function dedupedMetaMaps(): DataMap[] {
  const byCanon = new Map<string, DataMap>();
  for (const [k, v] of Object.entries(META_MAPS)) {
    const rankedCivs = Math.max(v["1v1"]?.all?.length ?? 0, v.team?.all?.length ?? 0);
    if (rankedCivs < MIN_RANKED_CIVS) continue;
    const c = canon(display(k));
    const games = v.games["1v1"] + v.games.team;
    const cur = byCanon.get(c);
    if (!cur || games > cur.games) byCanon.set(c, { key: k, slug: display(k).replace(/_/g, "-"), games });
  }
  return [...byCanon.values()].sort((a, b) => b.games - a.games);
}

/** The best map-meta key whose canonical name matches `slug` (highest games). */
export function resolveMetaKey(slug: string): string | null {
  const c = canon(slug);
  let best: { key: string; games: number } | null = null;
  for (const [k, v] of Object.entries(META_MAPS)) {
    if (canon(display(k)) !== c) continue;
    const games = v.games["1v1"] + v.games.team;
    if (!best || games > best.games) best = { key: k, games };
  }
  return best?.key ?? null;
}
