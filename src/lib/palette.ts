// Single source of truth for the data-viz colors that live in inline styles /
// Chart.js configs (Tailwind theme tokens can't reach those). Import in .astro
// frontmatter or module scripts; pass into is:inline scripts via define:vars.
// Keep any is:inline copy annotated "// mirror of src/lib/palette.ts".
//
// These are theme-independent brand hues (win/loss, difficulty, player slots) —
// deliberately NOT swapped per data-theme; where dark-mode legibility matters,
// the surrounding component supplies a lightened variant via a CSS token.

/** win-rate / stat bars and text */
export const STAT_GOOD = "#c79a3c"; // above-average bar fill
export const STAT_BAD = "#8a2d22"; // below-average bar fill
export const GOOD_TEXT = "#8a6a1f"; // above-average win-rate text (AA on parchment)
export const POS_TEXT = "#3f6b34"; // favored / positive edge text
export const NEUTRAL = "#6b5b41"; // neutral stone accent + chart default ink
export const TRACK = "rgba(120,96,52,0.16)"; // empty progress-track fill

/** build-order difficulty accent */
export const DIFF_COLORS: Record<string, string> = {
  beginner: "#3f6b34",
  intermediate: "#9a6a1e",
  advanced: "#8a2d22",
};

/** homepage dark-cover tier palette — brighter than lib/tiers.ts TIER_COLORS for
 *  legibility on bg-stone-bg (documented at the home page). */
export const HOME_TIER_COLORS: Record<string, string> = {
  S: "#e0654f",
  A: "#e6bd5e",
  B: "#bda983",
  C: "#a3926e",
  D: "#8f7f60",
};

/** analyzer per-player slot colors (2v2/3v3/4v4) */
export const PLAYER_COLORS = [
  "#2f6bd8",
  "#c2352b",
  "#3f8f3f",
  "#c78a1e",
  "#7a4fb0",
  "#0f8a8a",
  "#c04f9c",
  "#6b7a2a",
];
