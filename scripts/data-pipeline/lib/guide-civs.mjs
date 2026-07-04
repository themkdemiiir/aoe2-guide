// Single source for "which civs the guide covers" — the slug set used to filter
// crawl/replay rows down to guide civs. Was copy-pasted across ~8 generators.
import { readFileSync } from "node:fs";
import path from "node:path";

/** Set of guide civ slugs from src/data/civilizations.json. */
export function loadGuideCivs() {
  const file = path.resolve("src/data/civilizations.json");
  return new Set(JSON.parse(readFileSync(file, "utf8")).civs.map((c) => c.slug));
}
