import { describe, expect, it } from "vitest";
import { allLocalePaths, getLocaleFromUrl, localizedPath, t } from "./utils";

describe("getLocaleFromUrl", () => {
  it("returns locale from /<lang>/ prefix", () => {
    expect(getLocaleFromUrl(new URL("https://x/en/civs/britons"))).toBe("en");
    expect(getLocaleFromUrl(new URL("https://x/tr/civs/britons"))).toBe("tr");
  });
  it("falls back to defaultLocale when no /<lang>/ prefix", () => {
    expect(getLocaleFromUrl(new URL("https://x/"))).toBe("en");
    expect(getLocaleFromUrl(new URL("https://x/civs"))).toBe("en");
  });
});

describe("t", () => {
  it("returns translated string for known key", () => {
    expect(t("tr", "nav.civs")).toBe("Uygarlıklar");
    expect(t("en", "nav.units")).toBe("Units");
  });
});

describe("localizedPath", () => {
  it("swaps the leading locale segment", () => {
    expect(localizedPath("/en/civs/britons", "tr")).toBe("/tr/civs/britons");
    expect(localizedPath("/tr/builds", "en")).toBe("/en/builds");
  });
  it("inserts locale when missing", () => {
    expect(localizedPath("/civs/britons", "tr")).toBe("/tr/civs/britons");
    expect(localizedPath("civs", "en")).toBe("/en/civs");
  });
});

describe("allLocalePaths", () => {
  it("returns paths for all 2 locales", () => {
    const paths = allLocalePaths("/civs/britons");
    expect(paths).toHaveLength(2);
    expect(paths.find((p) => p.locale === "en")?.path).toBe("/en/civs/britons");
    expect(paths.find((p) => p.locale === "tr")?.path).toBe("/tr/civs/britons");
  });
});
