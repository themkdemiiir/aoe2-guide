import { describe, expect, it } from "vitest";
import { breakdown, type Cube, sliceWinRate } from "./cube-query";

const cube: Cube = {
  civs: ["franks", "britons"],
  elos: ["<1000", "1650-1799", "1800-1999"],
  maps: ["arabia", "arena"],
  months: [
    { patch: "100", label: "P100" },
    { patch: "101", label: "P101" },
  ],
  // [civIdx, eloIdx, mapIdx, patchIdx, games, wins]
  rows: [
    [0, 0, 0, 0, 100, 50], // franks <1000 arabia P100 -> 50%
    [0, 1, 0, 0, 100, 60], // franks 1650  arabia P100 -> 60%
    [0, 2, 0, 0, 100, 70], // franks 1800  arabia P100 -> 70%
    [0, 1, 1, 0, 40, 24], // franks 1650  arena  P100 -> 60%
    [0, 1, 0, 1, 100, 55], // franks 1650  arabia P101 -> 55%
    [1, 1, 0, 0, 100, 45], // britons 1650 arabia P100 -> 45%
  ],
};

describe("sliceWinRate", () => {
  it("sums a single civ×map×elo×patch slice", () => {
    expect(sliceWinRate(cube, { civ: "franks", map: "arabia", elo: "1650-1799", patch: "100" })).toEqual({ games: 100, wins: 60, wr: 60 });
  });
  it("any-elo pools every band for the fixed map/patch", () => {
    expect(sliceWinRate(cube, { civ: "franks", map: "arabia", patch: "100" })).toEqual({ games: 300, wins: 180, wr: 60 });
  });
  it("cumulative 1650+ sums only bands ≥ the edge", () => {
    expect(sliceWinRate(cube, { civ: "franks", map: "arabia", patch: "100", elo: "cum:1650" })).toEqual({ games: 200, wins: 130, wr: 65 });
  });
  it("returns null win rate for an absent civ or an empty slice", () => {
    expect(sliceWinRate(cube, { civ: "aztecs" }).wr).toBeNull();
    expect(sliceWinRate(cube, { civ: "britons", map: "arena" }).wr).toBeNull();
  });
  it("a specified-but-absent map is no data, never a silent widen", () => {
    expect(sliceWinRate(cube, { civ: "franks", map: "nomad" })).toEqual({ games: 0, wins: 0, wr: null });
  });
});

describe("breakdown", () => {
  it("by elo returns one row per band in natural order", () => {
    const rows = breakdown(cube, { civ: "franks", map: "arabia", patch: "100" }, "elo");
    expect(rows.map((r) => [r.label, r.wr])).toEqual([
      ["<1000", 50],
      ["1650-1799", 60],
      ["1800-1999", 70],
    ]);
  });
  it("by map honors the fixed elo/patch and sorts by win rate desc", () => {
    const rows = breakdown(cube, { civ: "franks", elo: "1650-1799", patch: "100" }, "map");
    expect(rows.map((r) => r.label).sort()).toEqual(["arabia", "arena"]);
    expect(rows.every((r) => r.wr === 60)).toBe(true);
  });
  it("by patch honors the fixed elo/map", () => {
    const rows = breakdown(cube, { civ: "franks", map: "arabia", elo: "1650-1799" }, "patch");
    expect(rows.map((r) => [r.label, r.wr])).toEqual([
      ["P100", 60],
      ["P101", 55],
    ]);
  });
});
