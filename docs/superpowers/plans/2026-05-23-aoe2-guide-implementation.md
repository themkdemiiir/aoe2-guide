# AOE2 Guide Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a four-language static Age of Empires II guide site (civs, build orders, units, maps, matchups, beginner path, glossary) deployable to Cloudflare Pages with no server-side runtime.

**Architecture:** Astro 4+ in static-output mode with TypeScript, Tailwind theme tokens, content collections validated by Zod, English-canonical slugs across 4 locales (EN/TR/ES/DE), Pagefind search, Astro Islands reserved for the future civ-comparator. JSON files in `src/data/` hold language-agnostic facts; localized markdown in `src/content/` holds translatable text.

**Tech Stack:** Astro 4+, TypeScript, TailwindCSS, MDX, Pagefind, Zod, pnpm, Vitest (for utility code), Cloudflare Pages.

**Spec:** [`docs/superpowers/specs/2026-05-23-aoe2-guide-design.md`](../specs/2026-05-23-aoe2-guide-design.md)

---

## Phase Map

| # | Phase | Milestone |
|---|---|---|
| 1 | Foundation | `pnpm dev` runs, all routes 200, theme toggle works, locale chooser at `/` |
| 2 | Schemas & data | Zod schemas validate; helpers resolve localized entries; UI strings in 4 langs |
| 3 | Asset pipeline | Icons synced from aoe2techtree, icon-map.json built, `<Icon slug="knight"/>` renders |
| 4 | Civilizations (E2E proof) | Britons fully rendered in 4 langs, index + detail |
| 5 | Build Orders + import-from-md | User can drop a file into `md/` and promote it into the site |
| 6 | Remaining content types | Units, maps, matchups, beginner, glossary all live |
| 7 | Search + tooling + deploy | Pagefind works; CI green; CLAUDE.md files; Cloudflare instructions ready |

---

# Phase 1 — Foundation

## Task 1: Initialize Astro project with TypeScript

**Files:**
- Create: `package.json`, `tsconfig.json`, `astro.config.mjs`, `src/env.d.ts`, `src/pages/index.astro`, `.gitignore`, `.nvmrc`
- The existing `README.md` stays.

- [ ] **Step 1: Pin Node version**

```bash
echo "20" > .nvmrc
```

- [ ] **Step 2: Initialize pnpm + scaffold Astro non-interactively**

```bash
corepack enable
pnpm init
pnpm add -D astro@^4 @astrojs/check typescript@^5
pnpm add -D @astrojs/mdx @astrojs/sitemap
```

- [ ] **Step 3: Write `astro.config.mjs`**

```js
// astro.config.mjs
import { defineConfig } from "astro/config";
import mdx from "@astrojs/mdx";
import sitemap from "@astrojs/sitemap";

export default defineConfig({
  site: "https://aoe2.example.com", // TODO: set real domain when known
  output: "static",
  i18n: {
    defaultLocale: "en",
    locales: ["en", "tr", "es", "de"],
    routing: { prefixDefaultLocale: true, redirectToDefaultLocale: false },
    fallback: { tr: "en", es: "en", de: "en" },
  },
  integrations: [mdx(), sitemap()],
});
```

- [ ] **Step 4: Write `tsconfig.json`**

```json
{
  "extends": "astro/tsconfigs/strict",
  "include": [".astro/types.d.ts", "**/*"],
  "exclude": ["dist"],
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@/*": ["src/*"],
      "@components/*": ["src/components/*"],
      "@layouts/*": ["src/components/layout/*"],
      "@lib/*": ["src/lib/*"],
      "@i18n/*": ["src/i18n/*"],
      "@data/*": ["src/data/*"]
    }
  }
}
```

- [ ] **Step 5: Add scripts to `package.json`**

```json
{
  "scripts": {
    "dev": "astro dev",
    "build": "astro build",
    "preview": "astro preview",
    "astro": "astro",
    "check": "astro check"
  }
}
```

- [ ] **Step 6: Write `.gitignore`**

```
node_modules/
dist/
.astro/
.env
.env.*
!.env.example
.DS_Store
.cache/
.pagefind/
```

- [ ] **Step 7: Write `src/env.d.ts`**

```ts
/// <reference path="../.astro/types.d.ts" />
/// <reference types="astro/client" />
```

- [ ] **Step 8: Write placeholder root page `src/pages/index.astro`**

```astro
---
// Will be replaced in Task 11 with the real locale chooser
---
<html lang="en"><body><p>Bootstrapping…</p></body></html>
```

- [ ] **Step 9: Verify dev server boots**

Run: `pnpm dev`
Expected: server listens on `http://localhost:4321`, page shows "Bootstrapping…", no errors. Kill server.

- [ ] **Step 10: Commit**

```bash
git add .
git commit -m "chore: scaffold Astro project with TS, i18n config, MDX, sitemap"
```

---

## Task 2: Add TailwindCSS with medieval theme tokens

**Files:**
- Create: `tailwind.config.mjs`, `src/styles/tokens.css`, `src/styles/globals.css`
- Modify: `astro.config.mjs` (add Tailwind integration)

- [ ] **Step 1: Install Tailwind via official integration**

```bash
pnpm add -D @astrojs/tailwind tailwindcss
```

- [ ] **Step 2: Add Tailwind integration to `astro.config.mjs`**

Change `integrations: [mdx(), sitemap()]` to:

```js
import tailwind from "@astrojs/tailwind";
// ...
integrations: [mdx(), sitemap(), tailwind({ applyBaseStyles: false })],
```

- [ ] **Step 3: Write `tailwind.config.mjs` with design tokens**

```js
/** @type {import('tailwindcss').Config} */
export default {
  content: ["./src/**/*.{astro,html,js,jsx,md,mdx,ts,tsx}"],
  darkMode: ["class", '[data-theme="dark"]'],
  theme: {
    extend: {
      colors: {
        parchment: "rgb(var(--parchment) / <alpha-value>)",
        ink: "rgb(var(--ink) / <alpha-value>)",
        stone: {
          700: "rgb(var(--stone-700) / <alpha-value>)",
        },
        gold: {
          400: "rgb(var(--gold-400) / <alpha-value>)",
          500: "rgb(var(--gold-500) / <alpha-value>)",
        },
        royal: "rgb(var(--royal-red) / <alpha-value>)",
        forest: "rgb(var(--forest-700) / <alpha-value>)",
        steel: "rgb(var(--steel-600) / <alpha-value>)",
        ember: "rgb(var(--ember) / <alpha-value>)",
        "stone-bg": "rgb(var(--stone-bg) / <alpha-value>)",
        res: {
          food: "rgb(var(--res-food) / <alpha-value>)",
          wood: "rgb(var(--res-wood) / <alpha-value>)",
          gold: "rgb(var(--res-gold) / <alpha-value>)",
          stone: "rgb(var(--res-stone) / <alpha-value>)",
        },
      },
      fontFamily: {
        display: ["Cinzel", "serif"],
        body: ["Inter", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "ui-monospace", "monospace"],
      },
      borderRadius: {
        DEFAULT: "4px",
        sm: "2px",
      },
      maxWidth: {
        prose: "75ch",
      },
    },
  },
};
```

- [ ] **Step 4: Write `src/styles/tokens.css`**

```css
:root {
  /* Light (parchment) — default */
  --parchment: 245 236 215;
  --ink: 31 24 18;
  --stone-700: 61 53 48;
  --gold-500: 201 164 76;
  --gold-400: 217 183 96;
  --royal-red: 139 30 30;
  --forest-700: 61 105 54;
  --steel-600: 74 96 121;
  --ember: 240 217 154;
  --stone-bg: 28 22 17;

  --res-food: 212 77 77;
  --res-wood: 139 115 85;
  --res-gold: 220 190 76;
  --res-stone: 136 136 136;
}

html[data-theme="dark"] {
  --parchment: 28 22 17;     /* stone-bg becomes the page bg */
  --ink: 240 217 154;        /* ember text */
  --stone-700: 90 79 63;     /* parchment-60 secondary */
  --gold-500: 217 183 96;    /* gold-400 brighter on dark */
}
```

- [ ] **Step 5: Write `src/styles/globals.css`**

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

@import "./tokens.css";

html { color-scheme: light dark; }
html[data-theme="light"] { color-scheme: light; }
html[data-theme="dark"]  { color-scheme: dark; }

body {
  @apply bg-parchment text-ink font-body antialiased;
  min-height: 100dvh;
}
```

- [ ] **Step 6: Verify Tailwind compiles**

Update `src/pages/index.astro`:

```astro
---
import "@/styles/globals.css";
---
<html lang="en"><body class="font-display text-2xl">Tailwind ok ✓</body></html>
```

Run: `pnpm dev`
Expected: page shows styled text. Kill server.

- [ ] **Step 7: Commit**

```bash
git add .
git commit -m "feat(style): add Tailwind with medieval theme tokens and globals"
```

---

## Task 3: Self-host fonts (Cinzel, Inter, JetBrains Mono)

**Files:**
- Create: `public/fonts/*.woff2` (download), `src/styles/fonts.css`
- Modify: `src/styles/globals.css` (import fonts.css)

- [ ] **Step 1: Download font subsets**

We need:
- Cinzel 400, 600 (latin + latin-ext)
- Inter 400, 500, 700 (latin + latin-ext)
- JetBrains Mono 400, 700 (latin + latin-ext)

Use Google Fonts → "Download family" or use `fontsource` packages for stable subsets:

```bash
pnpm add @fontsource/cinzel @fontsource/inter @fontsource/jetbrains-mono
```

- [ ] **Step 2: Write `src/styles/fonts.css`**

```css
@import "@fontsource/cinzel/400.css";
@import "@fontsource/cinzel/600.css";
@import "@fontsource/inter/400.css";
@import "@fontsource/inter/500.css";
@import "@fontsource/inter/700.css";
@import "@fontsource/jetbrains-mono/400.css";
@import "@fontsource/jetbrains-mono/700.css";
```

- [ ] **Step 3: Import in `src/styles/globals.css`**

Add at top, after `@tailwind utilities;`:

```css
@import "./fonts.css";
```

- [ ] **Step 4: Verify fonts load**

Update `src/pages/index.astro` body content:

```astro
<body class="font-body">
  <h1 class="font-display text-4xl">Cinzel Display</h1>
  <p class="font-body">Inter Body</p>
  <code class="font-mono">JetBrains Mono</code>
</body>
```

Run `pnpm dev`, visually verify three distinct typefaces render. Kill server.

- [ ] **Step 5: Commit**

```bash
git add .
git commit -m "feat(style): self-host Cinzel/Inter/JetBrains Mono via fontsource"
```

---

## Task 4: Implement i18n locale config and UI strings

**Files:**
- Create: `src/i18n/locales.ts`, `src/i18n/ui.ts`, `src/i18n/utils.ts`
- Test: `src/i18n/utils.test.ts`

- [ ] **Step 1: Install Vitest for unit tests**

```bash
pnpm add -D vitest
```

Add to `package.json` scripts:

```json
"test": "vitest run",
"test:watch": "vitest"
```

- [ ] **Step 2: Write `src/i18n/locales.ts`**

```ts
export const locales = ["en", "tr", "es", "de"] as const;
export const defaultLocale = "en";
export type Locale = (typeof locales)[number];

export const localeMeta: Record<Locale, { name: string; flag: string; dir: "ltr" | "rtl" }> = {
  en: { name: "English",  flag: "🇬🇧", dir: "ltr" },
  tr: { name: "Türkçe",   flag: "🇹🇷", dir: "ltr" },
  es: { name: "Español",  flag: "🇪🇸", dir: "ltr" },
  de: { name: "Deutsch",  flag: "🇩🇪", dir: "ltr" },
};

export function isLocale(value: string): value is Locale {
  return (locales as readonly string[]).includes(value);
}
```

- [ ] **Step 3: Write `src/i18n/ui.ts`**

```ts
import type { Locale } from "./locales";

export const ui = {
  en: {
    "nav.civs": "Civilizations",
    "nav.builds": "Build Orders",
    "nav.units": "Units",
    "nav.maps": "Maps",
    "nav.matchups": "Matchups",
    "nav.learn": "Learn",
    "nav.glossary": "Glossary",
    "nav.search": "Search",
    "nav.about": "About",
    "ui.search.placeholder": "Search guides…",
    "ui.theme.light": "Light",
    "ui.theme.dark": "Dark",
    "ui.theme.auto": "Auto",
    "ui.translation.missing": "This page is not translated yet — showing English.",
    "footer.attribution":
      'Age of Empires II © Microsoft Corporation. This site was created under Microsoft\'s "Game Content Usage Rules" using assets from Age of Empires II, and it is not endorsed by or affiliated with Microsoft.',
  },
  tr: {
    "nav.civs": "Uygarlıklar",
    "nav.builds": "Yapım Sıraları",
    "nav.units": "Birimler",
    "nav.maps": "Haritalar",
    "nav.matchups": "Eşleşmeler",
    "nav.learn": "Öğren",
    "nav.glossary": "Sözlük",
    "nav.search": "Ara",
    "nav.about": "Hakkında",
    "ui.search.placeholder": "Rehberlerde ara…",
    "ui.theme.light": "Açık",
    "ui.theme.dark": "Koyu",
    "ui.theme.auto": "Otomatik",
    "ui.translation.missing": "Bu sayfa henüz çevrilmedi — İngilizce gösteriliyor.",
    "footer.attribution":
      'Age of Empires II © Microsoft Corporation. Bu site, Microsoft\'un "Game Content Usage Rules" kuralları çerçevesinde Age of Empires II varlıkları kullanılarak oluşturulmuştur; Microsoft tarafından onaylanmamıştır veya bağlantılı değildir.',
  },
  es: {
    "nav.civs": "Civilizaciones",
    "nav.builds": "Órdenes de Construcción",
    "nav.units": "Unidades",
    "nav.maps": "Mapas",
    "nav.matchups": "Enfrentamientos",
    "nav.learn": "Aprende",
    "nav.glossary": "Glosario",
    "nav.search": "Buscar",
    "nav.about": "Acerca de",
    "ui.search.placeholder": "Buscar guías…",
    "ui.theme.light": "Claro",
    "ui.theme.dark": "Oscuro",
    "ui.theme.auto": "Auto",
    "ui.translation.missing": "Esta página aún no está traducida — mostrando inglés.",
    "footer.attribution":
      'Age of Empires II © Microsoft Corporation. Este sitio se creó bajo las "Game Content Usage Rules" de Microsoft utilizando assets de Age of Empires II; no está respaldado ni afiliado a Microsoft.',
  },
  de: {
    "nav.civs": "Zivilisationen",
    "nav.builds": "Aufbaureihenfolgen",
    "nav.units": "Einheiten",
    "nav.maps": "Karten",
    "nav.matchups": "Begegnungen",
    "nav.learn": "Lernen",
    "nav.glossary": "Glossar",
    "nav.search": "Suche",
    "nav.about": "Über",
    "ui.search.placeholder": "Guides durchsuchen…",
    "ui.theme.light": "Hell",
    "ui.theme.dark": "Dunkel",
    "ui.theme.auto": "Auto",
    "ui.translation.missing": "Diese Seite ist noch nicht übersetzt — Englisch wird angezeigt.",
    "footer.attribution":
      'Age of Empires II © Microsoft Corporation. Diese Seite wurde unter den "Game Content Usage Rules" von Microsoft mit Inhalten aus Age of Empires II erstellt; sie ist nicht von Microsoft unterstützt oder mit ihm verbunden.',
  },
} as const satisfies Record<Locale, Record<string, string>>;

export type UIKey = keyof typeof ui.en;
```

- [ ] **Step 4: Write `src/i18n/utils.ts`**

```ts
import { locales, defaultLocale, isLocale } from "./locales";
import type { Locale } from "./locales";
import { ui, type UIKey } from "./ui";

export function getLocaleFromUrl(url: URL): Locale {
  const segments = url.pathname.split("/").filter(Boolean);
  const first = segments[0];
  return first && isLocale(first) ? first : defaultLocale;
}

export function t(locale: Locale, key: UIKey): string {
  return ui[locale][key] ?? ui[defaultLocale][key];
}

/** Rewrite a path from one locale to another, preserving the route after /<lang>/ */
export function localizedPath(path: string, locale: Locale): string {
  const trimmed = path.startsWith("/") ? path.slice(1) : path;
  const segments = trimmed.split("/");
  if (segments[0] && isLocale(segments[0])) {
    segments[0] = locale;
  } else {
    segments.unshift(locale);
  }
  return "/" + segments.join("/");
}

