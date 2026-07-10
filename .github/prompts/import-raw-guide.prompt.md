---
name: "Import Raw Guide"
description: "Promote a raw md guide into schema-valid EN content and prepare translation scaffolding."
agent: "Content Curator"
tools: [read, search, edit, execute]
argument-hint: "guide-sources/<type>/<file>.md"
---

Import the provided raw guide into the Astro content pipeline.

1. Read the raw `guide-sources/<type>/<file>.md` source and the relevant schema in `src/content/config.ts`.
2. Run or recommend `pnpm import:md <path>` if the scaffold does not exist.
3. Fill the EN content entry with schema-valid frontmatter and useful guide structure.
4. Create or update the TR scaffold only when requested or when the workflow requires it.
5. Run `pnpm check`, `pnpm validate:icons`, and `pnpm check:translations` when applicable.

Return changed files, validation results, and translation gaps.