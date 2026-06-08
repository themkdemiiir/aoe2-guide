// astro.config.mjs

import mdx from "@astrojs/mdx";
import sitemap from "@astrojs/sitemap";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "astro/config";
import icon from "astro-icon";
import pagefind from "astro-pagefind";
import { remarkBuildOrderTruncate } from "./src/lib/remark/build-order-truncate.mjs";

export default defineConfig({
  site: "https://aoe2guide.com",
  output: "static",
  integrations: [
    mdx(),
    sitemap(),
    pagefind(),
    icon({ include: { lucide: ["*"], heroicons: ["*"] } }),
  ],
  markdown: {
    remarkPlugins: [remarkBuildOrderTruncate],
  },
  vite: { plugins: [tailwindcss()] },
});