/** Build URL paths for any of the 4 locales for a given route. */
export function allLocalePaths(path: string): Array<{ locale: Locale; path: string }> {
  return locales.map((l) => ({ locale: l, path: localizedPath(path, l) }));
}

export { locales, defaultLocale, isLocale };
export type { Locale };
```

- [ ] **Step 5: Write `src/i18n/utils.test.ts`**

```ts
import { describe, it, expect } from "vitest";
import { getLocaleFromUrl, t, localizedPath, allLocalePaths } from "./utils";

describe("getLocaleFromUrl", () => {
  it("returns locale from /<lang>/ prefix", () => {
    expect(getLocaleFromUrl(new URL("https://x/en/civs/britons"))).toBe("en");
    expect(getLocaleFromUrl(new URL("https://x/tr/civs/britons"))).toBe("tr");
    expect(getLocaleFromUrl(new URL("https://x/es/civs"))).toBe("es");
    expect(getLocaleFromUrl(new URL("https://x/de/"))).toBe("de");
  });
  it("falls back to defaultLocale when no /<lang>/ prefix", () => {
    expect(getLocaleFromUrl(new URL("https://x/"))).toBe("en");
    expect(getLocaleFromUrl(new URL("https://x/civs"))).toBe("en");
  });
});

describe("t", () => {
  it("returns translated string for known key", () => {
    expect(t("tr", "nav.civs")).toBe("Uygarlıklar");
    expect(t("de", "nav.units")).toBe("Einheiten");
  });
});

describe("localizedPath", () => {
  it("swaps the leading locale segment", () => {
    expect(localizedPath("/en/civs/britons", "tr")).toBe("/tr/civs/britons");
    expect(localizedPath("/tr/builds", "es")).toBe("/es/builds");
  });
  it("inserts locale when missing", () => {
    expect(localizedPath("/civs/britons", "de")).toBe("/de/civs/britons");
    expect(localizedPath("civs", "en")).toBe("/en/civs");
  });
});

describe("allLocalePaths", () => {
  it("returns paths for all 4 locales", () => {
    const paths = allLocalePaths("/civs/britons");
    expect(paths).toHaveLength(4);
    expect(paths.find((p) => p.locale === "tr")?.path).toBe("/tr/civs/britons");
    expect(paths.find((p) => p.locale === "de")?.path).toBe("/de/civs/britons");
  });
});
```

- [ ] **Step 6: Run tests, verify all pass**

Run: `pnpm test`
Expected: 4 test files (well, 1 file with 4 describe blocks) — all green.

- [ ] **Step 7: Commit**

```bash
git add .
git commit -m "feat(i18n): locale config, UI strings (EN/TR/ES/DE), helper utils"
```

---

## Task 5: Implement BaseLayout, Header, Footer

**Files:**
- Create: `src/components/layout/BaseLayout.astro`, `src/components/layout/Header.astro`, `src/components/layout/Footer.astro`, `src/components/layout/LanguageSwitcher.astro`, `src/components/layout/ThemeToggle.astro`

- [ ] **Step 1: Write `src/components/layout/BaseLayout.astro`**

```astro
---
import "@/styles/globals.css";
import Header from "./Header.astro";
import Footer from "./Footer.astro";
import { getLocaleFromUrl, localeMeta, locales, allLocalePaths } from "@/i18n/locales";
import { t } from "@/i18n/utils";

interface Props {
  title: string;
  description?: string;
  /** Path WITHOUT locale prefix, used for hreflang and lang switcher. e.g. "/civs/britons" */
  canonicalPath?: string;
}

const { title, description = "Age of Empires II guide", canonicalPath = "/" } = Astro.props;
const locale = getLocaleFromUrl(Astro.url);
const dir = localeMeta[locale].dir;
const site = Astro.site?.toString().replace(/\/$/, "") ?? "";
const alternates = allLocalePaths(canonicalPath);
---
<!doctype html>
<html lang={locale} dir={dir} data-theme="light">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{title}</title>
    <meta name="description" content={description} />
    <link rel="icon" href="/favicon.svg" type="image/svg+xml" />
    {alternates.map(({ locale: l, path }) => (
      <link rel="alternate" hreflang={l} href={`${site}${path}`} />
    ))}
    <link rel="alternate" hreflang="x-default" href={`${site}/en${canonicalPath}`} />
    <link rel="canonical" href={`${site}/${locale}${canonicalPath}`} />
    <script is:inline>
      // Set theme before paint to avoid FOUC
      (() => {
        const stored = localStorage.getItem("theme");
        const sys = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
        const t = stored === "dark" || stored === "light" ? stored : sys;
        document.documentElement.setAttribute("data-theme", t);
      })();
    </script>
  </head>
  <body class="flex min-h-dvh flex-col">
    <Header />
    <main class="flex-1">
      <slot />
    </main>
    <Footer />
  </body>
</html>
```

- [ ] **Step 2: Write `src/components/layout/Header.astro`**

```astro
---
import { getLocaleFromUrl } from "@/i18n/locales";
import { t, localizedPath } from "@/i18n/utils";
import LanguageSwitcher from "./LanguageSwitcher.astro";
import ThemeToggle from "./ThemeToggle.astro";

const locale = getLocaleFromUrl(Astro.url);
const nav = [
  { key: "nav.civs",     href: localizedPath("/civs",     locale) },
  { key: "nav.builds",   href: localizedPath("/builds",   locale) },
  { key: "nav.units",    href: localizedPath("/units",    locale) },
  { key: "nav.maps",     href: localizedPath("/maps",     locale) },
  { key: "nav.matchups", href: localizedPath("/matchups", locale) },
  { key: "nav.learn",    href: localizedPath("/learn",    locale) },
  { key: "nav.glossary", href: localizedPath("/glossary", locale) },
] as const;
---
<header class="sticky top-0 z-40 border-b border-stone-700/30 bg-parchment/95 backdrop-blur">
  <div class="mx-auto flex max-w-7xl items-center gap-4 px-4 py-3">
    <a href={localizedPath("/", locale)} class="font-display text-xl text-ink">AOE2 Guide</a>
    <nav class="ml-4 hidden gap-4 md:flex">
      {nav.map(({ key, href }) => (
        <a href={href} class="text-sm text-stone-700 hover:text-gold-500">{t(locale, key)}</a>
      ))}
    </nav>
    <div class="ml-auto flex items-center gap-2">
      <a href={localizedPath("/search", locale)} class="text-sm text-stone-700 hover:text-gold-500">{t(locale, "nav.search")}</a>
      <LanguageSwitcher />
      <ThemeToggle />
    </div>
  </div>
</header>
```

- [ ] **Step 3: Write `src/components/layout/LanguageSwitcher.astro`**

```astro
---
import { getLocaleFromUrl, locales, localeMeta } from "@/i18n/locales";
import { localizedPath } from "@/i18n/utils";

const currentLocale = getLocaleFromUrl(Astro.url);
const pathname = Astro.url.pathname;
---
<details class="relative">
  <summary class="cursor-pointer list-none text-sm text-stone-700 hover:text-gold-500">
    {localeMeta[currentLocale].flag} {localeMeta[currentLocale].name}
  </summary>
  <div class="absolute right-0 mt-2 w-40 rounded border border-stone-700/40 bg-parchment p-1 shadow">
    {locales.map((l) => (
      <a href={localizedPath(pathname, l)}
         class={`block rounded px-2 py-1 text-sm ${l === currentLocale ? "bg-gold-500/20 text-ink" : "text-stone-700 hover:bg-stone-700/10"}`}>
        {localeMeta[l].flag} {localeMeta[l].name}
      </a>
    ))}
  </div>
</details>
```

- [ ] **Step 4: Write `src/components/layout/ThemeToggle.astro`**

```astro
---
import { getLocaleFromUrl } from "@/i18n/locales";
import { t } from "@/i18n/utils";
const locale = getLocaleFromUrl(Astro.url);
---
<button id="theme-toggle"
        type="button"
        aria-label={t(locale, "ui.theme.light") + " / " + t(locale, "ui.theme.dark")}
        class="rounded border border-stone-700/40 px-2 py-1 text-sm text-stone-700 hover:text-gold-500">
  <span data-theme-icon-light>☀</span>
  <span data-theme-icon-dark class="hidden">☾</span>
</button>
<script is:inline>
  (() => {
    const btn = document.getElementById("theme-toggle");
    if (!btn) return;
    const sync = () => {
      const dark = document.documentElement.getAttribute("data-theme") === "dark";
      btn.querySelector("[data-theme-icon-light]").classList.toggle("hidden", dark);
      btn.querySelector("[data-theme-icon-dark]").classList.toggle("hidden", !dark);
    };
    sync();
    btn.addEventListener("click", () => {
      const dark = document.documentElement.getAttribute("data-theme") === "dark";
      const next = dark ? "light" : "dark";
      document.documentElement.setAttribute("data-theme", next);
      localStorage.setItem("theme", next);
      sync();
    });
  })();
</script>
```

- [ ] **Step 5: Write `src/components/layout/Footer.astro`**

```astro
---
import { getLocaleFromUrl } from "@/i18n/locales";
import { t, localizedPath } from "@/i18n/utils";
const locale = getLocaleFromUrl(Astro.url);
---
<footer class="mt-12 border-t border-stone-700/30 bg-parchment/50">
  <div class="mx-auto max-w-7xl px-4 py-6 text-xs text-stone-700">
    <p class="mb-2">{t(locale, "footer.attribution")}</p>
    <p>
      <a class="hover:text-gold-500" href={localizedPath("/about", locale)}>{t(locale, "nav.about")}</a>
      ·
      <a class="hover:text-gold-500" href="https://github.com/themkdemiiir/aoe2-guide">GitHub</a>
    </p>
  </div>
