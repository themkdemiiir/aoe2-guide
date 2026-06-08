import { defineCollection, z } from "astro:content";
import { file, glob } from "astro/loaders";

/** Force path-based IDs (e.g. "en/britons") instead of using frontmatter slug. */
function pathId({ entry }: { entry: string }): string {
  return entry.replace(/\.(md|mdx)$/i, "");
}

const civilizations = defineCollection({
  loader: glob({
    pattern: "**/*.{md,mdx}",
    base: "./src/content/civilizations",
    generateId: pathId,
  }),
  schema: z.object({
    slug: z.string(),
    name: z.string(),
    tagline: z.string(),
    bonuses: z.array(z.string()),
    teamBonus: z.string(),
    uniqueTechs: z.object({
      castle: z.object({ name: z.string(), effect: z.string() }),
      imperial: z.object({ name: z.string(), effect: z.string() }),
    }),
  }),
});

const buildOrders = defineCollection({
  loader: glob({
    pattern: "**/*.{md,mdx}",
    base: "./src/content/build-orders",
    generateId: pathId,
  }),
  schema: z.object({
    slug: z.string(),
    name: z.string(),
    difficulty: z.enum(["beginner", "intermediate", "advanced"]),
    targetAge: z.enum(["feudal", "castle", "imperial"]),
    durationMin: z.number(),
    civsRecommended: z.array(z.string()),
    steps: z.array(
      z.object({
        villagers: z.number(),
        time: z.string().optional(),
        assign: z.string(),
        note: z.string().optional(),
        icons: z.array(z.string()).optional(),
      }),
    ),
    source: z.object({
      author: z.string(),
      url: z.string().url().optional(),
    }),
  }),
});

const units = defineCollection({
  loader: glob({ pattern: "**/*.{md,mdx}", base: "./src/content/units", generateId: pathId }),
  schema: z.object({
    slug: z.string(),
    name: z.string(),
    role: z.string(),
    civ: z.string().optional(),
    line: z.string().optional(),
    lineRank: z.number().optional(),
  }),
});

const teamCompSlot = z
  .object({
    civs: z.array(z.string()),
    strategy: z.string(),
  })
  .optional();

const maps = defineCollection({
  loader: glob({ pattern: "**/*.{md,mdx}", base: "./src/content/maps", generateId: pathId }),
  schema: z.object({
    slug: z.string(),
    name: z.string(),
    type: z.enum(["open", "closed", "hybrid", "water", "nomad"]),
    size: z.enum(["tiny", "small", "medium", "large"]).optional(),
    recommendedCivs: z.array(z.string()),
    teamComps: z
      .object({
        "2v2": z.object({ flank: teamCompSlot, pocket: teamCompSlot }).optional(),
        "4v4": z.object({ flank: teamCompSlot, pocket: teamCompSlot }).optional(),
      })
      .optional(),
  }),
});

const beginner = defineCollection({
  loader: glob({ pattern: "**/*.{md,mdx}", base: "./src/content/beginner", generateId: pathId }),
  schema: z.object({
    slug: z.string(),
    title: z.string(),
    order: z.number(),
    prereq: z.array(z.string()).optional(),
  }),
});

const glossary = defineCollection({
  loader: glob({ pattern: "**/*.{md,mdx}", base: "./src/content/glossary", generateId: pathId }),
  schema: z.object({
    slug: z.string(),
    term: z.string(),
    letter: z.string(),
  }),
});

const articles = defineCollection({
  loader: glob({ pattern: "**/*.{md,mdx}", base: "./src/content/articles", generateId: pathId }),
  schema: z.object({
    slug: z.string(),
    title: z.string(),
    description: z.string(),
    category: z.enum(["fundamentals", "strategy", "counters", "meta", "resources"]),
    order: z.number().default(0),
    updated: z.string().optional(),
    heroIcon: z.string().optional(),
    sources: z.array(z.object({ label: z.string(), url: z.string().url() })).default([]),
    draft: z.boolean().default(false),
  }),
});

/* Data collections — sourced from src/data/*.json with Zod validation. */
/* JSON file shapes are preserved (build scripts write to them unchanged).   */
/* Each loader reshapes the JSON into the `[{id, ...}, ...]` form that the   */
/* Content Layer API expects.                                                */

const civData = defineCollection({
  loader: file("src/data/civilizations.json", {
    parser: (text) => {
      const raw = JSON.parse(text) as { civs: Array<Record<string, unknown>> };
      return raw.civs.map((c) => ({ id: c.slug as string, ...c }));
    },
  }),
  schema: z.object({
    slug: z.string(),
    region: z.string().optional(),
    specialty: z.string().optional(),
    uniqueUnits: z.array(z.string()).default([]),
    civBonuses: z.array(z.string()).default([]),
    teamBonus: z.string().optional().default(""),
    uniqueTechs: z
      .object({
        castle: z.object({ name: z.string(), effect: z.string() }),
        imperial: z.object({ name: z.string(), effect: z.string() }),
      })
      .optional(),
    meta: z
      .object({
        tier: z.string().nullable().optional(),
        winRate: z.number().nullable().optional(),
        playRate: z.number().nullable().optional(),
        sampleSize: z.number().nullable().optional(),
        snapshotDate: z.string().nullable().optional(),
      })
      .nullable()
      .optional(),
  }),
});

const unitStats = defineCollection({
  loader: file("src/data/unit-stats.json", {
    parser: (text) => {
      const raw = JSON.parse(text) as { units: Array<Record<string, unknown>> };
      return raw.units.map((u) => ({ id: u.slug as string, ...u }));
    },
  }),
  schema: z.object({
    slug: z.string(),
    hp: z.number(),
    attack: z.number(),
    range: z.number().default(0),
    minRange: z.number().default(0),
    cost: z.object({
      food: z.number().default(0),
      wood: z.number().default(0),
      gold: z.number().default(0),
      stone: z.number().default(0),
    }),
    trainTime: z.number().optional(),
    armorMelee: z.number().optional(),
    armorPiercing: z.number().optional(),
    line: z.string().optional(),
    lineRank: z.number().optional(),
  }),
});

const unitLines = defineCollection({
  loader: file("src/data/unit-lines.json", {
    parser: (text) => {
      const raw = JSON.parse(text) as Record<string, string[]>;
      return Object.entries(raw).map(([slug, members]) => ({ id: slug, slug, members }));
    },
  }),
  schema: z.object({
    slug: z.string(),
    members: z.array(z.string()),
  }),
});

export const collections = {
  civilizations,
  "build-orders": buildOrders,
  units,
  maps,
  beginner,
  glossary,
  articles,
  "civ-data": civData,
  "unit-stats": unitStats,
  "unit-lines": unitLines,
};
