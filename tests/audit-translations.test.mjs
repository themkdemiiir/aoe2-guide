import { describe, it, expect } from "vitest";
import { auditEntry } from "../scripts/lib/audit-core.mjs";

describe("auditEntry", () => {
  it("flags an untranslated descriptive field", () => {
    const issues = auditEntry("civilizations", "x.yaml", {
      tagline: { en: "A civ.", tr: "A civ." },
    });
    expect(issues).toContain("tagline");
  });

  it("allows proper-noun fields where en==tr", () => {
    const issues = auditEntry("civilizations", "x.yaml", {
      name: { en: "Britons", tr: "Britons" },
      uniqueTechs: { castle: { name: { en: "Warwolf", tr: "Warwolf" }, effect: { en: "A", tr: "B" } } },
    });
    expect(issues).toEqual([]);
  });

  it("allows a unit role naming a proper noun (Britons)", () => {
    const issues = auditEntry("units", "longbowman.yaml", {
      role: { en: "Unique foot archer (Britons)", tr: "Unique foot archer (Britons)" },
    });
    expect(issues).toEqual([]);
  });
});