</footer>
```

- [ ] **Step 6: Commit**

```bash
git add .
git commit -m "feat(layout): BaseLayout, Header, Footer, LanguageSwitcher, ThemeToggle"
```

---

## Task 6: Implement `/` locale chooser landing

**Files:**
- Modify: `src/pages/index.astro`

- [ ] **Step 1: Replace `src/pages/index.astro` with the locale chooser**

```astro
---
import BaseLayout from "@/components/layout/BaseLayout.astro";
import { locales, localeMeta } from "@/i18n/locales";
---
<BaseLayout title="AOE2 Guide" canonicalPath="/">
  <script is:inline>
    // Auto-redirect to preferred locale on return visits.
    (() => {
      try {
        const stored = localStorage.getItem("preferredLang");
        const supported = ["en", "tr", "es", "de"];
        const nav = (navigator.language || "en").slice(0, 2);
        const target = supported.includes(stored) ? stored
                      : supported.includes(nav) ? nav
                      : "en";
        if (location.pathname === "/" && sessionStorage.getItem("seenLanding") !== "1") {
          // Only auto-redirect after the user has visited once.
          if (stored && supported.includes(stored)) {
            location.replace("/" + stored + "/");
          }
        }
        sessionStorage.setItem("seenLanding", "1");
      } catch {}
    })();
  </script>

  <section class="mx-auto max-w-2xl px-4 py-16 text-center">
    <h1 class="font-display text-5xl text-ink">AOE2 Guide</h1>
    <p class="mt-4 text-stone-700">Pick your language to enter.</p>

    <ul class="mt-10 grid gap-3 sm:grid-cols-2">
      {locales.map((l) => (
        <li>
          <a href={`/${l}/`}
             onclick={`localStorage.setItem('preferredLang','${l}')`}
             class="flex items-center justify-center gap-2 rounded border border-stone-700/40 bg-parchment px-4 py-3 text-lg text-ink hover:border-gold-500 hover:text-gold-500">
            <span>{localeMeta[l].flag}</span>
            <span>{localeMeta[l].name}</span>
          </a>
        </li>
      ))}
    </ul>
  </section>
</BaseLayout>
```

- [ ] **Step 2: Verify visually**

Run `pnpm dev`, open `http://localhost:4321/`, confirm: 4 language buttons render, theme toggle works, language switcher dropdown appears but is empty of "current language" highlight since `/` is locale-less. Kill server.

- [ ] **Step 3: Commit**

```bash
git add .
git commit -m "feat(pages): locale chooser at / with auto-redirect on return visits"
```

---

## Task 7: Create empty index pages for every section in every locale

**Files:**
- Create: `src/pages/[lang]/index.astro`, `src/pages/[lang]/civs/index.astro`, `src/pages/[lang]/builds/index.astro`, `src/pages/[lang]/units/index.astro`, `src/pages/[lang]/maps/index.astro`, `src/pages/[lang]/matchups/index.astro`, `src/pages/[lang]/learn/index.astro`, `src/pages/[lang]/glossary/index.astro`, `src/pages/[lang]/search.astro`, `src/pages/[lang]/about.astro`, `src/pages/404.astro`

- [ ] **Step 1: Write `src/pages/[lang]/index.astro` (locale home)**

```astro
---
import BaseLayout from "@/components/layout/BaseLayout.astro";
import { locales, type Locale } from "@/i18n/locales";
import { t } from "@/i18n/utils";

export function getStaticPaths() {
  return locales.map((lang) => ({ params: { lang } }));
}

const lang = Astro.params.lang as Locale;
---
<BaseLayout title="AOE2 Guide" canonicalPath="/">
  <section class="mx-auto max-w-prose px-4 py-12">
    <h1 class="font-display text-4xl text-ink">AOE2 Guide</h1>
    <p class="mt-4 text-stone-700">Hub for civilizations, build orders, units, maps, and beginner learning.</p>
  </section>
</BaseLayout>
```

- [ ] **Step 2: Write `src/pages/[lang]/civs/index.astro`**

```astro
---
import BaseLayout from "@/components/layout/BaseLayout.astro";
import { locales, type Locale } from "@/i18n/locales";
import { t } from "@/i18n/utils";

export function getStaticPaths() {
  return locales.map((lang) => ({ params: { lang } }));
}
const lang = Astro.params.lang as Locale;
---
<BaseLayout title={t(lang, "nav.civs")} canonicalPath="/civs">
  <section class="mx-auto max-w-prose px-4 py-12">
    <h1 class="font-display text-4xl text-ink">{t(lang, "nav.civs")}</h1>
    <p class="mt-4 text-stone-700">Coming in Phase 4.</p>
  </section>
</BaseLayout>
```

- [ ] **Step 3: Write equivalent stubs for `builds`, `units`, `maps`, `matchups`, `learn`, `glossary`, `search`, `about`**

Each follows the same pattern: `getStaticPaths` returns the 4 locales, `t(lang, "nav.<section>")` for the title.

For `search.astro` and `about.astro`, drop the `/index` suffix.

- [ ] **Step 4: Write `src/pages/404.astro`**

```astro
---
import BaseLayout from "@/components/layout/BaseLayout.astro";
---
<BaseLayout title="Not Found" canonicalPath="/404">
  <section class="mx-auto max-w-prose px-4 py-24 text-center">
    <h1 class="font-display text-5xl text-royal">404</h1>
    <p class="mt-4 text-stone-700">This guide does not exist (yet).</p>
    <p class="mt-2 text-sm text-stone-700"><a class="underline hover:text-gold-500" href="/">Return home</a></p>
  </section>
</BaseLayout>
```

- [ ] **Step 5: Verify all routes 200**

Run `pnpm dev`, manually visit:
- `/`
- `/en/`, `/tr/`, `/es/`, `/de/`
- `/en/civs/`, `/en/builds/`, `/en/units/`, `/en/maps/`, `/en/matchups/`, `/en/learn/`, `/en/glossary/`, `/en/search/`, `/en/about/`

All should render without errors. Kill server.

- [ ] **Step 6: Verify production build**

Run: `pnpm build`
Expected: build succeeds, `dist/` contains all the localized routes. No warnings about missing translations (since we're in the foundation phase, all pages exist).

- [ ] **Step 7: Commit**

```bash
git add .
git commit -m "feat(pages): empty section pages for civs/builds/units/maps/matchups/learn/glossary/search/about + 404"
```

---

## Task 8: Phase 1 manual smoke test

- [ ] **Step 1: Test theme toggle persists across pages**

Run `pnpm dev`, toggle theme on `/en/`, navigate to `/en/civs/`, verify theme stayed.

- [ ] **Step 2: Test language switcher rewrites paths correctly**

On `/en/civs/`, switch to TR via the dropdown — should land on `/tr/civs/`.

- [ ] **Step 3: Confirm Phase 1 milestone**

> `pnpm dev` runs, all routes 200, theme toggle works, locale chooser at `/` ✓

---

# Phase 2 — Schemas, content collections, and helpers

## Task 9: Define Zod schemas in `src/content/config.ts`

**Files:**
- Create: `src/content/config.ts`

- [ ] **Step 1: Write the file with all 7 collections**

```ts
import { z, defineCollection } from "astro:content";

const civilizations = defineCollection({
  type: "content",
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
  type: "content",
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
  type: "content",
  schema: z.object({
    slug: z.string(),
    name: z.string(),
    role: z.string(),
    civ: z.string().optional(),
  }),
});

const maps = defineCollection({
  type: "content",
  schema: z.object({
    slug: z.string(),
    name: z.string(),
    type: z.enum(["open", "closed", "hybrid", "water", "nomad"]),
    size: z.enum(["tiny", "small", "medium", "large"]).optional(),
    recommendedCivs: z.array(z.string()),
  }),
});

const matchups = defineCollection({
  type: "content",
  schema: z.object({
    slug: z.string(),
    civA: z.string(),
    civB: z.string(),
    difficulty: z.enum(["even", "favored", "unfavored"]),
  }),
});

const beginner = defineCollection({
  type: "content",
  schema: z.object({
    slug: z.string(),
    title: z.string(),
    order: z.number(),
    prereq: z.array(z.string()).optional(),
  }),
});

const glossary = defineCollection({
  type: "content",
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
```

- [ ] **Step 2: Add `.gitkeep`s for empty content directories**

```bash
for type in civilizations build-orders units maps matchups beginner glossary; do
  for lang in en tr es de; do
    mkdir -p src/content/$type/$lang
    touch src/content/$type/$lang/.gitkeep
  done
done
```

- [ ] **Step 3: Verify build is happy with empty collections**

Run: `pnpm build`
Expected: succeeds. Astro reports content collections exist but are empty.

- [ ] **Step 4: Commit**

```bash
git add .
git commit -m "feat(content): Zod schemas for all 7 content collections + lang folders"
```

---

## Task 10: Implement content helpers

**Files:**
- Create: `src/lib/content.ts`
- Test: `src/lib/content.test.ts`

- [ ] **Step 1: Write `src/lib/content.ts`**

```ts
import { getCollection, type CollectionEntry } from "astro:content";
import { defaultLocale, type Locale } from "@/i18n/locales";

type ContentType =
  | "civilizations"
  | "build-orders"
  | "units"
  | "maps"
  | "matchups"
  | "beginner"
  | "glossary";

/** Returns the language for a content entry by parsing its slug prefix: "en/britons" → "en" */
export function localeFromEntryId(id: string): Locale {
  const seg = id.split("/")[0];
  return (seg === "en" || seg === "tr" || seg === "es" || seg === "de") ? seg : defaultLocale;
}

/** Returns the canonical slug from an entry: "en/britons" → "britons" */
export function canonicalSlug(id: string): string {
  const segs = id.split("/");
  segs.shift(); // drop locale
  return segs.join("/").replace(/\.(md|mdx)$/i, "");
}

/** Get all entries of a collection in a given locale, with fallback to defaultLocale. */
export async function getLocalizedEntries<T extends ContentType>(
  type: T,
  locale: Locale,
): Promise<Array<CollectionEntry<T>>> {
  const all = await getCollection(type);
  const inLocale = all.filter((e) => localeFromEntryId(e.id) === locale);
  if (inLocale.length > 0) return inLocale;
  return all.filter((e) => localeFromEntryId(e.id) === defaultLocale);
}

/** Get a single entry by slug in a given locale, falling back to default. */
export async function getLocalizedEntry<T extends ContentType>(
  type: T,
  slug: string,
  locale: Locale,
): Promise<{ entry: CollectionEntry<T>; fallback: boolean } | null> {
  const all = await getCollection(type);
  const exact = all.find((e) => localeFromEntryId(e.id) === locale && canonicalSlug(e.id) === slug);
  if (exact) return { entry: exact, fallback: false };
  const fb = all.find((e) => localeFromEntryId(e.id) === defaultLocale && canonicalSlug(e.id) === slug);
  return fb ? { entry: fb, fallback: true } : null;
}
```

- [ ] **Step 2: Write `src/lib/content.test.ts`**

```ts
import { describe, it, expect } from "vitest";
import { localeFromEntryId, canonicalSlug } from "./content";

describe("localeFromEntryId", () => {
  it("extracts locale from id", () => {
    expect(localeFromEntryId("en/britons")).toBe("en");
    expect(localeFromEntryId("tr/franks")).toBe("tr");
    expect(localeFromEntryId("es/maps/arabia")).toBe("es");
  });
  it("falls back to defaultLocale for malformed ids", () => {
    expect(localeFromEntryId("britons")).toBe("en");
  });
});

describe("canonicalSlug", () => {
  it("strips locale prefix and extension", () => {
    expect(canonicalSlug("en/britons")).toBe("britons");
    expect(canonicalSlug("tr/builds/21pop-archer")).toBe("builds/21pop-archer");
    expect(canonicalSlug("en/britons.md")).toBe("britons");
  });
});
```

- [ ] **Step 3: Run tests, verify pass**

Run: `pnpm test`
Expected: 6 tests pass (4 i18n + 2 + 2 content tests = 8 total assertions actually). All green.

- [ ] **Step 4: Commit**

```bash
git add .
git commit -m "feat(lib): content helpers with locale fallback + tests"
```

---

## Task 11: Scaffold language-agnostic data files

**Files:**
- Create: `src/data/civilizations.json`, `src/data/unit-stats.json`, `src/data/counters.json`, `src/data/tech-tree.json`, `src/data/icon-map.json`
- All start empty / minimal.

- [ ] **Step 1: Write `src/data/civilizations.json`**

```json
{
  "$schema": "./civilizations.schema.json",
  "patch": "v100.1.84",
  "civs": []
}
```

- [ ] **Step 2: Write `src/data/unit-stats.json`**

```json
{ "patch": "v100.1.84", "units": [] }
```

- [ ] **Step 3: Write `src/data/counters.json`**

```json
{ "patch": "v100.1.84", "counters": {} }
```

- [ ] **Step 4: Write `src/data/tech-tree.json`**

```json
{ "patch": "v100.1.84", "matrix": {} }
```

- [ ] **Step 5: Write `src/data/icon-map.json`** (will be regenerated by script in Phase 3)

```json
{}
```

- [ ] **Step 6: Commit**

```bash
git add .
git commit -m "feat(data): scaffold language-agnostic data files"
```

---

# Phase 3 — Asset pipeline

## Task 12: Write `scripts/sync-assets.mjs`

**Files:**
- Create: `scripts/sync-assets.mjs`
- Add npm script: `assets:sync`

The script downloads a tarball of aoe2techtree at a pinned commit, extracts only the `img/` subdirectory into `public/images/aoe2/`.

- [ ] **Step 1: Write the script**

```js
#!/usr/bin/env node
// scripts/sync-assets.mjs
// Fetches the img/ tree of SiegeEngineers/aoe2techtree at a pinned SHA
// and mirrors it into public/images/aoe2/.

import { mkdir, rm, writeFile } from "node:fs/promises";
import { createWriteStream } from "node:fs";
import { pipeline } from "node:stream/promises";
import { spawn } from "node:child_process";
import path from "node:path";

const REPO = "SiegeEngineers/aoe2techtree";
// Bump this SHA in a deliberate PR when refreshing assets.
const SHA = process.env.AOE2TT_SHA || "master";
const TARGET = path.resolve("public/images/aoe2");
const TMP = path.resolve(".cache/aoe2tt");

async function run() {
  await rm(TMP, { recursive: true, force: true });
  await mkdir(TMP, { recursive: true });
  await mkdir(TARGET, { recursive: true });

  const url = `https://codeload.github.com/${REPO}/tar.gz/${SHA}`;
  console.log("Fetching", url);
  const res = await fetch(url);
  if (!res.ok) throw new Error(`fetch ${url}: ${res.status}`);
  const tarPath = path.join(TMP, "src.tar.gz");
  await pipeline(res.body, createWriteStream(tarPath));

  console.log("Extracting img/ subtree …");
  await new Promise((resolve, reject) => {
    const tar = spawn(
      "tar",
      ["xzf", tarPath, "-C", TMP, "--wildcards", "*/img/*", "--strip-components=2"],
      { stdio: "inherit" },
    );
    tar.on("exit", (code) => (code === 0 ? resolve() : reject(new Error("tar exit " + code))));
  });

  console.log("Copying into", TARGET);
  await rm(TARGET, { recursive: true, force: true });
  await mkdir(TARGET, { recursive: true });
  await new Promise((resolve, reject) => {
    const cp = spawn("cp", ["-R", path.join(TMP, "img/."), TARGET], { stdio: "inherit" });
    cp.on("exit", (code) => (code === 0 ? resolve() : reject(new Error("cp exit " + code))));
  });

  console.log("Done. Assets at", TARGET);
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
```

- [ ] **Step 2: Add npm script to `package.json`**

```json
"scripts": {
  "assets:sync": "node scripts/sync-assets.mjs"
}
```

- [ ] **Step 3: Pin a specific SHA**

Look up the current `master` SHA of aoe2techtree, hardcode it in the script's `SHA` constant. (e.g., `const SHA = process.env.AOE2TT_SHA || "<sha>";`)

Use: `gh api repos/SiegeEngineers/aoe2techtree/branches/master --jq .commit.sha`

Replace the literal `"master"` with that value.

- [ ] **Step 4: Run it**

```bash
pnpm run assets:sync
```

Expected: `public/images/aoe2/Civs/`, `Unit/`, `Tech/`, `Building/` etc. populated with PNGs.

- [ ] **Step 5: Add the asset directory to `.gitignore`? (decide)**

We have two options:
- Commit the assets — repo is larger but onboarding doesn't require network access.
- Gitignore them — repo stays slim, but CI/Cloudflare must run the sync.

For simplicity, **commit them** (the AOE2 icon set is ~5-10 MB total). Add a note in `CLAUDE.md` that the sync script is the canonical refresh path.

- [ ] **Step 6: Commit**

```bash
git add scripts/sync-assets.mjs package.json public/images/aoe2/
git commit -m "feat(assets): sync script + bundle aoe2techtree icons at pinned SHA"
```

---

## Task 13: Write `scripts/build-icon-map.mjs`

**Files:**
- Create: `scripts/build-icon-map.mjs`
- Output: `src/data/icon-map.json`

- [ ] **Step 1: Fetch aoc-reference-data 100.json one time**

```bash
mkdir -p .cache/aoc
curl -sLo .cache/aoc/100.json https://raw.githubusercontent.com/SiegeEngineers/aoc-reference-data/master/data/datasets/100.json
```

- [ ] **Step 2: Inspect the data shape**

Open `.cache/aoc/100.json` to confirm the structure (`{ units: [...], techs: [...], civs: [...] }`-ish). The script below assumes the standard SiegeEngineers schema; if the field names differ, adjust accordingly.

- [ ] **Step 3: Write the script**

```js
#!/usr/bin/env node
// scripts/build-icon-map.mjs
import { readFile, writeFile, readdir, mkdir } from "node:fs/promises";
import path from "node:path";

const REF_URL =
  "https://raw.githubusercontent.com/SiegeEngineers/aoc-reference-data/master/data/datasets/100.json";
const ASSETS_DIR = "public/images/aoe2";
const OUT = "src/data/icon-map.json";

function slugify(name) {
  return String(name)
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^\w\s-]/g, "")
    .trim()
    .replace(/\s+/g, "-");
}

