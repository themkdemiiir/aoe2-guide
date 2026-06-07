import type { CollectionEntry } from "astro:content";
import { afterEach, describe, expect, it } from "vitest";
import { resetMockCollections, setMockCollection } from "./__mocks__/astro-content";
import {
  canonicalSlug,
  getLocalizedEntries,
  getLocalizedEntry,
  localeFromEntryId,
} from "./content";

function entry(id: string, name: string): CollectionEntry<"civilizations"> {
  return { id, data: { name } } as CollectionEntry<"civilizations">;
}

afterEach(() => {
  resetMockCollections();
});

describe("localeFromEntryId", () => {
  it("extracts locale from a typical entry id", () => {
    expect(localeFromEntryId("en/britons")).toBe("en");
    expect(localeFromEntryId("tr/franks")).toBe("tr");
    expect(localeFromEntryId("tr/maps/arabia")).toBe("tr");
    expect(localeFromEntryId("en/builds/21pop-archer")).toBe("en");
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
    expect(canonicalSlug("en/learn/01-resources")).toBe("learn/01-resources");
  });

  it("strips .md and .mdx extensions", () => {
    expect(canonicalSlug("en/britons.md")).toBe("britons");
    expect(canonicalSlug("tr/britons.mdx")).toBe("britons");
    expect(canonicalSlug("en/builds/test.MD")).toBe("builds/test");
  });
});

describe("getLocalizedEntries", () => {
  it("uses default locale entries for the default locale", async () => {
    setMockCollection("civilizations", [
      entry("en/britons", "Britons"),
      entry("tr/britons", "Britonlar"),
    ]);

    await expect(getLocalizedEntries("civilizations", "en")).resolves.toMatchObject([
      { id: "en/britons" },
    ]);
  });

  it("overlays translated entries onto the default locale list", async () => {
    setMockCollection("civilizations", [
      entry("en/britons", "Britons"),
      entry("en/franks", "Franks"),
      entry("tr/britons", "Britonlar"),
    ]);

    await expect(getLocalizedEntries("civilizations", "tr")).resolves.toMatchObject([
      { id: "tr/britons", data: { name: "Britonlar" } },
      { id: "en/franks", data: { name: "Franks" } },
    ]);
  });
});

describe("getLocalizedEntry", () => {
  it("returns exact localized entries before falling back to default locale", async () => {
    setMockCollection("civilizations", [
      entry("en/britons", "Britons"),
      entry("en/franks", "Franks"),
      entry("tr/britons", "Britonlar"),
    ]);

    await expect(getLocalizedEntry("civilizations", "britons", "tr")).resolves.toMatchObject({
      entry: { id: "tr/britons" },
      fallback: false,
    });
    await expect(getLocalizedEntry("civilizations", "franks", "tr")).resolves.toMatchObject({
      entry: { id: "en/franks" },
      fallback: true,
    });
  });
});
