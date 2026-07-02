// Single source of truth for tier thresholds and the dark codex tier palette.
// The homepage (src/pages/[lang]/index.astro) uses an intentionally BRIGHTER
// variant for legibility on its dark cover — documented there, not a duplicate.
// Any is:inline script copy of tierOf must carry a "// mirror of src/lib/tiers.ts"
// comment (is:inline scripts can't import modules).
export const tierOf = (wr: number) => (wr >= 53 ? "S" : wr >= 51 ? "A" : wr >= 49 ? "B" : wr >= 47 ? "C" : "D");

export const TIER_COLORS: Record<string, string> = {
  S: "#8a2d22",
  A: "#9a6a1e",
  B: "#6b5b41",
  C: "#7a6a4a",
  D: "#8a7a5a",
}; // dark codex palette