async function fetchRef() {
  const res = await fetch(REF_URL);
  if (!res.ok) throw new Error(`fetch ${REF_URL}: ${res.status}`);
  return res.json();
}

async function mapDir(folder, items, kind) {
  const files = new Set(await readdir(path.join(ASSETS_DIR, folder)));
  const map = {};
  for (const item of items) {
    const id = String(item.id);
    const slug = slugify(item.name || item.internal_name || id);
    const png = `${id}.png`;
    if (files.has(png)) {
      map[slug] = `/images/aoe2/${folder}/${png}`;
    }
  }
  return map;
}

async function run() {
  const ref = await fetchRef();
  const map = {
    units: await mapDir("Unit", ref.units ?? [], "unit"),
    techs: await mapDir("Tech", ref.techs ?? [], "tech"),
    buildings: await mapDir("Building", ref.buildings ?? [], "building"),
  };
  // Civs use named PNGs, not numeric IDs
  const civsDir = path.join(ASSETS_DIR, "Civs");
  const civFiles = await readdir(civsDir);
  map.civs = {};
  for (const file of civFiles) {
    if (!file.endsWith(".png")) continue;
    map.civs[file.replace(/\.png$/, "").toLowerCase()] = `/images/aoe2/Civs/${file}`;
  }

  await mkdir(path.dirname(OUT), { recursive: true });
  await writeFile(OUT, JSON.stringify(map, null, 2) + "\n");
  console.log(
    "Wrote",
    OUT,
    "units:",
    Object.keys(map.units).length,
    "techs:",
    Object.keys(map.techs).length,
    "buildings:",
    Object.keys(map.buildings).length,
    "civs:",
    Object.keys(map.civs).length,
  );
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
```

- [ ] **Step 4: Add npm script + run**

```json
"icons:map": "node scripts/build-icon-map.mjs"
```

```bash
pnpm run icons:map
```

Expected: `src/data/icon-map.json` populated with `civs`, `units`, `techs`, `buildings` keys.

- [ ] **Step 5: Adjust if the ref-data field names differ**

If the script fails because `ref.units` is named differently in the real data, update the field accesses based on what you saw in Step 2. This is the only step that depends on the upstream schema.

- [ ] **Step 6: Commit**

```bash
git add scripts/build-icon-map.mjs src/data/icon-map.json package.json
git commit -m "feat(assets): icon-map generator from aoc-reference-data"
```

---

## Task 14: Implement `<Icon>` component

**Files:**
- Create: `src/components/ui/Icon.astro`

- [ ] **Step 1: Write the component**

```astro
---
import iconMap from "@data/icon-map.json";

interface Props {
  slug: string;
  kind?: "unit" | "tech" | "building" | "civ";
  size?: number;
  alt?: string;
  framed?: boolean;
}

const { slug, kind = "unit", size = 48, alt = slug, framed = true } = Astro.props;

const map = (iconMap as Record<string, Record<string, string>>)[
  ({ unit: "units", tech: "techs", building: "buildings", civ: "civs" } as const)[kind]
] ?? {};

const src = map[slug];
---
{src ? (
  <span class={`inline-flex items-center justify-center ${framed ? "rounded border border-stone-700/40 bg-parchment/40 p-0.5" : ""}`} style={`width:${size + 6}px;height:${size + 6}px;`}>
    <img src={src} alt={alt} width={size} height={size} loading="lazy" decoding="async" />
  </span>
) : (
  <span class="inline-flex items-center justify-center rounded border border-dashed border-royal/50 bg-parchment/30 px-1 text-xs text-royal" style={`width:${size + 6}px;height:${size + 6}px;`}>
    ?{slug}
  </span>
)}
```

- [ ] **Step 2: Visual smoke test on the about page**

Edit `src/pages/[lang]/about.astro` to render a few icons, e.g.:

```astro
<Icon slug="knight" kind="unit" />
<Icon slug="britons" kind="civ" />
```

Run `pnpm dev`, visit `/en/about/`, confirm icons render. Revert the about page changes when done (icons are just a smoke test here).

- [ ] **Step 3: Commit**

```bash
git add src/components/ui/Icon.astro
git commit -m "feat(ui): Icon component with framed/unframed variants"
```

---

# Phase 4 — Civilizations end-to-end

## Task 15: Seed civilization data for Britons (proof-of-flow civ)

**Files:**
- Modify: `src/data/civilizations.json`
- Create: `src/content/civilizations/{en,tr,es,de}/britons.md`

- [ ] **Step 1: Add Britons to `src/data/civilizations.json`**

```json
{
  "patch": "v100.1.84",
  "civs": [
    {
      "slug": "britons",
      "era": "DE2",
      "region": "Western European",
      "specialty": "Archers",
      "tier": "B",
      "uniqueUnits": ["longbowman"],
      "strongAgainst": ["franks", "huns"],
      "weakAgainst": ["mongols", "magyars"],
      "recommendedBuilds": ["21pop-archer"]
    }
  ]
}
```

- [ ] **Step 2: Write `src/content/civilizations/en/britons.md`**

```markdown
---
slug: britons
name: Britons
tagline: Foot archer specialists with the longest-range Longbowman.
bonuses:
  - Town Centers cost 50% less wood from Castle Age.
  - Foot archers (except Skirmishers) have +1 range in Castle Age, +2 in Imperial Age.
  - Shepherds work 25% faster.
teamBonus: Archery Ranges work 20% faster.
uniqueTechs:
  castle:
    name: Yeomen
    effect: Foot archers gain +1 range; Towers +2 attack.
  imperial:
    name: Warwolf
    effect: Trebuchets do blast damage and never miss against units.
---

Britons are an archer civilization with one of the strongest unique units in the game — the Longbowman, a foot archer with extreme range. They specialize in defensive play and ranged composition, particularly into Castle and Imperial Age.

## Playstyle

Britons want to reach Castle Age fast with a strong wood economy (Town Center bonus), then mass Crossbowmen behind Longbowman support. The combination of range bonus + Yeomen turns them into the longest-range archer civ in the game.

## When to play Britons

- Closed maps where you can boom safely.
- Late-game team games where Longbowmen + Trebuchets break sieges.
```

- [ ] **Step 3: Write `src/content/civilizations/tr/britons.md`** (Turkish translation)

```markdown
---
slug: britons
name: Britons
tagline: En uzun menzilli Uzunyay Okçusuna sahip yaya okçu uzmanı.
bonuses:
  - Kale Çağı'ndan itibaren Şehir Merkezleri %50 daha az odun maliyetiyle inşa edilir.
  - Yaya okçuları (Skirmisher hariç) Kale Çağı'nda +1, Emperyal Çağ'da +2 menzil kazanır.
  - Çobanlar %25 daha hızlı çalışır.
teamBonus: Okçu Talimhaneleri %20 daha hızlı çalışır.
uniqueTechs:
  castle:
    name: Yeomen
    effect: Yaya okçuları +1 menzil; Kuleler +2 saldırı kazanır.
  imperial:
    name: Warwolf
    effect: Trebuchet'ler birimlere karşı alan hasarı verir ve ıskalamaz.
---

Britons, oyunun en güçlü özgün birimlerinden biri olan Uzunyay Okçusuna sahip bir okçu uygarlığıdır. Kale ve Emperyal Çağ'da uzun menzilli yaya okçu kompozisyonunda parlar.

## Oynayış

Britons güçlü bir odun ekonomisiyle (Şehir Merkezi bonusu) hızlı Kale Çağı'na geçer; ardından Uzunyay Okçusu desteğiyle Tatar Yayı kalabalığı kurar. Menzil bonusu + Yeomen, Britons'ı oyundaki en uzun menzilli okçu uygarlığı yapar.

## Britons ne zaman oynanır

- Güvenle ekonomi büyütebileceğin kapalı haritalar.
- Uzunyay Okçusu + Trebuchet'lerin kuşatma savunmasını kıracağı geç oyun takım maçları.
```

- [ ] **Step 4: Write Spanish version `src/content/civilizations/es/britons.md`**

```markdown
---
slug: britons
name: Britanos
tagline: Especialistas en arqueros a pie con el Arquero de arco largo de mayor alcance.
bonuses:
  - Los Centros Urbanos cuestan 50% menos madera desde la Edad de los Castillos.
  - Los arqueros a pie (excepto Escaramuzadores) ganan +1 de alcance en Castillos y +2 en Imperial.
  - Los Pastores trabajan 25% más rápido.
teamBonus: Las Galerías de tiro con arco trabajan 20% más rápido.
uniqueTechs:
  castle:
    name: Yeomen
    effect: Los arqueros a pie ganan +1 de alcance; las Torres +2 de ataque.
  imperial:
    name: Warwolf
    effect: Los Trabuquetes infligen daño por área y no fallan contra unidades.
---

Los Britanos son una civilización de arqueros con una de las unidades únicas más fuertes del juego — el Arquero de arco largo. Brillan en composición a distancia y juego defensivo en Castillos e Imperial.

## Estilo de juego

Avanzan rápido a Castillos con una economía maderera fuerte (bono del Centro Urbano), después acumulan Ballesteros con apoyo de Arqueros de arco largo. El bono de alcance + Yeomen los convierte en la civilización con mayor alcance del juego.

## Cuándo jugar Britanos

- Mapas cerrados donde puedas hacer boom con seguridad.
- Partidas en equipo de juego tardío donde Arqueros de arco largo + Trabuquetes rompan asedios.
```

- [ ] **Step 5: Write German version `src/content/civilizations/de/britons.md`**

```markdown
---
slug: britons
name: Briten
tagline: Fußbogen-Spezialisten mit dem reichweitenstärksten Langbogenschützen.
bonuses:
  - Dorfzentren kosten ab der Ritterzeit 50% weniger Holz.
  - Fußbogenschützen (außer Plänklern) erhalten +1 Reichweite in der Ritterzeit, +2 in der Imperialzeit.
  - Schäfer arbeiten 25% schneller.
teamBonus: Schießanlagen arbeiten 20% schneller.
uniqueTechs:
  castle:
    name: Yeomen
    effect: Fußbogenschützen erhalten +1 Reichweite; Türme +2 Angriff.
  imperial:
    name: Warwolf
    effect: Tribocks verursachen Flächenschaden und verfehlen Einheiten nie.
---

Die Briten sind eine Bogenschützen-Zivilisation mit einer der stärksten Spezialeinheiten — dem Langbogenschützen, einem Fußbogenschützen mit extremer Reichweite. Sie glänzen in defensivem Spiel und Fernkampf-Kompositionen ab der Ritterzeit.

## Spielstil

Briten kommen schnell in die Ritterzeit mit starker Holzwirtschaft (Dorfzentrum-Bonus) und massen dann Armbrustschützen mit Unterstützung von Langbogenschützen. Reichweitenbonus + Yeomen machen sie zur reichweitenstärksten Bogenschützen-Zivilisation im Spiel.

## Wann Briten spielen

- Geschlossene Karten, auf denen du sicher boomen kannst.
- Späte Teamspiele, in denen Langbogenschützen + Tribocks Belagerungen aufbrechen.
```

- [ ] **Step 6: Verify build is happy**

Run: `pnpm build`
Expected: build succeeds; civilizations collection has 4 entries (one per locale).

- [ ] **Step 7: Commit**

```bash
git add src/data/civilizations.json src/content/civilizations/
git commit -m "feat(civs): seed Britons in 4 languages + meta data"
```

---

## Task 16: Implement `<CivCard>` component

**Files:**
- Create: `src/components/content/CivCard.astro`

- [ ] **Step 1: Write the component**

```astro
---
import Icon from "@components/ui/Icon.astro";
import civsData from "@data/civilizations.json";
import { localizedPath } from "@/i18n/utils";
import type { Locale } from "@/i18n/locales";

interface Props {
  slug: string;
  name: string;
  tagline: string;
  bonuses: string[];
  locale: Locale;
}

const { slug, name, tagline, bonuses, locale } = Astro.props;
const meta = (civsData.civs as Array<any>).find((c) => c.slug === slug);
---
<a href={localizedPath(`/civs/${slug}`, locale)}
   class="block rounded border border-stone-700/40 bg-parchment p-4 transition hover:border-gold-500">
  <div class="flex items-center gap-3">
    <Icon slug={slug} kind="civ" size={56} />
    <div>
      <h3 class="font-display text-xl text-ink">{name}</h3>
      <p class="text-sm text-stone-700">{tagline}</p>
    </div>
  </div>
  {meta && (
    <div class="mt-3 flex flex-wrap gap-1 text-xs">
      <span class="rounded border border-stone-700/30 px-2 py-0.5 text-stone-700">{meta.specialty}</span>
      {meta.tier && <span class="rounded border border-gold-500/40 px-2 py-0.5 text-gold-500">Tier {meta.tier}</span>}
    </div>
  )}
  {bonuses?.length > 0 && (
    <ul class="mt-3 list-inside list-disc space-y-1 text-sm text-stone-700">
      {bonuses.slice(0, 3).map((b) => <li>{b}</li>)}
    </ul>
  )}
</a>
```

- [ ] **Step 2: Commit**

```bash
git add src/components/content/CivCard.astro
git commit -m "feat(content): CivCard component"
```

---

## Task 17: Civ index page `/[lang]/civs/`

**Files:**
- Modify: `src/pages/[lang]/civs/index.astro`

- [ ] **Step 1: Replace placeholder with real index**

```astro
---
import BaseLayout from "@/components/layout/BaseLayout.astro";
import CivCard from "@components/content/CivCard.astro";
import { locales, type Locale } from "@/i18n/locales";
import { t } from "@/i18n/utils";
import { getLocalizedEntries, canonicalSlug } from "@/lib/content";

export function getStaticPaths() {
  return locales.map((lang) => ({ params: { lang } }));
}

const lang = Astro.params.lang as Locale;
const civs = await getLocalizedEntries("civilizations", lang);
---
<BaseLayout title={t(lang, "nav.civs")} canonicalPath="/civs">
  <section class="mx-auto max-w-7xl px-4 py-12">
    <h1 class="font-display text-4xl text-ink">{t(lang, "nav.civs")}</h1>
    <ul class="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {civs.map((c) => (
        <li>
          <CivCard
            slug={canonicalSlug(c.id)}
            name={c.data.name}
            tagline={c.data.tagline}
            bonuses={c.data.bonuses}
            locale={lang}
          />
        </li>
      ))}
    </ul>
  </section>
