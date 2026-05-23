import { describe, it, expect } from "vitest";
import { localeFromEntryId, canonicalSlug } from "./content";

describe("localeFromEntryId", () => {
  it("extracts locale from a typical entry id", () => {
    expect(localeFromEntryId("en/britons")).toBe("en");
    expect(localeFromEntryId("tr/franks")).toBe("tr");
    expect(localeFromEntryId("es/maps/arabia")).toBe("es");
    expect(localeFromEntryId("de/builds/21pop-archer")).toBe("de");
  });

  it("falls back to defaultLocale when id has no locale prefix", () => {
    expect(localeFromEntryId("britons")).toBe("en");
    expect(localeFromEntryId("")).toBe("en");
    expect(localeFromEntryId("xx/britons")).toBe("en");
  });
});

describe("canonicalSlug", () => {
  it("strips the locale prefix", () => {
    expect(canonicalSlug("en/britons")).toBe("britons");
    expect(canonicalSlug("tr/franks")).toBe("franks");
  });

  it("preserves nested slugs after locale", () => {
    expect(canonicalSlug("tr/builds/21pop-archer")).toBe("builds/21pop-archer");
    expect(canonicalSlug("de/learn/01-resources")).toBe("learn/01-resources");
  });

  it("strips .md and .mdx extensions", () => {
    expect(canonicalSlug("en/britons.md")).toBe("britons");
    expect(canonicalSlug("tr/britons.mdx")).toBe("britons");
    expect(canonicalSlug("en/builds/test.MD")).toBe("builds/test");
  });
});
