// Guards for the TWO distinct civ-id spaces (the production civ-label bug of
// 2026-06/07 came from conflating them):
//   src/data/civ-id-map.json        — GAME/replay civ_id -> slug
//   src/data/relic-civ-id-map.json  — Relic API civilization_id -> slug
// Derivation + evidence: relic-civ-id-map.json provenance block.
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => JSON.parse(readFileSync(path.join(ROOT, p), "utf8"));

const gameMap = read("src/data/civ-id-map.json");
const relicDoc = read("src/data/relic-civ-id-map.json");
const relicMap = relicDoc.map;
const guideCivs = new Set(read("src/data/civilizations.json").civs.map((c) => c.slug));

// Chronicles civs exist in both id spaces but are not on the ranked ladder / site.
const NON_GUIDE = new Set(["random", "achaemenids", "athenians", "spartans", "macedonians", "thracians", "puru"]);

describe("relic-civ-id-map (API id space)", () => {
  it("covers exactly the current-era API ids (0-52, 55-60; 53/54 retired)", () => {
    const ids = Object.keys(relicMap).map(Number).sort((a, b) => a - b);
    const expected = [...Array(53).keys(), 55, 56, 57, 58, 59, 60];
    expect(ids).toEqual(expected);
  });

  it("maps every id to a guide civ or a documented non-ranked slug", () => {
    for (const slug of Object.values(relicMap)) {
      expect(guideCivs.has(slug) || NON_GUIDE.has(slug), `unexpected slug ${slug}`).toBe(true);
    }
  });

  it("matches the replay-verified ground-truth pairs", () => {
    // From the 734k-row replay×crawl join + the API races table (2026-07-02).
    expect(relicMap["0"]).toBe("armenians");
    expect(relicMap["21"]).toBe("hindustanis"); // API still names it "Indians"
    expect(relicMap["32"]).toBe("persians"); // the user's own mislabeled game
    expect(relicMap["51"]).toBe("jurchens");
    expect(relicMap["58"]).toBe("muisca");
    expect(relicMap["60"]).toBe("tupi");
  });

  it("genuinely differs from the game id space (same table = regression)", () => {
    expect(relicMap["32"]).not.toBe(gameMap["32"]); // persians vs bulgarians
    expect(relicMap["0"]).not.toBe(gameMap["0"]); // armenians vs random
  });

  it("declares its era validity", () => {
    expect(relicDoc.provenance.validFrom).toBe("2025-09-01");
  });
});

describe("civ-id-map (game/replay id space)", () => {
  it("has the corrected tail: 57=muisca, 58=mapuche, 59=tupi, no 60", () => {
    expect(gameMap["57"]).toBe("muisca"); // was "non-guide-57" — a real civ all along
    expect(gameMap["58"]).toBe("mapuche");
    expect(gameMap["59"]).toBe("tupi");
    expect(gameMap["60"]).toBeUndefined(); // no game id 60 exists (was a spurious "muisca")
  });
});

describe("replay-rs baked tsv mirrors", () => {
  const parseTsv = (p) =>
    Object.fromEntries(
      readFileSync(path.join(ROOT, p), "utf8")
        .split("\n")
        .filter((l) => l.trim() && !l.startsWith("#"))
        .map((l) => l.split("\t").map((s) => s.trim())),
    );

  it("relic-civs.tsv is in sync with relic-civ-id-map.json", () => {
    expect(parseTsv("scripts/data-pipeline/replay-rs/data/relic-civs.tsv")).toEqual(relicMap);
  });

  it("civs.tsv is in sync with civ-id-map.json", () => {
    expect(parseTsv("scripts/data-pipeline/replay-rs/data/civs.tsv")).toEqual(gameMap);
  });
});