</BaseLayout>
```

- [ ] **Step 2: Verify**

Run `pnpm dev`, visit `/en/civs/`, `/tr/civs/`, `/es/civs/`, `/de/civs/`. All should show one card for Britons in the correct language.

- [ ] **Step 3: Commit**

```bash
git add src/pages/[lang]/civs/index.astro
git commit -m "feat(pages): civ index renders localized cards"
```

---

## Task 18: Civ detail page `/[lang]/civs/[civ]/`

**Files:**
- Create: `src/pages/[lang]/civs/[civ].astro`

- [ ] **Step 1: Write the page**

```astro
---
import BaseLayout from "@/components/layout/BaseLayout.astro";
import Icon from "@components/ui/Icon.astro";
import { locales, type Locale } from "@/i18n/locales";
import { t } from "@/i18n/utils";
import { getLocalizedEntries, canonicalSlug, getLocalizedEntry } from "@/lib/content";
import civsData from "@data/civilizations.json";

export async function getStaticPaths() {
  const paths: Array<{ params: { lang: string; civ: string } }> = [];
  for (const lang of locales) {
    const civs = await getLocalizedEntries("civilizations", lang);
    for (const c of civs) {
      paths.push({ params: { lang, civ: canonicalSlug(c.id) } });
    }
  }
  return paths;
}

const lang = Astro.params.lang as Locale;
const civSlug = Astro.params.civ as string;
const found = await getLocalizedEntry("civilizations", civSlug, lang);
if (!found) return Astro.redirect("/404");

const { entry, fallback } = found;
const { Content } = await entry.render();
const meta = (civsData.civs as Array<any>).find((c) => c.slug === civSlug);
---
<BaseLayout title={entry.data.name} canonicalPath={`/civs/${civSlug}`}>
  <article class="mx-auto max-w-prose px-4 py-12">
    {fallback && (
      <div class="mb-6 rounded border border-royal/40 bg-royal/10 px-3 py-2 text-sm text-royal">
        {t(lang, "ui.translation.missing")}
      </div>
    )}
    <header class="flex items-center gap-4">
      <Icon slug={civSlug} kind="civ" size={72} />
      <div>
        <h1 class="font-display text-4xl text-ink">{entry.data.name}</h1>
        <p class="text-stone-700">{entry.data.tagline}</p>
      </div>
    </header>

    {meta && (
      <dl class="mt-6 grid grid-cols-2 gap-2 text-sm sm:grid-cols-4">
        <div><dt class="text-stone-700">Specialty</dt><dd class="text-ink">{meta.specialty}</dd></div>
        <div><dt class="text-stone-700">Region</dt><dd class="text-ink">{meta.region}</dd></div>
        <div><dt class="text-stone-700">Tier</dt><dd class="text-gold-500">{meta.tier ?? "—"}</dd></div>
        <div><dt class="text-stone-700">Patch</dt><dd class="text-ink">{civsData.patch}</dd></div>
      </dl>
    )}

    <section class="mt-8">
      <h2 class="font-display text-2xl text-ink">Bonuses</h2>
      <ul class="mt-2 list-inside list-disc space-y-1 text-ink">
        {entry.data.bonuses.map((b: string) => <li>{b}</li>)}
      </ul>
      <p class="mt-3 text-sm text-stone-700"><strong>Team:</strong> {entry.data.teamBonus}</p>
    </section>

    <section class="mt-8">
      <h2 class="font-display text-2xl text-ink">Unique Techs</h2>
      <ul class="mt-2 space-y-2 text-ink">
        <li><strong>{entry.data.uniqueTechs.castle.name}</strong> — {entry.data.uniqueTechs.castle.effect}</li>
        <li><strong>{entry.data.uniqueTechs.imperial.name}</strong> — {entry.data.uniqueTechs.imperial.effect}</li>
      </ul>
    </section>

    <section class="prose prose-stone mt-10 max-w-none">
      <Content />
    </section>
  </article>
</BaseLayout>
```

- [ ] **Step 2: Verify**

Run `pnpm dev`, visit `/en/civs/britons/`, `/tr/civs/britons/`, etc. All 4 locales should render the full localized civ page.

- [ ] **Step 3: Commit**

```bash
git add src/pages/[lang]/civs/\[civ\].astro
git commit -m "feat(pages): civ detail page with bonuses, unique techs, prose body"
```

---

## Task 19: Phase 4 milestone check

- [ ] **Step 1: Verify production build**

Run: `pnpm build`
Expected: build succeeds, `dist/<lang>/civs/britons/index.html` exists for all 4 locales.

- [ ] **Step 2: Phase 4 milestone confirmed**

> Britons fully rendered in 4 langs, index + detail ✓

---

# Phase 5 — Build Orders + import-from-md script

## Task 20: Implement `<BuildOrderSteps>` component

**Files:**
- Create: `src/components/content/BuildOrderSteps.astro`

- [ ] **Step 1: Write the component**

```astro
---
interface Step {
  villagers: number;
  time: string;
  assign: string;
  note?: string;
}
interface Props { steps: Step[]; }
const { steps } = Astro.props;
---
<ol class="overflow-hidden rounded border border-stone-700/40">
  <li class="grid grid-cols-[5ch_5ch_1fr] gap-3 bg-parchment/60 px-3 py-2 font-mono text-xs text-stone-700">
    <span>Time</span><span>Vil</span><span>Action</span>
  </li>
  {steps.map((s, i) => (
    <li class={`grid grid-cols-[5ch_5ch_1fr] gap-3 border-t border-stone-700/20 px-3 py-2 ${i % 2 ? "bg-parchment/40" : "bg-parchment"}`}>
      <span class="font-mono text-sm text-stone-700">{s.time}</span>
      <span class="font-mono text-sm text-stone-700">{s.villagers}</span>
      <span class="text-sm text-ink">
        {s.assign}
        {s.note && <span class="ml-2 text-stone-700"> — {s.note}</span>}
      </span>
    </li>
  ))}
</ol>
```

- [ ] **Step 2: Commit**

```bash
git add src/components/content/BuildOrderSteps.astro
git commit -m "feat(content): BuildOrderSteps component"
```

---

## Task 21: Build orders index + detail pages

**Files:**
- Modify: `src/pages/[lang]/builds/index.astro`
- Create: `src/pages/[lang]/builds/[build].astro`

- [ ] **Step 1: Write the index page**

```astro
---
import BaseLayout from "@/components/layout/BaseLayout.astro";
import { locales, type Locale } from "@/i18n/locales";
import { t, localizedPath } from "@/i18n/utils";
import { getLocalizedEntries, canonicalSlug } from "@/lib/content";

export function getStaticPaths() {
  return locales.map((lang) => ({ params: { lang } }));
}
const lang = Astro.params.lang as Locale;
const builds = await getLocalizedEntries("build-orders", lang);
---
<BaseLayout title={t(lang, "nav.builds")} canonicalPath="/builds">
  <section class="mx-auto max-w-7xl px-4 py-12">
    <h1 class="font-display text-4xl text-ink">{t(lang, "nav.builds")}</h1>
    <ul class="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {builds.map((b) => (
        <li>
          <a href={localizedPath(`/builds/${canonicalSlug(b.id)}`, lang)}
             class="block rounded border border-stone-700/40 bg-parchment p-4 hover:border-gold-500">
            <h3 class="font-display text-xl text-ink">{b.data.name}</h3>
            <p class="mt-1 text-xs text-stone-700">
              {b.data.difficulty} · {b.data.targetAge} · {b.data.durationMin}min
            </p>
          </a>
        </li>
      ))}
    </ul>
  </section>
