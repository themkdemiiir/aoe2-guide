// Canonical slug → display-name prettifier: every word capitalized
// ("black-forest" / "black_forest" → "Black Forest"). Display only — never
// use this for slug generation (that lives in scripts/).
export const displayName = (slug: string) =>
  slug
    .split(/[-_]/)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
