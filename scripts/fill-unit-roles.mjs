import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import yaml from "js-yaml";
import { roleTr } from "./lib/role-tr.mjs";

const DIR = path.resolve("src/content/units");
let changed = 0;
for (const f of readdirSync(DIR).filter((x) => x.endsWith(".yaml"))) {
  const p = path.join(DIR, f);
  const d = yaml.load(readFileSync(p, "utf8"));
  if (!d.role?.en) continue;
  const want = roleTr(d.role.en);
  if (d.role.tr !== want) {
    d.role.tr = want;
    writeFileSync(p, yaml.dump(d, { lineWidth: 120 }), "utf8");
    changed++;
  }
}
console.log(`fill-unit-roles: updated ${changed} files`);