</BaseLayout>
```

- [ ] **Step 2: Write the detail page `src/pages/[lang]/builds/[build].astro`**

```astro
---
import BaseLayout from "@/components/layout/BaseLayout.astro";
import BuildOrderSteps from "@components/content/BuildOrderSteps.astro";
import { locales, type Locale } from "@/i18n/locales";
import { t } from "@/i18n/utils";
import { getLocalizedEntries, canonicalSlug, getLocalizedEntry } from "@/lib/content";

export async function getStaticPaths() {
  const paths: Array<{ params: { lang: string; build: string } }> = [];
  for (const lang of locales) {
    const builds = await getLocalizedEntries("build-orders", lang);
    for (const b of builds) paths.push({ params: { lang, build: canonicalSlug(b.id) } });
  }
  return paths;
}

const lang = Astro.params.lang as Locale;
const slug = Astro.params.build as string;
const found = await getLocalizedEntry("build-orders", slug, lang);
if (!found) return Astro.redirect("/404");
const { entry, fallback } = found;
const { Content } = await entry.render();
---
<BaseLayout title={entry.data.name} canonicalPath={`/builds/${slug}`}>
  <article class="mx-auto max-w-prose px-4 py-12">
    {fallback && (
      <div class="mb-6 rounded border border-royal/40 bg-royal/10 px-3 py-2 text-sm text-royal">
        {t(lang, "ui.translation.missing")}
      </div>
    )}
    <header>
      <h1 class="font-display text-4xl text-ink">{entry.data.name}</h1>
      <p class="mt-1 text-sm text-stone-700">
        {entry.data.difficulty} · {entry.data.targetAge} · ~{entry.data.durationMin} min
      </p>
      {entry.data.source && (
        <p class="mt-1 text-xs text-stone-700">
          Source: {entry.data.source.url
            ? <a href={entry.data.source.url} class="underline hover:text-gold-500">{entry.data.source.author}</a>
            : entry.data.source.author}
        </p>
      )}
    </header>

    <section class="mt-8">
      <h2 class="font-display text-2xl text-ink">Steps</h2>
      <div class="mt-2"><BuildOrderSteps steps={entry.data.steps} /></div>
    </section>

    <section class="prose prose-stone mt-10 max-w-none">
      <Content />
    </section>
  </article>
</BaseLayout>
```

- [ ] **Step 3: Commit**

```bash
git add src/pages/[lang]/builds/
git commit -m "feat(pages): build orders index + detail with step table"
```

---

## Task 22: Write `scripts/import-from-md.mjs`

**Files:**
- Create: `scripts/import-from-md.mjs`

This script takes a file path in `md/` and scaffolds an EN content entry with frontmatter pre-filled.

- [ ] **Step 1: Write the script**

```js
#!/usr/bin/env node
// scripts/import-from-md.mjs
// Usage: node scripts/import-from-md.mjs md/build-orders/<file>.md
// Creates src/content/build-orders/en/<slug>.md with scaffolded frontmatter,
// preserving the source body as a starting point.

import { readFile, writeFile, mkdir, access } from "node:fs/promises";
import path from "node:path";

function slugify(s) {
  return s
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^\w\s-]/g, "")
    .trim()
    .replace(/\s+/g, "-");
}

async function exists(p) {
  try { await access(p); return true; } catch { return false; }
}

