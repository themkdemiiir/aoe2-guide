// scripts/data-pipeline/lib/stats.mjs
//
// Single source for the stat helpers previously duplicated across 9 generators;
// tier thresholds live ONLY here.

// Wilson score 95% CI for a binomial proportion (robust for small/extreme n).
export function wilson(wins, n, z = 1.96) {
  if (!n) return [0, 0];
  const p = wins / n, d = 1 + (z * z) / n;
  const c = (p + (z * z) / (2 * n)) / d;
  const m = (z * Math.sqrt((p * (1 - p)) / n + (z * z) / (4 * n * n))) / d;
  return [c - m, c + m];
}

// Fraction → percent, 2 decimals (0.5123 → 51.23).
export const pct = (x) => +(x * 100).toFixed(2);

// Tier from a win rate expressed as a PERCENT (not a fraction).
export const tierOf = (w) => (w >= 53 ? "S" : w >= 51 ? "A" : w >= 49 ? "B" : w >= 47 ? "C" : "D");

export const TIER_METHOD = "Win rate: ≥53 S | 51–53 A | 49–51 B | 47–49 C | <47 D. No data → null.";
