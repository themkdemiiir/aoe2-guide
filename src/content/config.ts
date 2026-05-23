import { defineCollection, z } from "astro:content";
import { glob } from "astro/loaders";

/** Force path-based IDs (e.g. "en/britons") instead of using frontmatter slug. */
function pathId({ entry }: { entry: string }): string {
  return entry.replace(/\.(md|mdx)$/i, "");
}

const civilizations = defineCollection({
  loader: glob({ pattern: "**/*.{md,mdx}", base: "./src/content/civilizations", generateId: pathId }),
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
  loader: glob({ pattern: "**/*.{md,mdx}", base: "./src/content/build-orders" }),
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
        time: z.string(),
        assign: z.string(),
        note: z.string().optional(),
      }),
    ),
    source: z.object({
      author: z.string(),
      url: z.string().url().optional(),
    }),
  }),
});

const units = defineCollection({
  loader: glob({ pattern: "**/*.{md,mdx}", base: "./src/content/units" }),
  schema: z.object({
    slug: z.string(),
    name: z.string(),
    role: z.string(),
    civ: z.string().optional(),
  }),
});

const maps = defineCollection({
  loader: glob({ pattern: "**/*.{md,mdx}", base: "./src/content/maps" }),
  schema: z.object({
    slug: z.string(),
    name: z.string(),
    type: z.enum(["open", "closed", "hybrid", "water", "nomad"]),
    size: z.enum(["tiny", "small", "medium", "large"]).optional(),
    recommendedCivs: z.array(z.string()),
  }),
});

const matchups = defineCollection({
  loader: glob({ pattern: "**/*.{md,mdx}", base: "./src/content/matchups" }),
  schema: z.object({
    slug: z.string(),
    civA: z.string(),
    civB: z.string(),
    difficulty: z.enum(["even", "favored", "unfavored"]),
  }),
});

const beginner = defineCollection({
  loader: glob({ pattern: "**/*.{md,mdx}", base: "./src/content/beginner" }),
  schema: z.object({
    slug: z.string(),
    title: z.string(),
    order: z.number(),
    prereq: z.array(z.string()).optional(),
  }),
});

const glossary = defineCollection({
  loader: glob({ pattern: "**/*.{md,mdx}", base: "./src/content/glossary" }),
  schema: z.object({
    slug: z.string(),
    term: z.string(),
    letter: z.string(),
  }),
});

export const collections = {
  civilizations,
  "build-orders": buildOrders,
  units,
  maps,
  matchups,
  beginner,
  glossary,
};