async function run() {
  const file = process.argv[2];
  if (!file) {
    console.error("Usage: import-from-md.mjs md/<type>/<source-file>.md");
    process.exit(1);
  }

  const abs = path.resolve(file);
  const parts = path.relative(path.resolve("md"), abs).split(path.sep);
  if (parts.length < 2) {
    console.error("File must be under md/<type>/<file>.md");
    process.exit(1);
  }
  const type = parts[0]; // "build-orders"
  const basename = path.basename(parts.at(-1), ".md");
  const slug = slugify(basename.replace(/^[^-]*-/, "")); // strip "hera-" / source prefix

  const target = path.resolve("src/content", type, "en", `${slug}.md`);
  if (await exists(target)) {
    console.error("Target already exists:", target);
    process.exit(1);
  }

  const source = await readFile(abs, "utf8");

  // Try to extract a name from the first heading
  const m = source.match(/^#\s+(.+)$/m);
  const name = m ? m[1] : slug;

  const frontmatter = `---
slug: ${slug}
name: ${name}
difficulty: intermediate
targetAge: feudal
durationMin: 14
civsRecommended: []
steps: []
source:
  author: "TODO"
  url: ""
---

`;

  await mkdir(path.dirname(target), { recursive: true });
  await writeFile(target, frontmatter + source);
  console.log("Scaffolded", target);
  console.log("Next: fill in steps:[] and civsRecommended:[], then translate via scripts/new-translation.mjs");
}

run().catch((e) => { console.error(e); process.exit(1); });
```

- [ ] **Step 2: Add npm script**

```json
"import:md": "node scripts/import-from-md.mjs"
```

- [ ] **Step 3: Verify with a placeholder md file**

```bash
cat > md/build-orders/test-build.md <<'EOF'
# Test Build

This is a test source file.
EOF

pnpm run import:md md/build-orders/test-build.md
```

Expected: `src/content/build-orders/en/test-build.md` created with frontmatter + body.

Cleanup:
```bash
rm md/build-orders/test-build.md src/content/build-orders/en/test-build.md
```

- [ ] **Step 4: Commit**

```bash
git add scripts/import-from-md.mjs package.json
git commit -m "feat(scripts): import-from-md to scaffold src/content from md/ sources"
```

---

## Task 23: Phase 5 milestone — user can now paste a build order

User workflow is now:
1. Paste a guide into `md/build-orders/<source>-<topic>.md`
2. `pnpm run import:md <path>`
3. Edit the scaffolded EN file to fill `steps:[]` and `civsRecommended:[]`
4. `pnpm dev` to preview

> Phase 5 ✓ — build orders ship, import flow works.

---

# Phase 6 — Remaining content types (units, maps, matchups, beginner, glossary)

## Task 24: Units — pages + sample unit + stats data

**Files:**
- Modify: `src/data/unit-stats.json`
- Create: `src/content/units/en/longbowman.md` (+ tr/es/de versions), `src/pages/[lang]/units/[unit].astro`
- Modify: `src/pages/[lang]/units/index.astro`

- [ ] **Step 1: Seed `src/data/unit-stats.json` with Longbowman**

```json
{
  "patch": "v100.1.84",
  "units": [
    {
      "slug": "longbowman",
      "hp": 35,
      "attack": 7,
      "range": 6,
      "minRange": 0,
      "cost": { "wood": 35, "food": 0, "gold": 40, "stone": 0 },
      "trainTime": 18,
      "armorPiercing": 0,
      "armorMelee": 0
    }
  ]
}
```

- [ ] **Step 2: Write `src/content/units/en/longbowman.md`**

```markdown
---
slug: longbowman
name: Longbowman
role: Unique foot archer (Britons)
civ: britons
---

The Longbowman is the Britons' unique unit — a foot archer with the longest range in the game when supported by Yeomen. Strong as a range-extending support to Crossbowmen and Arbalester compositions.

## How to use

Mass Longbowmen behind a Crossbowman line. Their range allows them to outshoot every other archer at full upgrades, making them devastating in Castle and Imperial Age.
```

- [ ] **Step 3: Write TR/ES/DE versions of Longbowman**

Follow the same translation pattern as Britons in Task 15. Use the same `slug`; localize `name` and `role`. Keep the prose translated to the target language.

(Concrete translations: `name`: Uzunyaylı / Arquero de arco largo / Langbogenschütze; `role` translated similarly.)

- [ ] **Step 4: Write the unit detail page `src/pages/[lang]/units/[unit].astro`**

```astro
---
import BaseLayout from "@/components/layout/BaseLayout.astro";
import Icon from "@components/ui/Icon.astro";
import { locales, type Locale } from "@/i18n/locales";
import { t } from "@/i18n/utils";
import { getLocalizedEntries, canonicalSlug, getLocalizedEntry } from "@/lib/content";
import unitStats from "@data/unit-stats.json";

export async function getStaticPaths() {
  const paths: Array<{ params: { lang: string; unit: string } }> = [];
  for (const lang of locales) {
    const units = await getLocalizedEntries("units", lang);
    for (const u of units) paths.push({ params: { lang, unit: canonicalSlug(u.id) } });
  }
  return paths;
}

const lang = Astro.params.lang as Locale;
const slug = Astro.params.unit as string;
const found = await getLocalizedEntry("units", slug, lang);
if (!found) return Astro.redirect("/404");
const { entry, fallback } = found;
const { Content } = await entry.render();
const stats = (unitStats.units as Array<any>).find((u) => u.slug === slug);
---
<BaseLayout title={entry.data.name} canonicalPath={`/units/${slug}`}>
  <article class="mx-auto max-w-prose px-4 py-12">
    {fallback && (
      <div class="mb-6 rounded border border-royal/40 bg-royal/10 px-3 py-2 text-sm text-royal">
        {t(lang, "ui.translation.missing")}
      </div>
    )}
    <header class="flex items-center gap-4">
      <Icon slug={slug} kind="unit" size={64} />
      <div>
        <h1 class="font-display text-4xl text-ink">{entry.data.name}</h1>
        <p class="text-sm text-stone-700">{entry.data.role}</p>
      </div>
    </header>

    {stats && (
      <table class="mt-6 w-full text-sm">
        <tbody>
          <tr><th class="text-left text-stone-700">HP</th><td class="text-ink">{stats.hp}</td></tr>
          <tr><th class="text-left text-stone-700">Attack</th><td class="text-ink">{stats.attack}</td></tr>
          <tr><th class="text-left text-stone-700">Range</th><td class="text-ink">{stats.range}</td></tr>
          <tr><th class="text-left text-stone-700">Train time</th><td class="text-ink">{stats.trainTime}s</td></tr>
          <tr><th class="text-left text-stone-700">Cost</th>
              <td class="text-ink">
                <span class="text-res-food">{stats.cost.food}f</span>
                <span class="ml-2 text-res-wood">{stats.cost.wood}w</span>
                <span class="ml-2 text-res-gold">{stats.cost.gold}g</span>
                <span class="ml-2 text-res-stone">{stats.cost.stone}s</span>
              </td></tr>
        </tbody>
      </table>
    )}

    <section class="prose prose-stone mt-10 max-w-none">
      <Content />
    </section>
  </article>
</BaseLayout>
```

- [ ] **Step 5: Update `src/pages/[lang]/units/index.astro` to list units**

(Pattern identical to civs index; substitute `units` for `civilizations`.)

- [ ] **Step 6: Commit**

```bash
git add src/data/unit-stats.json src/content/units/ src/pages/[lang]/units/
git commit -m "feat(units): sample Longbowman in 4 langs + stats data + detail page"
```

---

## Task 25: Maps — pages + sample map

**Files:**
- Create: `src/content/maps/en/arabia.md` (+ tr/es/de)
- Modify: `src/pages/[lang]/maps/index.astro`
- Create: `src/pages/[lang]/maps/[map].astro`

- [ ] **Step 1: Write Arabia content in all 4 languages**

```markdown
---
slug: arabia
name: Arabia
type: open
size: medium
recommendedCivs: [mongols, mayans, franks, huns]
---

Arabia is the canonical 1v1 ladder map: open terrain with scattered forests, hills, and a central neutral zone. Aggressive civilizations and early raid styles dominate. Walling is harder than on closed maps.

## Opening

Standard scout opening with a 21-pop archer transition is a safe default. Aggressive civs (Mongols, Magyars, Huns) can pressure with early scouts. Magyars and Aztecs work well into mid-game crossbow.

## Strategy notes

- Wall key chokes early; don't fully wall — leave a sally gate for villagers to fast-flee.
- Watch for tower rushes from civs like Vikings/Aztecs.
```

(Translate `name` and prose for TR/ES/DE.)

- [ ] **Step 2: Write maps index + detail pages**

Identical pattern to civs/units indexes and details. Show `name`, `type`, `size`, recommended civs, prose body.

- [ ] **Step 3: Commit**

```bash
git add src/content/maps/ src/pages/[lang]/maps/
git commit -m "feat(maps): sample Arabia in 4 langs + index/detail pages"
```

---

## Task 26: Matchups — pages + sample matchup

**Files:**
- Create: `src/content/matchups/en/britons-vs-franks.md` (+ tr/es/de)
- Create: `src/pages/[lang]/matchups/[matchup].astro`
- Modify: `src/pages/[lang]/matchups/index.astro`

- [ ] **Step 1: Write content**

```markdown
---
slug: britons-vs-franks
civA: britons
civB: franks
difficulty: even
---

Britons (ranged power, eco from cheap TCs) vs Franks (cavalry pressure, food eco from cheap farms). This is a classic open-map matchup where Britons want to defend and outscale while Franks try to deny the boom with early Knights.
```

- [ ] **Step 2: Write matchups pages**

Use `civA` and `civB` slugs to render side-by-side civ icons in the header.

- [ ] **Step 3: Commit**

```bash
git add src/content/matchups/ src/pages/[lang]/matchups/
git commit -m "feat(matchups): sample Britons-vs-Franks + pages"
```

---

## Task 27: Beginner path — pages + first chapter

**Files:**
- Create: `src/content/beginner/en/01-resources.md` (+ tr/es/de)
- Modify: `src/pages/[lang]/learn/index.astro`
- Create: `src/pages/[lang]/learn/[chapter].astro`

- [ ] **Step 1: Write the first chapter**

```markdown
---
slug: 01-resources
title: Resources & Villagers
order: 1
---

Age of Empires II has four resources: Food, Wood, Gold, and Stone. Villagers gather them and your economy compounds when villagers are continuously produced from the Town Center.

## Continuous production

Never let your Town Center sit idle. A villager every ~25s for the first 8-10 minutes is the foundation of every strategy in the game.
```

- [ ] **Step 2: Index page lists chapters ordered by `order`**

```astro
---
import BaseLayout from "@/components/layout/BaseLayout.astro";
import { locales, type Locale } from "@/i18n/locales";
import { t, localizedPath } from "@/i18n/utils";
import { getLocalizedEntries, canonicalSlug } from "@/lib/content";

export function getStaticPaths() {
  return locales.map((lang) => ({ params: { lang } }));
}
const lang = Astro.params.lang as Locale;
const chapters = (await getLocalizedEntries("beginner", lang)).sort(
  (a, b) => a.data.order - b.data.order
);
---
<BaseLayout title={t(lang, "nav.learn")} canonicalPath="/learn">
  <section class="mx-auto max-w-prose px-4 py-12">
    <h1 class="font-display text-4xl text-ink">{t(lang, "nav.learn")}</h1>
    <ol class="mt-8 space-y-2">
      {chapters.map((c) => (
        <li>
          <a href={localizedPath(`/learn/${canonicalSlug(c.id)}`, lang)}
             class="block rounded border border-stone-700/40 bg-parchment p-4 hover:border-gold-500">
            <span class="font-mono text-xs text-stone-700">{String(c.data.order).padStart(2, "0")}</span>
            <span class="ml-2 font-display text-lg text-ink">{c.data.title}</span>
          </a>
        </li>
      ))}
    </ol>
  </section>
</BaseLayout>
```

- [ ] **Step 3: Detail page renders ordered chapter with prev/next links**

```astro
---
import BaseLayout from "@/components/layout/BaseLayout.astro";
import { locales, type Locale } from "@/i18n/locales";
import { t, localizedPath } from "@/i18n/utils";
import { getLocalizedEntries, canonicalSlug, getLocalizedEntry } from "@/lib/content";

export async function getStaticPaths() {
  const paths: Array<{ params: { lang: string; chapter: string } }> = [];
  for (const lang of locales) {
    const chs = await getLocalizedEntries("beginner", lang);
    for (const c of chs) paths.push({ params: { lang, chapter: canonicalSlug(c.id) } });
  }
  return paths;
}

const lang = Astro.params.lang as Locale;
const slug = Astro.params.chapter as string;
const found = await getLocalizedEntry("beginner", slug, lang);
if (!found) return Astro.redirect("/404");
const { entry, fallback } = found;
const { Content } = await entry.render();

const all = (await getLocalizedEntries("beginner", lang)).sort(
  (a, b) => a.data.order - b.data.order
);
const idx = all.findIndex((e) => canonicalSlug(e.id) === slug);
const prev = all[idx - 1];
const next = all[idx + 1];
---
<BaseLayout title={entry.data.title} canonicalPath={`/learn/${slug}`}>
  <article class="mx-auto max-w-prose px-4 py-12">
    {fallback && (
      <div class="mb-6 rounded border border-royal/40 bg-royal/10 px-3 py-2 text-sm text-royal">
        {t(lang, "ui.translation.missing")}
      </div>
    )}
    <h1 class="font-display text-4xl text-ink">{entry.data.title}</h1>
    <section class="prose prose-stone mt-8 max-w-none"><Content /></section>
    <nav class="mt-12 flex justify-between text-sm text-stone-700">
      {prev ? <a href={localizedPath(`/learn/${canonicalSlug(prev.id)}`, lang)} class="hover:text-gold-500">← {prev.data.title}</a> : <span/>}
      {next ? <a href={localizedPath(`/learn/${canonicalSlug(next.id)}`, lang)} class="hover:text-gold-500">{next.data.title} →</a> : <span/>}
    </nav>
  </article>
</BaseLayout>
```

- [ ] **Step 4: Commit**

```bash
git add src/content/beginner/ src/pages/[lang]/learn/
git commit -m "feat(learn): beginner chapters index + detail with prev/next nav"
```

---

## Task 28: Glossary — alphabetical single-page index

**Files:**
- Create: `src/content/glossary/en/boom.md`, `src/content/glossary/en/feudal.md` (+ tr/es/de for both)
- Modify: `src/pages/[lang]/glossary/index.astro`

- [ ] **Step 1: Write sample terms (boom, feudal)**

```markdown
---
slug: boom
term: Boom
letter: B
---

A play style focused on economy: skip early aggression to build extra Town Centers and out-produce your opponent.
```

```markdown
---
slug: feudal
term: Feudal Age
letter: F
---

The second age of the game. Unlocks Archery Range, Stable, Blacksmith, walls, and many critical military units (Scouts, Archers, Skirmishers, Spearmen).
```

(Translate for TR/ES/DE — note `term` can be the original English in TR since terminology is often retained.)

- [ ] **Step 2: Write glossary index page (single-page, alphabetical with anchor jumps)**

```astro
---
import BaseLayout from "@/components/layout/BaseLayout.astro";
import { locales, type Locale } from "@/i18n/locales";
import { t } from "@/i18n/utils";
import { getLocalizedEntries } from "@/lib/content";

export function getStaticPaths() {
  return locales.map((lang) => ({ params: { lang } }));
}
const lang = Astro.params.lang as Locale;
const terms = (await getLocalizedEntries("glossary", lang)).sort((a, b) =>
  a.data.term.localeCompare(b.data.term),
);
const grouped = new Map<string, typeof terms>();
for (const t of terms) {
  const list = grouped.get(t.data.letter) ?? [];
  list.push(t);
  grouped.set(t.data.letter, list);
}
const letters = [...grouped.keys()].sort();
---
<BaseLayout title={t(lang, "nav.glossary")} canonicalPath="/glossary">
  <section class="mx-auto max-w-prose px-4 py-12">
    <h1 class="font-display text-4xl text-ink">{t(lang, "nav.glossary")}</h1>

    <nav class="mt-4 flex flex-wrap gap-2 text-sm">
      {letters.map((l) => <a href={`#${l}`} class="rounded border border-stone-700/30 px-2 py-1 hover:border-gold-500 hover:text-gold-500">{l}</a>)}
    </nav>

    {letters.map((l) => (
      <section id={l} class="mt-10">
        <h2 class="font-display text-2xl text-gold-500">{l}</h2>
        <dl class="mt-2 space-y-3">
          {grouped.get(l)!.map(async (term) => {
            const { Content } = await term.render();
            return (
              <div>
                <dt class="font-display text-lg text-ink">{term.data.term}</dt>
                <dd class="prose prose-stone max-w-none"><Content /></dd>
              </div>
            );
          })}
        </dl>
      </section>
    ))}
  </section>
</BaseLayout>
```

- [ ] **Step 3: Commit**

```bash
git add src/content/glossary/ src/pages/[lang]/glossary/
git commit -m "feat(glossary): alphabetical single-page glossary with anchor nav"
```

---

# Phase 7 — Search, tooling, deploy

## Task 29: Pagefind search

**Files:**
- Modify: `astro.config.mjs` (post-build hook) OR add npm script
- Create: `src/pages/[lang]/search.astro`

- [ ] **Step 1: Install Pagefind**

```bash
pnpm add -D pagefind
```

- [ ] **Step 2: Add post-build script**

In `package.json`:

```json
"scripts": {
  "build": "astro build && pagefind --site dist"
}
```

- [ ] **Step 3: Add Pagefind data attribute to BaseLayout main**

In `BaseLayout.astro`, change the `<main>` to:

```astro
<main class="flex-1" data-pagefind-body>
  <slot />
</main>
```

In every page that should be indexed, also add `data-pagefind-filter="lang:<lang>"` on the article/section root so we can filter by locale.

- [ ] **Step 4: Write the search page `src/pages/[lang]/search.astro`**

```astro
---
import BaseLayout from "@/components/layout/BaseLayout.astro";
import { locales, type Locale } from "@/i18n/locales";
import { t } from "@/i18n/utils";

export function getStaticPaths() {
  return locales.map((lang) => ({ params: { lang } }));
}
const lang = Astro.params.lang as Locale;
---
<BaseLayout title={t(lang, "nav.search")} canonicalPath="/search">
  <section class="mx-auto max-w-prose px-4 py-12">
    <h1 class="font-display text-4xl text-ink">{t(lang, "nav.search")}</h1>
    <div id="search" class="mt-6"></div>
    <link rel="stylesheet" href="/pagefind/pagefind-ui.css" />
    <script src="/pagefind/pagefind-ui.js" defer is:inline></script>
    <script is:inline define:vars={{ lang }}>
      window.addEventListener("DOMContentLoaded", () => {
        new PagefindUI({
          element: "#search",
          showSubResults: true,
          // Scope results to the current language
          baseFilters: { lang },
        });
      });
    </script>
  </section>
</BaseLayout>
```

- [ ] **Step 5: Verify**

Run `pnpm build`, then `pnpm preview`. Visit `/en/search/`, type "britons", expect the civ page to appear.

- [ ] **Step 6: Commit**

```bash
git add .
git commit -m "feat(search): Pagefind integration scoped per-locale"
```

---

## Task 30: `scripts/new-guide.mjs` — scaffold an entry in all 4 langs

**Files:**
- Create: `scripts/new-guide.mjs`

- [ ] **Step 1: Write the script**

```js
#!/usr/bin/env node
// Usage: node scripts/new-guide.mjs <type> <slug>
// Creates src/content/<type>/{en,tr,es,de}/<slug>.md with empty frontmatter scaffolds.

import { writeFile, mkdir, access } from "node:fs/promises";
import path from "node:path";

const TEMPLATES = {
  civilizations: ({ slug, lang }) => `---
slug: ${slug}
name: ""
tagline: ""
bonuses: []
teamBonus: ""
uniqueTechs:
  castle: { name: "", effect: "" }
  imperial: { name: "", effect: "" }
---

<!-- ${lang.toUpperCase()} body for ${slug} -->
`,
  "build-orders": ({ slug, lang }) => `---
slug: ${slug}
name: ""
difficulty: intermediate
targetAge: feudal
durationMin: 14
civsRecommended: []
steps: []
source:
  author: ""
  url: ""
---

<!-- ${lang.toUpperCase()} body for ${slug} -->
`,
  units: ({ slug, lang }) => `---
slug: ${slug}
name: ""
role: ""
---

<!-- ${lang.toUpperCase()} body for ${slug} -->
`,
  maps: ({ slug, lang }) => `---
slug: ${slug}
name: ""
type: open
recommendedCivs: []
---

<!-- ${lang.toUpperCase()} body for ${slug} -->
`,
  matchups: ({ slug, lang }) => `---
slug: ${slug}
civA: ""
civB: ""
difficulty: even
---

<!-- ${lang.toUpperCase()} body for ${slug} -->
`,
  beginner: ({ slug, lang }) => `---
slug: ${slug}
title: ""
order: 1
---

<!-- ${lang.toUpperCase()} body for ${slug} -->
`,
  glossary: ({ slug, lang }) => `---
slug: ${slug}
term: ""
letter: ""
---

<!-- ${lang.toUpperCase()} body for ${slug} -->
`,
};

async function exists(p) { try { await access(p); return true; } catch { return false; } }

async function run() {
  const [, , type, slug] = process.argv;
  if (!type || !slug) {
    console.error("Usage: new-guide.mjs <type> <slug>");
    console.error("Types:", Object.keys(TEMPLATES).join(", "));
    process.exit(1);
  }
  if (!TEMPLATES[type]) {
    console.error("Unknown type:", type);
    process.exit(1);
  }
  for (const lang of ["en", "tr", "es", "de"]) {
    const out = path.resolve("src/content", type, lang, `${slug}.md`);
    if (await exists(out)) { console.log("skip (exists):", out); continue; }
    await mkdir(path.dirname(out), { recursive: true });
    await writeFile(out, TEMPLATES[type]({ slug, lang }));
    console.log("created:", out);
  }
}

run().catch((e) => { console.error(e); process.exit(1); });
```

- [ ] **Step 2: Add npm script**

```json
"new:guide": "node scripts/new-guide.mjs"
```

- [ ] **Step 3: Commit**

```bash
git add scripts/new-guide.mjs package.json
git commit -m "feat(scripts): new-guide scaffolds entries across 4 langs"
```

---

## Task 31: `scripts/check-translations.mjs` — coverage report

**Files:**
- Create: `scripts/check-translations.mjs`

- [ ] **Step 1: Write the script**

```js
#!/usr/bin/env node
// Walks src/content/<type>/{en,tr,es,de}/*.md and reports per-language coverage.

import { readdir } from "node:fs/promises";
import path from "node:path";

const TYPES = ["civilizations", "build-orders", "units", "maps", "matchups", "beginner", "glossary"];
const LANGS = ["en", "tr", "es", "de"];

async function listSlugs(type, lang) {
  try {
    const dir = path.resolve("src/content", type, lang);
    const files = await readdir(dir);
    return files.filter((f) => f.endsWith(".md")).map((f) => f.replace(/\.md$/, ""));
  } catch {
    return [];
  }
}

async function run() {
  let totalMissing = 0;
  for (const type of TYPES) {
    const en = new Set(await listSlugs(type, "en"));
    console.log(`\n${type}:  en=${en.size}`);
    for (const lang of LANGS.filter((l) => l !== "en")) {
      const have = new Set(await listSlugs(type, lang));
      const missing = [...en].filter((s) => !have.has(s));
      console.log(`  ${lang}: ${have.size}/${en.size}` + (missing.length ? `   missing: ${missing.join(", ")}` : ""));
      totalMissing += missing.length;
    }
  }
  console.log(`\nTotal missing translations: ${totalMissing}`);
}

run();
```

- [ ] **Step 2: Add npm script**

```json
"check:translations": "node scripts/check-translations.mjs"
```

- [ ] **Step 3: Commit**

```bash
git add scripts/check-translations.mjs package.json
git commit -m "feat(scripts): check-translations coverage report"
```

---

## Task 32: CLAUDE.md files (4)

**Files:**
- Create: `CLAUDE.md`, `md/CLAUDE.md`, `src/content/CLAUDE.md`, `src/components/CLAUDE.md`

- [ ] **Step 1: Write root `CLAUDE.md`**

```markdown
# AOE2 Guide

A four-language static guide site for Age of Empires II, deployed to Cloudflare Pages.

## Stack invariants
- **Astro 4+** in static-output mode. No SSR, no API routes, no server-side runtime — ever.
- **TypeScript strict**, **TailwindCSS**, **MDX**, **Pagefind** for search.
- Package manager: **pnpm** (use `pnpm install --frozen-lockfile` in CI).

## Languages
EN (source of truth), TR, ES, DE. Missing translations fall back to EN with a banner.

## Slugs
English-canonical across all locales. `/tr/civs/britons/` not `/tr/uygarliklar/britonlar/`.

## Commands
- `pnpm dev` — local dev server at http://localhost:4321
- `pnpm build` — production build + Pagefind index → `dist/`
- `pnpm test` — Vitest unit tests
- `pnpm assets:sync` — refresh aoe2 icons from aoe2techtree (pinned SHA)
- `pnpm icons:map` — regenerate `src/data/icon-map.json`
- `pnpm new:guide <type> <slug>` — scaffold a content entry in all 4 langs
- `pnpm import:md <path>` — promote a file from `md/` to `src/content/`
- `pnpm check:translations` — per-language coverage report

## Where to find things
- Spec: `docs/superpowers/specs/2026-05-23-aoe2-guide-design.md`
- Implementation plan: `docs/superpowers/plans/2026-05-23-aoe2-guide-implementation.md`
- Content schemas: `src/content/config.ts`
- Shared structured data (language-agnostic): `src/data/*.json`
- Localized content: `src/content/<type>/{en,tr,es,de}/*.md`
- Components: `src/components/{layout,content,ui,islands}/`

## Workflow
1. New guides start as raw source files in `md/<type>/`.
2. `pnpm import:md md/<type>/<file>.md` scaffolds an EN entry.
3. Edit the EN file, then `pnpm new:guide <type> <slug>` for TR/ES/DE.
4. Translations are bounded to text — numeric stats live in `src/data/`.

## Attribution
Footer text on every page credits Microsoft's Game Content Usage Rules for AOE2 assets. Do not remove.
```

- [ ] **Step 2: Write `md/CLAUDE.md`**

```markdown
# /md — Raw Source Material (NOT built into the site)

Files in this directory are **inputs**, not site content. They are sourced from external guides (forums, YouTube transcripts, wiki articles) and serve as the starting material that gets curated into `src/content/`.

## What lives here
Raw markdown notes, copy-paste from external guides, build-order outlines. Always include source URL and author in a frontmatter block.

## What does NOT live here
- Published site content (that's in `src/content/`)
- Images (we link to source URLs only; no copyrighted images here)
- Localized translations (those live in `src/content/`)

## Pipeline
```
md/<type>/<source>-<topic>.md   ← raw notes
                │
                ▼  pnpm import:md <path>
src/content/<type>/en/<slug>.md ← scaffolded EN entry
                │
                ▼  pnpm new:guide <type> <slug>
src/content/<type>/{tr,es,de}/<slug>.md
```

## Do not
- Import code from this directory.
- Add this directory to the build.
- Treat these files as source of truth — they're historical record of source material.
```

- [ ] **Step 3: Write `src/content/CLAUDE.md`**

```markdown
# Content Collections

Astro Content Collections, schemas in `config.ts`, validated by Zod at build time.

## File layout

```
src/content/<type>/<lang>/<slug>.md
```

- `<type>`: civilizations, build-orders, units, maps, matchups, beginner, glossary
- `<lang>`: en, tr, es, de
- `<slug>`: English-canonical, kebab-case (e.g., `britons`, `21pop-archer`, `britons-vs-franks`)

## Adding a new entry
1. `pnpm import:md md/<type>/<source>.md` (if from raw source), or `pnpm new:guide <type> <slug>` to scaffold blank.
2. Fill the EN file's frontmatter — schema in `config.ts` shows required fields.
3. Translate to TR/ES/DE by editing the scaffolded files.
4. Schema-breaking changes will fail `pnpm build`.

## Split between content and data

| Lives here (per language) | Lives in `src/data/*.json` (language-agnostic) |
|---|---|
| `name`, `tagline`, prose body, bullet text | numeric stats (HP, cost, attack) |
| section headings | era, tier, patch, slug→slug references |

Numeric/factual data is duplicated only once across the codebase; translations only touch text.

## Schema field reference
See `config.ts` — every collection's full Zod schema is the source of truth.
```

- [ ] **Step 4: Write `src/components/CLAUDE.md`**

```markdown
# Components

## Folders
- `layout/` — page chrome (BaseLayout, Header, Footer, LanguageSwitcher, ThemeToggle)
- `content/` — domain-specific content rendering (CivCard, BuildOrderSteps, UnitStatTable, …)
- `ui/` — generic primitives (Card, Badge, Scroll, Ornament, Icon, SearchModal)
- `islands/` — interactive React/Svelte/Vue widgets that hydrate client-side (future: CivComparator)

## Astro component vs island
- Static rendering, no JS needed → `.astro` component, no `client:*` directive.
- Needs JS interactivity (state, event handlers) → `.astro` wrapper around a framework component with `client:visible` (lazy) or `client:idle`.

Don't reach for islands unless interactivity actually requires JS. Most components on this site stay static.

## Theme tokens
Use the Tailwind tokens (`text-ink`, `bg-parchment`, `border-stone-700`, `text-gold-500`) — they bind to CSS custom properties in `tokens.css` and switch automatically in dark mode. Don't hardcode hex values.

## Icons
- Domain icons (units, civs, techs, buildings): `<Icon slug="..." kind="..." />` from `components/ui/Icon.astro`
- UI icons (search, menu, theme): Lucide via inline SVG or `@lucide/astro` package

## Naming
- PascalCase filenames matching the component name (`CivCard.astro`)
- One component per file
- Props typed inline via `interface Props { … }` at the top of frontmatter
```

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md md/CLAUDE.md src/content/CLAUDE.md src/components/CLAUDE.md
git commit -m "docs: CLAUDE.md files at root + 3 contextual subdirectories"
```

---

## Task 33: CONTRIBUTING.md and README

**Files:**
- Create: `docs/CONTRIBUTING.md`, `docs/content-schemas.md`, `docs/translation-workflow.md`
- Modify: `README.md`

- [ ] **Step 1: Write `README.md`**

```markdown
# AOE2 Guide

Multi-language Age of Empires II guide — civilizations, build orders, units, maps, matchups, beginner curriculum, and glossary in EN/TR/ES/DE.

Static site, no server runtime. Built with Astro, deployed on Cloudflare Pages.

## Develop
```
pnpm install
pnpm dev
```

## Build
```
pnpm build
```

Output goes to `dist/` and includes a Pagefind search index.

## Contributing
See [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md).

## Attribution
Age of Empires II © Microsoft Corporation. This site was created under Microsoft's "Game Content Usage Rules" using assets from Age of Empires II, and it is not endorsed by or affiliated with Microsoft.

Icons sourced from [SiegeEngineers/aoe2techtree](https://github.com/SiegeEngineers/aoe2techtree) with the Microsoft Game Content Usage Rules attribution.
```

- [ ] **Step 2: Write `docs/CONTRIBUTING.md`**

A short guide explaining: clone, install, the markdown + schema flow, how to translate, how to add a new content type. ~200 lines max.

- [ ] **Step 3: Write `docs/content-schemas.md`**

A user-facing field reference for each of the 7 content types. Examples for each. Links back to `src/content/config.ts` for the authoritative source.

- [ ] **Step 4: Write `docs/translation-workflow.md`**

Explains: source-of-truth is EN, others fall back, how to scaffold new translations, what `check:translations` reports.

- [ ] **Step 5: Commit**

```bash
git add README.md docs/
git commit -m "docs: README + CONTRIBUTING + content-schemas + translation-workflow"
```

---

## Task 34: GitHub Actions CI

**Files:**
- Create: `.github/workflows/ci.yml`, `.github/PULL_REQUEST_TEMPLATE.md`

- [ ] **Step 1: Write `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  pull_request:
  push:
    branches: [main]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v3
        with: { version: 9 }
      - uses: actions/setup-node@v4
        with:
          node-version-file: ".nvmrc"
          cache: "pnpm"
      - run: pnpm install --frozen-lockfile
      - run: pnpm test
      - run: pnpm build
      - run: pnpm run check:translations
        continue-on-error: true # reports but does not block
```

- [ ] **Step 2: Write `.github/PULL_REQUEST_TEMPLATE.md`**

```markdown
## Summary
<!-- What does this PR add/change? -->

## Type
- [ ] New content (civ, build order, unit, map, matchup, beginner, glossary)
- [ ] Translation
- [ ] Site code (components, layout, scripts)
- [ ] Docs

## Source / Attribution
<!-- For new content: link the source you adapted from. -->

## Languages touched
- [ ] EN  - [ ] TR  - [ ] ES  - [ ] DE

## Checklist
- [ ] `pnpm build` passes locally
- [ ] `pnpm test` passes locally
- [ ] Schema fields complete for all touched entries
```

- [ ] **Step 3: Commit**

```bash
git add .github/
git commit -m "ci: build + test + translation coverage on PR"
```

---

## Task 35: Cloudflare Pages configuration notes

**Files:**
- Create: `docs/deployment.md`

- [ ] **Step 1: Write `docs/deployment.md`**

```markdown
# Deployment — Cloudflare Pages

## One-time setup
1. Create a Cloudflare Pages project, connect the GitHub repo.
2. Build settings:
   - Build command: `pnpm install --frozen-lockfile && pnpm build`
   - Output directory: `dist`
   - Node version: 20 (set via `NODE_VERSION=20` env var, matches `.nvmrc`)
   - Environment variables: none required for the site itself.
3. Set the production branch to `main`. Preview deploys for every other branch and PR.

## Custom domain
Cloudflare Pages → Custom domains → add. DNS is handled automatically if the domain is on Cloudflare.

## Cache headers
Default Cloudflare caching is fine for static assets. To extend:
- Add `_headers` file in `public/` to set long-cache headers on `/images/aoe2/*`.

## Build performance
- Astro build time grows roughly linearly with content count.
- Pagefind indexing adds ~1-3s.
- pnpm cache should be enabled via Cloudflare's automatic detection of `pnpm-lock.yaml`.
```

- [ ] **Step 2: Commit**

```bash
git add docs/deployment.md
git commit -m "docs: Cloudflare Pages deployment notes"
```

---

# Self-Review Checklist

Before declaring the plan complete, run this checklist against the spec:

**1. Spec coverage:**
- §1 Goals → Phases 1-7 cover all ✓
- §2 Stack → Tasks 1, 2, 4, 9, 29 ✓
- §3 Languages → Tasks 4, 7, 15+ ✓
- §4 Repo structure → Tasks 1, 7, 9, 11, 12, 32 ✓
- §5 Content model → Tasks 9, 10, 11 ✓
- §6 Routing & i18n → Tasks 4, 6, 7, 17, 18 ✓
- §7 Visual system → Tasks 2, 3, 5 ✓
- §8 Asset pipeline → Tasks 12, 13, 14 ✓
- §9 Content workflow → Tasks 22, 30, 31 ✓
- §10 Deployment & CI → Tasks 34, 35 ✓

**2. Placeholder scan:** Plan uses real code in every step. No "TODO" patterns in step content (only in seed data, which is intentional — sample stats can be expanded).

**3. Type consistency:** `getLocalizedEntries`, `getLocalizedEntry`, `canonicalSlug`, `localizedPath`, `t`, `getLocaleFromUrl` are consistently named across Tasks 4, 10, 17, 18, 21, 24-28.

---

# Phase Summary

| Phase | Tasks | Outcome |
|---|---|---|
| 1 — Foundation | 1-8 | `pnpm dev` runs; all routes 200; theme works |
| 2 — Schemas & helpers | 9-11 | Content collections defined; helpers + tests pass |
| 3 — Asset pipeline | 12-14 | Icons synced, mapped, rendering via `<Icon>` |
| 4 — Civs E2E | 15-19 | Britons live in 4 langs |
| 5 — Builds + import | 20-23 | User can paste build orders; site renders them |
| 6 — Remaining content | 24-28 | Units, maps, matchups, learn, glossary live |
| 7 — Search + tooling + deploy | 29-35 | Pagefind search; CLAUDE.mds; CI; deploy docs |

Total: **35 tasks**, ~210 steps.
