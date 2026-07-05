// Generate public/llms.txt from the content collections so it can never drift
// from reality (the hand-maintained version already claimed 46 build orders when
// there were 43). Runs in `prebuild`. Every line is source-derived — this script
// fails loud on a missing required field rather than emitting a placeholder.
//
// llms.txt is the emerging convention for giving AI crawlers a concise, curated
// map of a site (https://llmstxt.org). We front-load the two flagship tools
// (analyzer, explorer) and the build orders, then enumerate the rest.

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import yaml from "js-yaml";

const CONTENT = "src/content";

/** Fail loud on a missing required field — never emit a fabricated/placeholder value. */
function req(val, what) {
  if (val == null || val === "") throw new Error(`build-llms: missing required field — ${what}`);
  return val;
}

/** Load every YAML entry in a content type, sorted by slug for a stable diff. */
function loadYaml(type) {
  const dir = path.resolve(CONTENT, type);
  return readdirSync(dir)
    .filter((f) => /\.ya?ml$/.test(f))
    .sort()
    .map((f) => ({
      slug: f.replace(/\.ya?ml$/, ""),
      data: yaml.load(readFileSync(path.join(dir, f), "utf8")),
    }));
}

/** Load MD frontmatter for the per-locale long-form types (beginner, articles). */
function loadMd(type) {
  const dir = path.resolve(CONTENT, type, "en");
  let files = [];
  try {
    files = readdirSync(dir).filter((f) => /\.mdx?$/.test(f));
  } catch {
    return [];
  }
  return files.map((f) => {
    const raw = readFileSync(path.join(dir, f), "utf8");
    const m = raw.match(/^---\n([\s\S]*?)\n---/);
    return { slug: f.replace(/\.mdx?$/, ""), data: m ? yaml.load(m[1]) : {} };
  });
}

