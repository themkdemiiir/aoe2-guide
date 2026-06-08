#!/usr/bin/env node

// scripts/validate-icons.mjs
//
// Scans all content files in src/content/**/*.{md,mdx} for icon references,
// then asserts every referenced slug exists in src/data/icon-map.json.
//
// Reference forms detected:
//   - <Icon slug="longbowman" kind="unit" />
//   - <Icon slug="britons" kind="civ" />
//   - Frontmatter `icons: [longbowman, archery-range]`
//   - Frontmatter inline `icons:` arrays in YAML
//
// Slugs that are intentionally allowed to render as text fallbacks can be listed
// in KNOWN_MISSING. Unit/Tech icon-map generation uses aoe2techtree
// `picture_index`, so those refs should resolve. Building refs stay as text
// labels because the available Building PNGs are not reliable building artwork.
//
// Exit codes:
//   0 — all references resolve, or only resolve to KNOWN_MISSING
//   1 — at least one reference is unresolvable AND not in KNOWN_MISSING

import { readdir, readFile, stat } from "node:fs/promises";
import path from "node:path";

const CONTENT_ROOT = "src/content";
const ICON_MAP = "src/data/icon-map.json";

// Resources resolved by hardcoded RESOURCE_ICONS in BuildOrderSteps.astro,
// not via icon-map. Always considered valid.
const KNOWN_RESOURCES = new Set(["food", "wood", "gold", "stone"]);

const KNOWN_MISSING = new Set([
  "archery-range",
  "barracks",
  "blacksmith",
  "castle",
  "dock",
  "farm",
  "fortress",
  "house",
  "lumber-camp",
  "market",
  "mill",
  "mining-camp",
  "monastery",
  "siege-workshop",
  "stable",
  "town-center",
  "watch-tower",
]);

async function walkContent(dir) {
  const out = [];
  const entries = await readdir(dir, { withFileTypes: true });
  for (const e of entries) {
    const full = path.join(dir, e.name);
    if (e.isDirectory()) {
      out.push(...(await walkContent(full)));
    } else if (e.isFile() && /\.(md|mdx)$/i.test(e.name)) {
      out.push(full);
    }
  }
  return out;
}

function extractIconRefs(source) {
  const refs = [];

  // <Icon slug="..." kind="..." /> (kind optional, defaults to "unit")
  const iconTagRe =
    /<Icon\b[^>]*?\bslug\s*=\s*["']([^"']+)["'][^>]*?(?:\bkind\s*=\s*["']([^"']+)["'])?[^>]*?\/?>/g;
  for (const m of source.matchAll(iconTagRe)) {
    refs.push({ slug: m[1], kind: m[2] || "unit" });
  }

  // Frontmatter inline arrays like `icons: [foo, bar]` or `- foo`
  // Conservative: only catch list-style icons under a heading like `icons:`
  const frontmatterMatch = source.match(/^---\n([\s\S]+?)\n---/);
  if (frontmatterMatch) {
    const fm = frontmatterMatch[1];
    // inline: icons: [a, b, c]
    const inlineRe = /\bicons:\s*\[([^\]]*)\]/g;
    for (const m of fm.matchAll(inlineRe)) {
      for (const s of m[1].split(",").map((x) => x.trim().replace(/^["']|["']$/g, ""))) {
        if (s) refs.push({ slug: s, kind: "unit" });
      }
    }
  }

  return refs;
}

async function run() {
  // Load icon-map
  const iconMap = JSON.parse(await readFile(ICON_MAP, "utf8"));
  const kindToKey = { unit: "units", tech: "techs", building: "buildings", civ: "civs" };

  // Walk content files
  const stats = await stat(CONTENT_ROOT).catch(() => null);
  if (!stats?.isDirectory()) {
    console.error(`${CONTENT_ROOT} not found`);
    process.exit(1);
  }
  const files = await walkContent(CONTENT_ROOT);

  // Collect references
  const allRefs = []; // [{ file, slug, kind }]
  for (const file of files) {
    const src = await readFile(file, "utf8");
    const refs = extractIconRefs(src);
    for (const r of refs) allRefs.push({ file, ...r });
  }

  // Check each reference. For <Icon kind="..."> use the named bucket strictly.
  // For frontmatter `icons:` arrays (kind defaults to "unit"), search all
  // buckets — they're polymorphic per BuildOrderSteps.astro's resolveIcon().
  const allBuckets = ["units", "techs", "buildings", "civs"];
  const unresolved = [];
  const knownMissing = [];
  for (const { file, slug, kind } of allRefs) {
    if (KNOWN_RESOURCES.has(slug)) continue; // resource icons are hardcoded
    let resolved = false;
    if (kind && kindToKey[kind]) {
      // Explicit <Icon kind="..."> — must match that exact bucket
      const bucket = iconMap[kindToKey[kind]];
      if (bucket?.[slug]) resolved = true;
    }
    if (!resolved) {
      // Polymorphic fallback (frontmatter icons:) — try all buckets
      for (const b of allBuckets) {
        if (iconMap[b]?.[slug]) {
          resolved = true;
          break;
        }
      }
    }
    if (!resolved) {
      if (KNOWN_MISSING.has(slug)) {
        knownMissing.push({ file, slug, kind });
      } else {
        unresolved.push({ file, slug, kind });
      }
    }
  }

  // Report
  console.log(`Scanned ${files.length} content files; ${allRefs.length} icon refs.`);
  console.log(`  resolved:       ${allRefs.length - unresolved.length - knownMissing.length}`);
  console.log(
    `  known-missing:  ${knownMissing.length} (missing or unverified icon, rendered as ?slug placeholder)`,
  );
  console.log(`  UNRESOLVED:     ${unresolved.length}`);

  if (unresolved.length) {
    console.error("\nUnresolved icon references (typos or new slugs needing icons):");
    // Group by slug for compact output
    const bySlug = new Map();
    for (const u of unresolved) {
      if (!bySlug.has(u.slug)) bySlug.set(u.slug, []);
      bySlug.get(u.slug).push(`${u.file} (kind=${u.kind})`);
    }
    for (const [slug, locations] of [...bySlug].sort()) {
      console.error(`  ${slug}:`);
      for (const loc of locations.slice(0, 3)) console.error(`    ${loc}`);
      if (locations.length > 3) console.error(`    ...${locations.length - 3} more`);
    }
    process.exit(1);
  }

  console.log("\nAll icon references resolve.");
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
