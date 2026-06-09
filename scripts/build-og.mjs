#!/usr/bin/env node
// Build-time Open Graph images (FEATURE-IDEAS §3a): one branded card per content
// detail page (civ / build / unit / map) + a default. satori (HTML→SVG) + resvg
// (SVG→PNG), pure Node at build — no Worker/runtime cost. Output → public/og/**
// (gitignored, regenerated each build, copied into dist by Astro). BaseLayout points
// og:image at /og/<seg>/<slug>.png. Language-agnostic (the display name is English).

import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { Resvg } from "@resvg/resvg-js";
import satori from "satori";

const FONT = (p) => readFileSync(`node_modules/@fontsource/${p}`);
const FONTS = [
  {
    name: "Inter",
    weight: 400,
    style: "normal",
    data: FONT("inter/files/inter-latin-400-normal.woff"),
  },
  {
    name: "Inter",
    weight: 700,
    style: "normal",
    data: FONT("inter/files/inter-latin-700-normal.woff"),
  },
  {
    name: "Cinzel",
    weight: 600,
    style: "normal",
    data: FONT("cinzel/files/cinzel-latin-600-normal.woff"),
  },
];

const TYPES = [
  { dir: "civilizations", seg: "civs", label: "Civilization" },
  { dir: "build-orders", seg: "builds", label: "Build Order" },
  { dir: "units", seg: "units", label: "Unit" },
  { dir: "maps", seg: "maps", label: "Map" },
];

const OUT = "public/og";
const PARCHMENT = "#ece0c6";
const INK = "#2c2620";
const GOLD = "#b08a36";

const nameOf = (file) => {
  const m = readFileSync(file, "utf8").match(/^name:\s*"?(.+?)"?\s*$/m);
  return m ? m[1] : null;
};

const node = (style, children) => ({ type: "div", props: { style, children } });

const card = (title, label) =>
  node(
    {
      display: "flex",
      flexDirection: "column",
      justifyContent: "space-between",
      width: "100%",
      height: "100%",
      padding: "72px",
      backgroundColor: PARCHMENT,
      borderTop: `16px solid ${GOLD}`,
      fontFamily: "Inter",
    },
    [
      node(
        { display: "flex", fontSize: 30, color: GOLD, fontWeight: 700, letterSpacing: 3 },
        label.toUpperCase(),
      ),
      node(
        {
          display: "flex",
          fontSize: title.length > 22 ? 76 : 96,
          color: INK,
          fontFamily: "Cinzel",
          fontWeight: 600,
          lineHeight: 1.05,
        },
        title,
      ),
      node(
        { display: "flex", fontSize: 34, color: INK, fontWeight: 700 },
        "AOE2 Guide · aoe2guide.com",
      ),
    ],
  );

async function render(title, label) {
  const svg = await satori(card(title, label), { width: 1200, height: 630, fonts: FONTS });
  return new Resvg(svg, { background: PARCHMENT }).render().asPng();
}

let count = 0;
mkdirSync(OUT, { recursive: true });
writeFileSync(
  join(OUT, "default.png"),
  await render("Age of Empires II Guide", "Strategy · Civs · Builds"),
);
count++;
for (const { dir, seg, label } of TYPES) {
  const enDir = `src/content/${dir}/en`;
  if (!existsSync(enDir)) continue;
  mkdirSync(join(OUT, seg), { recursive: true });
  for (const f of readdirSync(enDir).filter((x) => x.endsWith(".md"))) {
    const slug = f.replace(/\.md$/, "");
    writeFileSync(
      join(OUT, seg, `${slug}.png`),
      await render(nameOf(join(enDir, f)) ?? slug, label),
    );
    count++;
  }
}
console.log(`build:og — generated ${count} OG images`);
