#!/usr/bin/env node
// scripts/new-guide.mjs
// Usage: node scripts/new-guide.mjs <type> <slug>
// Creates src/content/<type>/{en,tr,es,de}/<slug>.md with empty frontmatter scaffolds.

import { writeFile, mkdir, access } from "node:fs/promises";
import path from "node:path";

const TEMPLATES = {
  civilizations: ({ slug, lang }) => `---
slug: ${slug}
name: ""
tagline: ""
bonuses: []
teamBonus: ""
uniqueTechs:
  castle: { name: "", effect: "" }
  imperial: { name: "", effect: "" }
---

<!-- ${lang.toUpperCase()} body for ${slug} -->
`,
  "build-orders": ({ slug, lang }) => `---
slug: ${slug}
name: ""
difficulty: intermediate
targetAge: feudal
durationMin: 14
civsRecommended: []
steps: []
source:
  author: ""
  url: ""
---

<!-- ${lang.toUpperCase()} body for ${slug} -->
`,
  units: ({ slug, lang }) => `---
slug: ${slug}
name: ""
role: ""
---

<!-- ${lang.toUpperCase()} body for ${slug} -->
`,
  maps: ({ slug, lang }) => `---
slug: ${slug}
name: ""
type: open
recommendedCivs: []
---

<!-- ${lang.toUpperCase()} body for ${slug} -->
`,
  matchups: ({ slug, lang }) => `---
slug: ${slug}
civA: ""
civB: ""
difficulty: even
---

<!-- ${lang.toUpperCase()} body for ${slug} -->
`,
  beginner: ({ slug, lang }) => `---
slug: ${slug}
title: ""
order: 1
---

<!-- ${lang.toUpperCase()} body for ${slug} -->
`,
  glossary: ({ slug, lang }) => `---
slug: ${slug}
term: ""
letter: ""
---

<!-- ${lang.toUpperCase()} body for ${slug} -->
`,
};

async function exists(p) {
  try {
    await access(p);
    return true;
  } catch {
    return false;
  }
}

async function run() {
  const [, , type, slug] = process.argv;
  if (!type || !slug) {
    console.error("Usage: new-guide.mjs <type> <slug>");
    console.error("Types:", Object.keys(TEMPLATES).join(", "));
    process.exit(1);
  }
  if (!TEMPLATES[type]) {
    console.error("Unknown type:", type);
    console.error("Supported:", Object.keys(TEMPLATES).join(", "));
    process.exit(1);
  }
  for (const lang of ["en", "tr", "es", "de"]) {
    const out = path.resolve("src/content", type, lang, `${slug}.md`);
    if (await exists(out)) {
      console.log("skip (exists):", out);
      continue;
    }
    await mkdir(path.dirname(out), { recursive: true });
    await writeFile(out, TEMPLATES[type]({ slug, lang }));
    console.log("created:", out);
  }
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
