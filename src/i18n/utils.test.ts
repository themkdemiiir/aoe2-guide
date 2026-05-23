import { describe, it, expect } from "vitest";
import { getLocaleFromUrl, t, localizedPath, allLocalePaths } from "./utils";

describe("getLocaleFromUrl", () => {
  it("returns locale from /<lang>/ prefix", () => {
    expect(getLocaleFromUrl(new URL("https://x/en/civs/britons"))).toBe("en");
    expect(getLocaleFromUrl(new URL("https://x/tr/civs/britons"))).toBe("tr");
    expect(getLocaleFromUrl(new URL("https://x/es/civs"))).toBe("es");
    expect(getLocaleFromUrl(new URL("https://x/de/"))).toBe("de");
  });
  it("falls back to defaultLocale when no /<lang>/ prefix", () => {
    expect(getLocaleFromUrl(new URL("https://x/"))).toBe("en");
    expect(getLocaleFromUrl(new URL("https://x/civs"))).toBe("en");
  });
});

describe("t", () => {
  it("returns translated string for known key", () => {
    expect(t("tr", "nav.civs")).toBe("Uygarlıklar");
    expect(t("de", "nav.units")).toBe("Einheiten");
    expect(t("es", "nav.maps")).toBe("Mapas");
  });
});

describe("localizedPath", () => {
  it("swaps the leading locale segment", () => {
    expect(localizedPath("/en/civs/britons", "tr")).toBe("/tr/civs/britons");
    expect(localizedPath("/tr/builds", "es")).toBe("/es/builds");
  });
  it("inserts locale when missing", () => {
    expect(localizedPath("/civs/britons", "de")).toBe("/de/civs/britons");
    expect(localizedPath("civs", "en")).toBe("/en/civs");
  });
});

describe("allLocalePaths", () => {
  it("returns paths for all 4 locales", () => {
    const paths = allLocalePaths("/civs/britons");
    expect(paths).toHaveLength(4);
    expect(paths.find((p) => p.locale === "tr")?.path).toBe("/tr/civs/britons");
    expect(paths.find((p) => p.locale === "de")?.path).toBe("/de/civs/britons");
  });
});
