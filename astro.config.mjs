// astro.config.mjs
import { defineConfig } from "astro/config";
import mdx from "@astrojs/mdx";
import sitemap from "@astrojs/sitemap";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  site: "https://aoe2.example.com",
  output: "static",
  i18n: {
    defaultLocale: "en",
    locales: ["en", "tr", "es", "de"],
    routing: {
      prefixDefaultLocale: true,
      redirectToDefaultLocale: false,
      fallbackType: "rewrite",
    },
    fallback: { tr: "en", es: "en", de: "en" },
  },
  integrations: [mdx(), sitemap()],
  vite: { plugins: [tailwindcss()] },
});
