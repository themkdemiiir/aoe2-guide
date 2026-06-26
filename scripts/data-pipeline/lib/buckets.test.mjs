import { describe, expect, it } from "vitest";
import { ELO_BUCKETS, ELO_BUCKETS_WITH_ALL, canonMap, eloBucket, eloCaseSql } from "./buckets.mjs";

describe("eloBucket", () => {
  it("returns null for null/undefined (never fabricates a band)", () => {
    expect(eloBucket(null)).toBe(null);
    expect(eloBucket(undefined)).toBe(null);
  });
  it("buckets known ratings", () => {
    expect(eloBucket(0)).toBe("<1000");
    expect(eloBucket(999)).toBe("<1000");
    expect(eloBucket(1000)).toBe("1000-1199");
    expect(eloBucket(2499)).toBe("2200-2499");
    expect(eloBucket(2500)).toBe("2500+");
  });
});

describe("eloCaseSql", () => {
  it("emits all 9 labels and an IS NULL branch", () => {
    const sql = eloCaseSql("p.rating");
    for (const b of ELO_BUCKETS) expect(sql).toContain(`'${b}'`);
    expect(sql).toMatch(/p\.rating IS NULL THEN 'unknown'/);
  });
  it("honors a custom nullLabel", () => {
    expect(eloCaseSql("x", { nullLabel: "na" })).toMatch(/x IS NULL THEN 'na'/);
  });
});

describe("canonMap", () => {
  it("lowercases, strips extension and non-alphanumerics", () => {
    expect(canonMap("Arabia.rms2")).toBe("arabia");
    expect(canonMap("Golden_Pit")).toBe("goldenpit");
    expect(canonMap("African Clearing")).toBe("africanclearing");
  });
});

describe("ELO_BUCKETS_WITH_ALL", () => {
  it("prepends 'all'", () => {
    expect(ELO_BUCKETS_WITH_ALL[0]).toBe("all");
    expect(ELO_BUCKETS_WITH_ALL.slice(1)).toEqual(ELO_BUCKETS);
  });
});