/** Strip markdown to clean plain text for a one-line description. */
const stripMd = (s) =>
  String(s)
    .replace(/\[\[([^\]]+)\]\]/g, "$1") // [[wikilink]]
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1") // [text](url)
    .replace(/[*_`>#]/g, "")
    .replace(/\s+/g, " ")
    .trim();

/** First sentence, whitespace-collapsed, capped so lines stay scannable. */
function oneline(s, max = 170) {
  const clean = stripMd(s);
  const sentence = clean.match(/^.*?[.!?](\s|$)/)?.[0].trim() ?? clean;
  return sentence.length > max ? `${clean.slice(0, max).trim()}…` : sentence;
}

/** Drop a leading "Name — " / "Name: " prefix so it doesn't echo the link slug. */
const stripNamePrefix = (s) =>
  String(s)
    .replace(/^[^—:-]+\s*[—:-]\s*/, "")
    .trim();
const cap = (s) => (s ? s[0].toUpperCase() + s.slice(1) : s);

const civs = loadYaml("civilizations");
const builds = loadYaml("build-orders");
const maps = loadYaml("maps");
const units = loadYaml("units");
const glossary = loadYaml("glossary");
const learn = loadMd("beginner").sort((a, b) => (a.data.order ?? 0) - (b.data.order ?? 0));
const blog = loadMd("articles")
  .filter((a) => !a.data.draft)
  .sort((a, b) => (a.data.order ?? 0) - (b.data.order ?? 0));

const DIFF_RANK = { beginner: 0, intermediate: 1, advanced: 2 };
const byName = (a, b) =>
  req(a.data.name?.en, `name for ${a.slug}`).localeCompare(
    req(b.data.name?.en, `name for ${b.slug}`),
  );

const lines = [];
const push = (s = "") => lines.push(s);

// --- Header ---------------------------------------------------------------
push("# AOE2 Guide");
push();
push(
  `> Free, bilingual (English/Turkish) Age of Empires II: Definitive Edition strategy reference covering all ${civs.length} civilizations, ${builds.length} build orders, ${maps.length} maps, ${units.length} units, and ${glossary.length} glossary terms — plus a free in-browser replay analyzer.`,
);
push();
push(
  "AOE2 Guide provides step-by-step build orders with villager timings, per-civilization strategy, map guides, unit counters, and a WebAssembly replay coach that runs entirely client-side. Every page is available in English (`/en/…`) and Turkish (`/tr/…`); the URLs below show the English path.",
);
push();

// --- Tools (flagship, front-loaded) --------------------------------------
push("## Tools");
push(
  "- [Replay Analyzer](/en/analyzer) — Upload an AoE2 `.aoe2record` replay for instant coaching: build-order timing, eco-upgrade and first-military timings measured against winner benchmarks by Elo bracket. Runs 100% in-browser via WebAssembly — no replay is ever uploaded to a server.",
);
push(
  "- [Data Explorer](/en/explorer) — Build custom civilization win-rate queries (by civ, map, Elo bracket, and patch) over ranked 1v1 ladder data, charted client-side.",
);
push();

// --- Build orders (flagship) ---------------------------------------------
push("## Build Orders");
push("Step-by-step villager-assignment guides with population counts and in-game timings.");
for (const b of [...builds].sort(
  (a, b) => DIFF_RANK[a.data.difficulty] - DIFF_RANK[b.data.difficulty] || byName(a, b),
)) {
  const name = req(b.data.name?.en, `build name for ${b.slug}`);
  const diff = req(b.data.difficulty, `difficulty for ${b.slug}`);
  const age = req(b.data.targetAge, `targetAge for ${b.slug}`);
  const dur = req(b.data.durationMin, `durationMin for ${b.slug}`);
  const gist = b.data.intro?.en ? ` ${oneline(b.data.intro.en)}` : "";
  push(`- [${name}](/en/builds/${b.slug}) — ${diff}, ${age} ~${dur}min.${gist}`);
}
push();

// --- Civilizations --------------------------------------------------------
push("## Civilizations");
push(
  `All ${civs.length} AoE2 DE civilizations — bonuses, team bonus, unique units and techs, and strategy.`,
);
for (const c of [...civs].sort(byName)) {
  const name = req(c.data.name?.en, `civ name for ${c.slug}`);
  const tagline = stripNamePrefix(req(c.data.tagline?.en, `tagline for ${c.slug}`));
  push(`- [${name}](/en/civs/${c.slug}) — ${cap(tagline)}`);
}
push();

// --- Maps -----------------------------------------------------------------
push("## Maps");
for (const m of [...maps].sort(byName)) {
  const name = req(m.data.name?.en, `map name for ${m.slug}`);
  const type = req(m.data.type, `type for ${m.slug}`);
  const gist = m.data.body?.en ? ` ${oneline(m.data.body.en)}` : "";
  push(`- [${name}](/en/maps/${m.slug}) — ${cap(type)} map.${gist}`);
}
push();

// --- Units ----------------------------------------------------------------
push("## Units");
push("Stats, costs, and role for every standard and unique unit.");
for (const u of [...units].sort(byName)) {
  const name = req(u.data.name?.en, `unit name for ${u.slug}`);
  const role = req(u.data.role?.en, `role for ${u.slug}`);
  push(`- [${name}](/en/units/${u.slug}) — ${role}`);
}
push();

// --- Learn (beginner course) ---------------------------------------------
if (learn.length) {
  push("## Learn (beginner course)");
  for (const c of learn) {
    const title = req(c.data.title, `title for beginner/${c.slug}`);
    const desc = c.data.description ? ` ${oneline(c.data.description)}` : "";
    push(`- [${title}](/en/learn/${c.slug}) —${desc}`);
  }
  push();
}

// --- Blog / articles ------------------------------------------------------
if (blog.length) {
  push("## Articles");
  for (const a of blog) {
    const title = req(a.data.title, `title for articles/${a.slug}`);
    const desc = a.data.description ? ` ${oneline(a.data.description)}` : "";
    push(`- [${title}](/en/blog/${a.slug}) —${desc}`);
  }
  push();
}

// --- Counters + Glossary --------------------------------------------------
push("## Counters");
push("- [Unit counter chart](/en/counters) — What beats what, by age.");
push();

push("## Glossary");
push(`- [Full glossary](/en/glossary) — ${glossary.length} AoE2 terms defined.`);
for (const g of [...glossary].sort((a, b) =>
  req(a.data.term?.en, `term for ${a.slug}`).localeCompare(b.data.term.en),
)) {
  const term = req(g.data.term?.en, `term for ${g.slug}`);
  push(`  - ${term}: ${oneline(req(g.data.definition?.en, `definition for ${g.slug}`))}`);
}
push();

const out = `${lines
  .join("\n")
  .replace(/\n{3,}/g, "\n\n")
  .trimEnd()}\n`;
writeFileSync(path.resolve("public/llms.txt"), out, "utf8");
console.log(
  `build-llms: wrote public/llms.txt (${civs.length} civs, ${builds.length} builds, ${maps.length} maps, ${units.length} units, ${glossary.length} glossary, ${learn.length} learn, ${blog.length} articles)`,
);
