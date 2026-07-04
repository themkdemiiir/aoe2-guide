import { readdirSync, readFileSync } from "node:fs";
import yaml from "js-yaml";
import { describe, expect, it } from "vitest";
import { roleTr } from "../scripts/lib/role-tr.mjs";

describe("roleTr", () => {
  it("translates a known role", () => {
    expect(roleTr("ranged infantry")).toBe("menzilli piyade");
    expect(roleTr("heavy cavalry")).toBe("ağır süvari");
  });

  it("covers every role.en present in the unit YAML", () => {
    const dir = "src/content/units";
    const roles = new Set();
    for (const f of readdirSync(dir).filter((x) => x.endsWith(".yaml"))) {
      const d = yaml.load(readFileSync(`${dir}/${f}`, "utf8"));
      if (d.role?.en) roles.add(d.role.en);
    }
    for (const r of roles) expect(() => roleTr(r)).not.toThrow();
  });
});
