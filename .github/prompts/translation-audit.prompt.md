---
name: "Translation Audit"
description: "Audit EN/TR content coverage and translation workflow status without changing unrelated files."
agent: "Content Curator"
tools: [read, search, execute]
argument-hint: "Collection or slug"
---

Audit translation coverage for the requested collection or slug.

1. Run `pnpm check:translations` or inspect the relevant `src/content/<type>/{en,tr}/` files.
2. Confirm EN and TR share the same canonical slug.
3. Identify missing TR files, stale scaffolds, and schema-breaking frontmatter.
4. Do not machine-translate large content unless explicitly requested.

Return coverage status, missing files, and suggested next edits.