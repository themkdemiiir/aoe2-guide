import { defineCollection, z } from "astro:content";
import { file, glob } from "astro/loaders";

/** Force path-based IDs (e.g. "en/britons") instead of using frontmatter slug. */
function pathId({ entry }: { entry: string }): string {
  return entry.replace(/\.(md|mdx)$/i, "");
}

const localizedString = z.object({ en: z.string(), tr: z.string() });

const civilizations = defineCollection({
  loader: glob({
    pattern: "*.{yaml,yml}",
    base: "./src/content/civilizations",
    generateId: ({ entry }) => entry.replace(/\.(yaml|yml)$/i, ""),
  }),
  schema: z.object({
    slug: z.string(),
    name: localizedString,
    tagline: localizedString,
    bonuses: z.object({ en: z.array(z.string()), tr: z.array(z.string()) }),
    teamBonus: localizedString,
    uniqueTechs: z.object({
      castle: z.object({ name: localizedString, effect: localizedString }),
      imperial: z.object({ name: localizedString, effect: localizedString }),
    }),
    strategy: z.object({ en: z.string(), tr: z.string() }).optional(),
  }),
});

const buildOrders = defineCollection({
  loader: glob({
    pattern: "*.{yaml,yml}",
    base: "./src/content/build-orders",
    generateId: ({ entry }) => entry.replace(/\.(yaml|yml)$/i, ""),
  }),
  schema: z.object({
    slug: z.string(),
    name: localizedString,
    difficulty: z.enum(["beginner", "intermediate", "advanced"]),
    targetAge: z.enum(["feudal", "castle", "imperial"]),
    durationMin: z.number(),
    civsRecommended: z.array(z.string()),
    steps: z
      .array(
        z.object({
          villagers: z.number().optional(),
          phase: z.enum(["feudal", "castle", "imperial"]).optional(),
          time: z.string().optional(),
          assign: localizedString,
          note: localizedString.optional(),
          icons: z.array(z.string()).optional(),
          // Cumulative villager split AFTER the step (derived from the verified
          // step texts + Hera transcripts, adversarially re-checked). Optional:
          // builds whose text is genuinely ambiguous ship without columns
          // rather than with guessed numbers.
          resources: z
            .object({
              food: z.number().int().min(0),
              wood: z.number().int().min(0),
              gold: z.number().int().min(0),
              stone: z.number().int().min(0),
              builder: z.number().int().min(0),
            })
            .optional(),
        }),
      )
      .superRefine((steps, ctx) => {
        // The split must reconcile with the audited Vil-Pop rail — fail the build, never render wrong numbers.
        steps.forEach((s, i) => {
          if (s.resources && s.villagers != null) {
            const sum =
              s.resources.food +
              s.resources.wood +
              s.resources.gold +
              s.resources.stone +
              s.resources.builder;
            if (sum !== s.villagers) {
              ctx.addIssue({
                code: z.ZodIssueCode.custom,
                path: [i, "resources"],
                message: `resources sum ${sum} != villagers ${s.villagers}`,
              });
            }
          }
        });
      }),
    intro: localizedString.optional(),
    strategy: z.object({ en: z.array(z.string()), tr: z.array(z.string()) }).optional(),
    source: z
      .object({
        author: z.string(),
        url: z.string().url().optional(),
      })
      .optional(),
  }),
});

const units = defineCollection({
  loader: glob({
    pattern: "*.{yaml,yml}",
    base: "./src/content/units",
    generateId: ({ entry }) => entry.replace(/\.(yaml|yml)$/i, ""),
  }),
  schema: z.object({
    slug: z.string(),
    name: localizedString,
    role: localizedString,
    civ: z.string().optional(),
    line: z.string().optional(),
    lineRank: z.number().optional(),
    description: localizedString.optional(),
    upgrades: localizedString.optional(),
  }),
});

const teamCompSlot = z
  .object({
    civs: z.array(z.string()),
    strategy: localizedString,
  })
  .optional();

const maps = defineCollection({
  loader: glob({
    pattern: "*.{yaml,yml}",
    base: "./src/content/maps",
    generateId: ({ entry }) => entry.replace(/\.(yaml|yml)$/i, ""),
  }),
  schema: z.object({
    slug: z.string(),
    name: localizedString,
    type: z.enum(["open", "closed", "hybrid", "water", "nomad"]),
    size: z.enum(["tiny", "small", "medium", "large"]).optional(),
    recommendedCivs: z.array(z.string()),
    teamComps: z
      .object({
        "2v2": z.object({ flank: teamCompSlot, pocket: teamCompSlot }).optional(),
        "4v4": z.object({ flank: teamCompSlot, pocket: teamCompSlot }).optional(),
      })
      .optional(),
    body: localizedString.optional(),
  }),
});

const beginner = defineCollection({
  loader: glob({ pattern: "**/*.{md,mdx}", base: "./src/content/beginner", generateId: pathId }),
  schema: z.object({
    slug: z.string(),
    title: z.string(),
    description: z.string(), // meta description — required so no chapter ships Google a duplicate snippet
    order: z.number(),
    prereq: z.array(z.string()).optional(),
  }),
});

const glossary = defineCollection({
  loader: glob({
    pattern: "*.{yaml,yml}",
    base: "./src/content/glossary",
    generateId: ({ entry }) => entry.replace(/\.(yaml|yml)$/i, ""),
  }),
  schema: z.object({
    slug: z.string(),
    term: localizedString,
    letter: z.string(),
    definition: localizedString,
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
    published: z.string().optional(), // first-publication date (Article JSON-LD datePublished)
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
