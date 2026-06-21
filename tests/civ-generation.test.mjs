import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import yaml from "js-yaml";
import civData from "../src/data/civilizations.json" assert { type: "json" };

const CIV_DIR = "src/content/civilizations";
const FABRICATED_SLUGS = ["achaemenids", "athenians", "macedonians", "spartans", "thracians", "puru"];

describe("civilizations.json integrity", () => {
  it("has exactly 53 civs", () => {
    expect(civData.civs.length).toBe(53);
  });

  it("contains no fabricated civ slugs", () => {
    const slugs = civData.civs.map((c) => c.slug);
    for (const bad of FABRICATED_SLUGS) {
      expect(slugs, `fabricated slug "${bad}" found in civilizations.json`).not.toContain(bad);
    }
  });
});

describe("civ YAML bilingual invariants", () => {
  const files = readdirSync(CIV_DIR).filter((f) => f.endsWith(".yaml"));

  for (const file of files) {
    const slug = file.replace(/\.yaml$/, "");
    const data = yaml.load(readFileSync(`${CIV_DIR}/${file}`, "utf8"));

    it(`[${slug}] bonuses.tr is not a copy of bonuses.en`, () => {
      expect(
        data.bonuses?.tr,
        `${slug}: bonuses.tr is missing`,
      ).toBeDefined();
      expect(
        JSON.stringify(data.bonuses?.tr),
        `${slug}: bonuses.tr is deep-equal to bonuses.en (untranslated)`,
      ).not.toBe(JSON.stringify(data.bonuses?.en));
    });

    it(`[${slug}] teamBonus.tr !== teamBonus.en`, () => {
      expect(
        data.teamBonus?.tr,
        `${slug}: teamBonus.tr is missing`,
      ).toBeDefined();
      expect(
        data.teamBonus?.tr,
        `${slug}: teamBonus.tr is identical to teamBonus.en (untranslated)`,
      ).not.toBe(data.teamBonus?.en);
    });

    it(`[${slug}] uniqueTechs castle effect is translated`, () => {
      const castle = data.uniqueTechs?.castle?.effect;
      expect(castle?.tr, `${slug}: uniqueTechs.castle.effect.tr is missing`).toBeDefined();
      expect(
        castle?.tr,
        `${slug}: uniqueTechs.castle.effect.tr === .en (untranslated)`,
      ).not.toBe(castle?.en);
    });

    it(`[${slug}] uniqueTechs imperial effect is translated`, () => {
      const imperial = data.uniqueTechs?.imperial?.effect;
      expect(imperial?.tr, `${slug}: uniqueTechs.imperial.effect.tr is missing`).toBeDefined();
      expect(
        imperial?.tr,
        `${slug}: uniqueTechs.imperial.effect.tr === .en (untranslated)`,
      ).not.toBe(imperial?.en);
    });

    if (data.strategy) {
      it(`[${slug}] strategy.tr is a non-empty string`, () => {
        expect(typeof data.strategy?.tr, `${slug}: strategy.tr is not a string`).toBe("string");
        expect(
          data.strategy?.tr?.trim().length,
          `${slug}: strategy.tr is empty`,
        ).toBeGreaterThan(0);
      });
    }

    it(`[${slug}] tagline.tr !== tagline.en`, () => {
      expect(data.tagline?.tr, `${slug}: tagline.tr is missing`).toBeDefined();
      expect(
        data.tagline?.tr,
        `${slug}: tagline.tr is identical to tagline.en (untranslated)`,
      ).not.toBe(data.tagline?.en);
    });
  }
});
