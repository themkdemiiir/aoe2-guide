# AI Development Setup

This project is configured for GitHub Copilot in VS Code and Claude Code. The goal is to make agents productive without weakening the static Astro, content-schema, icon, and Cloudflare Pages constraints.

## What Is Shared In The Repo

- `.github/copilot-instructions.md` - project-wide Copilot instructions.
- `AGENTS.md` - cross-agent project contract for tools that read AGENTS files.
- `.github/instructions/*.instructions.md` - scoped Copilot instructions for content, Astro/UI, scripts, and data.
- `.github/agents/*.agent.md` - focused custom agents for curation, validation, launch, research, and visual checks.
- `.github/prompts/*.prompt.md` - reusable task prompts for common workflows.
- `.vscode/extensions.json` - recommended language server and tooling extensions.
- `.vscode/settings.json` - workspace editor, Tailwind, Copilot customization, and MCP behavior settings.
- `.vscode/tasks.json` - task shortcuts for the main pnpm validation commands.
- `.vscode/mcp.json` - VS Code MCP server definitions.
- `.mcp.json` - Claude Code project-scoped MCP server definitions.

## MCP Servers

The shared MCP configs declare these servers:

| Server | Purpose | Notes |
|---|---|---|
| GitHub | Issues, PRs, Actions, repository context | Uses GitHub remote MCP and host OAuth/PAT flow. Do not commit tokens. |
| Context7 | Current framework and library docs | API key may be configured per user for higher limits. |
| Cloudflare Docs | Current Cloudflare documentation | Default Cloudflare MCP for this static Pages site. |
| Playwright | Browser route and preview checks | Configured isolated/headless for repeatable smoke tests. |
| Sequential Thinking | Planning complex multi-step tasks | Local stdio server; useful for migrations and validation design. |

Cloudflare account-wide or API-mutating MCPs are intentionally not enabled by default. This site currently deploys as static Pages output, so documentation context is the safe default.

## Security Rules

- Never commit API keys, PATs, OAuth tokens, browser profiles, `.env.local`, MCP auth caches, or Cloudflare account secrets.
- Review and approve project MCP servers in VS Code or Claude Code before use.
- Keep `chat.mcp.autoStart` disabled unless you intentionally want automatic startup.
- Treat local stdio MCP servers as code execution on your machine. Only run trusted packages.
- Use Cloudflare account-management MCPs only when a task explicitly requires deployment/account inspection.

## Common Workflows

### Content Import

Use the Content Curator agent or `/import-raw-guide` prompt.

1. Add a raw guide under `md/<type>/<source>-<topic>.md`.
2. Run `pnpm import:md md/<type>/<file>.md`.
3. Fill the generated EN entry using [src/content/config.ts](../src/content/config.ts).
4. Run `pnpm new:guide <type> <slug>` if a TR scaffold is needed.
5. Validate with `pnpm check`, `pnpm validate:icons`, and `pnpm check:translations`.

### Icon Or Unit Mismatch

Use the Data Integrity Validator agent or `/fix-icon-unit-mismatch` prompt.

1. Reproduce with `pnpm validate:icons` or the relevant data script.
2. Trace the slug through content, `icon-map.json`, unit stats, unit lines, and civ data.
3. Fix the controlling source or validator.
4. Run the same reproduction command again, then `pnpm build` if page output is affected.

### Cloudflare Launch

Use the Cloudflare Launch Checker agent or `/validate-release` prompt.

1. Confirm static Astro output and production `site` in [astro.config.mjs](../astro.config.mjs).
2. Run `pnpm validate:icons`, `pnpm check`, `pnpm test`, and `pnpm build`.
3. Confirm Cloudflare Pages uses build command `pnpm install --frozen-lockfile && pnpm build` and output directory `dist`.
4. Smoke test representative EN and TR routes in local preview or Pages preview.

## Recommended Local Checks

Run these before declaring a change launch-ready:

```sh
pnpm validate:icons
pnpm check
pnpm test
pnpm build
```

For translation-sensitive changes, add:

```sh
pnpm check:translations
```

For editor/task access, use VS Code task `pnpm: release baseline`.

## Frontmatter Intelligence

The repo does not yet generate JSON schemas from `src/content/config.ts`. If frontmatter autocomplete becomes a priority, add a schema generation step first, then wire the generated schemas into VS Code/YAML tooling. Do not duplicate schema truth manually without a maintenance plan.