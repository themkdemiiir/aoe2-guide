---
name: "Cloudflare Preview Smoke"
description: "Smoke test local preview or Cloudflare Pages preview routes for static output, language routing, search assets, and obvious rendering problems."
agent: "Visual Regression Checker"
tools: [read, search, execute, "playwright/*"]
argument-hint: "Preview URL or local route"
---

Smoke test the provided local or Cloudflare Pages preview URL.

1. Confirm whether the target is `pnpm dev`, `pnpm preview`, or a Cloudflare preview URL.
2. Check representative EN and TR routes.
3. Look for console errors, missing styles, layout overlap, broken navigation, placeholder icons, and Pagefind/search asset issues.
4. Do not suggest SSR or Cloudflare adapters for static-site issues.

Return routes checked, visible failures, console/network issues, and launch-blocking status.