---
name: "Validate Release"
description: "Run the local release-readiness checks for icons, types, tests, build output, translations, and Cloudflare static deployment assumptions."
agent: "Cloudflare Launch Checker"
tools: [read, search, execute]
argument-hint: "Optional release focus"
---

Validate the project for a release or Cloudflare Pages deployment.

Run and summarize:

1. `pnpm validate:icons`
2. `pnpm check`
3. `pnpm test`
4. `pnpm check:translations`
5. `pnpm build`

Then inspect deployment assumptions in `astro.config.mjs` and `docs/deployment.md`.

Return pass/fail status, blockers, warnings, and next action.